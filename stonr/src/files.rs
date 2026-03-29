//! File blob storage, metadata, and upload validation.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    config::{FileKeepMode, Settings},
    event::{Event, Tag},
};

/// File metadata stored alongside a local blob.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlobMeta {
    pub sha256: String,
    pub size: u64,
    pub mime: String,
    pub uploaded_at: u64,
    pub original_name: Option<String>,
    pub owners: HashSet<String>,
    pub refs: HashSet<String>,
    pub expires_at: Option<u64>,
}

/// Lightweight blob description returned by list and upload operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlobInfo {
    pub sha256: String,
    pub size: u64,
    pub mime: String,
    pub url: String,
    pub owners: usize,
    pub refs: usize,
    pub expires_at: Option<u64>,
}

/// File upload request after the HTTP layer has written the temp file.
#[derive(Debug, Clone)]
pub struct UploadCandidate {
    pub temp_path: PathBuf,
    pub filename: Option<String>,
    pub mime: String,
    pub owner: Option<String>,
    pub expires_at: Option<u64>,
}

/// Summary of a blob pruning run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PruneSummary {
    pub removed: usize,
    pub kept: usize,
}

/// File-backed content-addressed blob store.
#[derive(Clone)]
pub struct FileStore {
    root: PathBuf,
}

impl FileStore {
    /// Create a new file store rooted under `root/files`.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Ensure the file storage directory tree exists.
    pub fn init(&self) -> Result<()> {
        for dir in [
            "files/blobs",
            "files/meta",
            "files/refs",
            "files/tmp",
            "files/quarantine",
        ] {
            fs::create_dir_all(self.root.join(dir))?;
        }
        Ok(())
    }

    /// Store an uploaded blob after policy checks pass.
    pub fn store_upload(
        &self,
        candidate: UploadCandidate,
        settings: &Settings,
        base_url: &str,
    ) -> Result<BlobInfo> {
        self.init()?;
        if !settings.file_mime_allowed(&candidate.mime) {
            return Err(anyhow!("blocked: MIME type not allowed"));
        }

        let (hash, size) = hash_file(&candidate.temp_path)?;
        if size > settings.file_max_bytes as u64 {
            return Err(anyhow!("blocked: file exceeds max size"));
        }
        if !settings.file_hash_allowed(&hash) {
            return Err(anyhow!("blocked: file hash denylisted"));
        }

        let existing = self.load_meta(&hash)?;
        if let (Some(owner), Some(limit)) = (
            candidate.owner.as_deref(),
            settings.max_blob_bytes_per_pubkey,
        ) {
            let current_usage = self.owner_usage(owner)?;
            let additional = match &existing {
                Some(meta) if meta.owners.contains(owner) => 0,
                Some(meta) => meta.size,
                None => size,
            };
            if current_usage.saturating_add(additional) > limit {
                return Err(anyhow!("blocked: blob quota exceeded"));
            }
        }

        let blob_path = self.blob_path(&hash);
        if !blob_path.exists() {
            if let Some(parent) = blob_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(&candidate.temp_path, &blob_path)?;
        } else if candidate.temp_path.exists() {
            fs::remove_file(candidate.temp_path)?;
        }

        let mut meta = existing.unwrap_or_else(|| BlobMeta {
            sha256: hash.clone(),
            size,
            mime: candidate.mime.clone(),
            uploaded_at: now_unix(),
            original_name: candidate.filename.clone(),
            owners: HashSet::new(),
            refs: HashSet::new(),
            expires_at: candidate.expires_at,
        });
        meta.size = size;
        meta.mime = candidate.mime;
        meta.expires_at = meta.expires_at.or(candidate.expires_at);
        if meta.original_name.is_none() {
            meta.original_name = candidate.filename;
        }
        if let Some(owner) = candidate.owner {
            meta.owners.insert(owner);
        }
        self.write_meta(&meta)?;
        Ok(blob_info_from_meta(&meta, base_url))
    }

    /// Return file metadata for a given blob hash.
    pub fn load_meta(&self, hash: &str) -> Result<Option<BlobMeta>> {
        let path = self.meta_path(hash);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    /// Return all known blob metadata documents.
    pub fn all_meta(&self) -> Result<Vec<BlobMeta>> {
        let mut metas = Vec::new();
        for entry in walkdir::WalkDir::new(self.root.join("files/meta")) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            metas.push(serde_json::from_str(&fs::read_to_string(entry.path())?)?);
        }
        Ok(metas)
    }

    /// Add a reference from an event id to a blob hash.
    pub fn add_reference(&self, hash: &str, event_id: &str) -> Result<()> {
        let Some(mut meta) = self.load_meta(hash)? else {
            return Ok(());
        };
        meta.refs.insert(event_id.to_string());
        self.write_meta(&meta)?;
        let ref_path = self.ref_path(hash, event_id);
        if let Some(parent) = ref_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(ref_path, event_id)?;
        Ok(())
    }

    /// Add all file/blob references from one event.
    pub fn add_event_references(&self, event: &Event) -> Result<()> {
        for hash in referenced_hashes(event) {
            self.add_reference(&hash, &event.id)?;
        }
        Ok(())
    }

    /// Rebuild blob references from the provided events.
    pub fn rebuild_references(&self, events: &[Event]) -> Result<()> {
        let refs_root = self.root.join("files/refs");
        if refs_root.exists() {
            fs::remove_dir_all(&refs_root)?;
        }
        fs::create_dir_all(&refs_root)?;
        for entry in walkdir::WalkDir::new(self.root.join("files/meta")) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let mut meta: BlobMeta = serde_json::from_str(&fs::read_to_string(entry.path())?)?;
            meta.refs.clear();
            self.write_meta(&meta)?;
        }
        for event in events {
            self.add_event_references(event)?;
        }
        Ok(())
    }

    /// Remove one owner's claim on a blob and delete it if policy allows.
    pub fn delete_for_owner(&self, hash: &str, owner: &str, settings: &Settings) -> Result<bool> {
        let Some(mut meta) = self.load_meta(hash)? else {
            return Ok(false);
        };
        if !meta.owners.remove(owner) {
            return Err(anyhow!("blocked: uploader does not own file"));
        }
        let should_remove = meta.owners.is_empty()
            && meta.refs.is_empty()
            && matches!(settings.file_keep_mode, FileKeepMode::Referenced);
        if should_remove {
            self.remove_blob(hash)?;
            return Ok(true);
        }
        self.write_meta(&meta)?;
        Ok(true)
    }

    /// Prune blobs that are unreferenced, unowned, expired, or denylisted.
    pub fn prune(&self, settings: &Settings, dry_run: bool) -> Result<PruneSummary> {
        let mut summary = PruneSummary::default();
        for entry in walkdir::WalkDir::new(self.root.join("files/meta")) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let meta: BlobMeta = serde_json::from_str(&fs::read_to_string(entry.path())?)?;
            let expired = meta.expires_at.is_some_and(|ts| now_unix() >= ts);
            let unkept = matches!(settings.file_keep_mode, FileKeepMode::Referenced)
                && meta.owners.is_empty()
                && meta.refs.is_empty();
            let denied = !settings.file_hash_allowed(&meta.sha256)
                || !settings.file_mime_allowed(&meta.mime);
            if expired || unkept || denied {
                summary.removed += 1;
                if !dry_run {
                    self.remove_blob(&meta.sha256)?;
                }
            } else {
                summary.kept += 1;
            }
        }
        Ok(summary)
    }

    /// List all blobs in the store.
    pub fn list(&self, base_url: &str) -> Result<Vec<BlobInfo>> {
        let mut blobs = Vec::new();
        for meta in self.all_meta()? {
            blobs.push(blob_info_from_meta(&meta, base_url));
        }
        blobs.sort_by(|left, right| right.sha256.cmp(&left.sha256));
        Ok(blobs)
    }

    /// Open the canonical path for a blob hash.
    pub fn blob_path(&self, hash: &str) -> PathBuf {
        self.root
            .join("files/blobs")
            .join(&hash[0..2.min(hash.len())])
            .join(&hash[2.min(hash.len())..4.min(hash.len())])
            .join(hash)
    }

    /// Directory used for staging uploads.
    pub fn temp_dir(&self) -> PathBuf {
        self.root.join("files/tmp")
    }

    /// Validate a NIP-94 metadata event against local blob state when applicable.
    pub fn validate_metadata_event(&self, event: &Event, settings: &Settings) -> Result<()> {
        if event.kind != 1063 {
            return Ok(());
        }
        if !settings.file_metadata_enabled() {
            return Err(anyhow!("blocked: file metadata disabled"));
        }
        let Some(url) = first_tag_value(event, "url") else {
            return Err(anyhow!("invalid: kind 1063 missing url tag"));
        };
        let Some(hash) = first_tag_value(event, "x").or_else(|| first_tag_value(event, "ox"))
        else {
            return Err(anyhow!("invalid: kind 1063 missing x or ox tag"));
        };
        if !settings.file_hash_allowed(&hash) {
            return Err(anyhow!("blocked: file hash denylisted"));
        }
        let Some(meta) = self.load_meta(&hash)? else {
            if url.contains("/files/") || url.contains(&hash) {
                return Err(anyhow!("invalid: metadata references unknown local blob"));
            }
            return Ok(());
        };
        if let Some(size) =
            first_tag_value(event, "size").and_then(|value| value.parse::<u64>().ok())
        {
            if size != meta.size {
                return Err(anyhow!(
                    "invalid: kind 1063 size tag does not match local blob"
                ));
            }
        }
        if let Some(mime) = first_tag_value(event, "m") {
            if mime != meta.mime {
                return Err(anyhow!(
                    "invalid: kind 1063 mime tag does not match local blob"
                ));
            }
        }
        Ok(())
    }

    fn meta_path(&self, hash: &str) -> PathBuf {
        self.root.join("files/meta").join(format!("{hash}.json"))
    }

    fn ref_path(&self, hash: &str, event_id: &str) -> PathBuf {
        self.root
            .join("files/refs")
            .join(hash)
            .join(format!("{event_id}.ref"))
    }

    fn write_meta(&self, meta: &BlobMeta) -> Result<()> {
        let path = self.meta_path(&meta.sha256);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec(meta)?)?;
        Ok(())
    }

    fn remove_blob(&self, hash: &str) -> Result<()> {
        let blob_path = self.blob_path(hash);
        if blob_path.exists() {
            fs::remove_file(blob_path)?;
        }
        let meta_path = self.meta_path(hash);
        if meta_path.exists() {
            fs::remove_file(meta_path)?;
        }
        let refs_path = self.root.join("files/refs").join(hash);
        if refs_path.exists() {
            fs::remove_dir_all(refs_path)?;
        }
        Ok(())
    }

    fn owner_usage(&self, owner: &str) -> Result<u64> {
        let mut total = 0u64;
        for meta in self.all_meta()? {
            if meta.owners.contains(owner) {
                total = total.saturating_add(meta.size);
            }
        }
        Ok(total)
    }
}

/// Build the standard NIP-94 tag set for a stored blob.
pub fn nip94_tags(info: &BlobInfo) -> Vec<Vec<String>> {
    vec![
        vec!["url".into(), info.url.clone()],
        vec!["m".into(), info.mime.clone()],
        vec!["x".into(), info.sha256.clone()],
        vec!["ox".into(), info.sha256.clone()],
        vec!["size".into(), info.size.to_string()],
    ]
}

/// Parse file references from events.
pub fn referenced_hashes(event: &Event) -> Vec<String> {
    let mut hashes = Vec::new();
    if event.kind == 1063 {
        if let Some(hash) = first_tag_value(event, "x").or_else(|| first_tag_value(event, "ox")) {
            hashes.push(hash);
        }
    }
    for Tag(fields) in &event.tags {
        if fields.first().map(String::as_str) != Some("imeta") {
            continue;
        }
        for field in fields.iter().skip(1) {
            if let Some(value) = field.strip_prefix("x ") {
                hashes.push(value.to_string());
            }
            if let Some(value) = field.strip_prefix("ox ") {
                hashes.push(value.to_string());
            }
        }
    }
    hashes.sort();
    hashes.dedup();
    hashes
}

/// Parse multipart form fields into a flat map.
pub fn parse_text_fields(fields: &[(String, String)]) -> HashMap<String, String> {
    fields.iter().cloned().collect()
}

/// Guess a stable extension for the stored blob.
pub fn blob_extension(meta: &BlobMeta) -> Option<String> {
    meta.original_name
        .as_deref()
        .and_then(|name| Path::new(name).extension().and_then(|ext| ext.to_str()))
        .map(sanitize_extension)
        .filter(|ext| !ext.is_empty())
        .or_else(|| {
            mime_guess::get_mime_extensions_str(&meta.mime)
                .and_then(|values| values.first().copied())
                .map(sanitize_extension)
                .filter(|ext| !ext.is_empty())
        })
}

fn blob_info_from_meta(meta: &BlobMeta, base_url: &str) -> BlobInfo {
    let base_url = base_url.trim_end_matches('/');
    let url = if base_url.ends_with("/files") {
        format!("{base_url}/{}", meta.sha256)
    } else {
        format!("{base_url}/files/{}", meta.sha256)
    };
    BlobInfo {
        sha256: meta.sha256.clone(),
        size: meta.size,
        mime: meta.mime.clone(),
        url,
        owners: meta.owners.len(),
        refs: meta.refs.len(),
        expires_at: meta.expires_at,
    }
}

pub(crate) fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buf = [0u8; 8192];
    loop {
        let read = std::io::Read::read(&mut file, &mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
        size += read as u64;
    }
    Ok((hex::encode(hasher.finalize()), size))
}

fn first_tag_value(event: &Event, name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find_map(|Tag(fields)| match fields.as_slice() {
            [tag, value, ..] if tag == name => Some(value.clone()),
            _ => None,
        })
}

fn sanitize_extension(ext: &str) -> String {
    ext.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn settings(root: &std::path::Path) -> Settings {
        Settings {
            store_root: root.to_path_buf(),
            bind_http: "127.0.0.1:0".into(),
            bind_ws: "127.0.0.1:0".into(),
            verify_sig: false,
            relay_name: "stonr".into(),
            relay_description: "File-backed Nostr relay".into(),
            enable_nip11: true,
            enable_query: true,
            enable_publish: true,
            enable_live_subscriptions: true,
            enable_count: true,
            enable_tag_queries: true,
            enable_search: true,
            enable_mirroring: true,
            allowed_kinds: None,
            blocked_kinds: None,
            allowed_pubkeys: None,
            blocked_pubkeys: None,
            enable_nip42: false,
            require_auth_for_query: false,
            require_auth_for_count: false,
            require_auth_for_publish: false,
            auth_must_match_event_pubkey: false,
            auth_max_age_secs: 600,
            support_nip11: true,
            support_nip09: true,
            support_nip12: true,
            support_nip42: true,
            support_nip40: true,
            support_nip45: true,
            support_nip50: true,
            support_nip94: true,
            support_nip96: true,
            support_nip98: true,
            support_nip_b7: true,
            filter_private_messages: true,
            relays_upstream: vec![],
            tor_socks: None,
            filter_authors: None,
            filter_kinds: None,
            filter_tag_t: None,
            filter_tag_a: None,
            filter_since_mode: crate::config::SinceMode::Cursor,
            mirror_mode: crate::config::MirrorMode::Broad,
            mirror_site_author: None,
            mirror_site_include_comments: true,
            max_stored_events: None,
            max_stored_event_bytes: None,
            max_limit: None,
            max_event_bytes: None,
            max_event_age_secs: None,
            max_event_future_secs: None,
            rate_limit_window_secs: None,
            max_queries_per_window: None,
            max_counts_per_window: None,
            max_publishes_per_window: None,
            enable_file_metadata: true,
            enable_file_api: true,
            enable_blossom: true,
            enable_blossom_list: true,
            enable_blossom_mirror: false,
            require_nip98_auth: false,
            require_blossom_auth: false,
            require_blossom_get_auth: false,
            file_api_url: None,
            blossom_public_url: None,
            file_max_bytes: 32 * 1024 * 1024,
            file_allowed_mime: None,
            file_blocked_mime: None,
            file_hash_denylist: None,
            file_keep_mode: FileKeepMode::Referenced,
            max_blob_bytes_per_pubkey: None,
            owner_pubkeys: None,
            follow_pubkeys: None,
            pinned_event_ids: None,
            protect_pinned_from_deletes: true,
        }
    }

    fn write_candidate(dir: &TempDir, bytes: &[u8], mime: &str) -> UploadCandidate {
        let temp_path = dir.path().join("upload.bin");
        fs::write(&temp_path, bytes).unwrap();
        UploadCandidate {
            temp_path,
            filename: Some("upload.bin".into()),
            mime: mime.into(),
            owner: Some("pubkey".into()),
            expires_at: None,
        }
    }

    #[test]
    fn store_upload_writes_blob_and_meta() {
        let dir = TempDir::new().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let info = store
            .store_upload(
                write_candidate(&dir, b"hello", "text/plain"),
                &settings(dir.path()),
                "http://example.test",
            )
            .unwrap();
        assert_eq!(info.size, 5);
        assert!(store.blob_path(&info.sha256).exists());
        assert!(store.load_meta(&info.sha256).unwrap().is_some());
    }

    #[test]
    fn denylisted_hash_is_rejected() {
        let dir = TempDir::new().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let hash = hex::encode(Sha256::digest(b"hello"));
        let mut cfg = settings(dir.path());
        cfg.file_hash_denylist = Some(vec![hash]);
        assert!(store
            .store_upload(
                write_candidate(&dir, b"hello", "text/plain"),
                &cfg,
                "http://example.test",
            )
            .is_err());
    }

    #[test]
    fn metadata_event_references_local_blob() {
        let dir = TempDir::new().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        let cfg = settings(dir.path());
        let info = store
            .store_upload(
                write_candidate(&dir, b"hello", "text/plain"),
                &cfg,
                "http://example.test",
            )
            .unwrap();
        let event = Event {
            id: "aa".repeat(32),
            pubkey: "p1".into(),
            kind: 1063,
            created_at: 1,
            tags: nip94_tags(&info).into_iter().map(Tag).collect(),
            content: String::new(),
            sig: String::new(),
        };
        store.validate_metadata_event(&event, &cfg).unwrap();
    }
}
