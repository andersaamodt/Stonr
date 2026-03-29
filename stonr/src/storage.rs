//! Minimal file-backed storage and query engine.

use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process, thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use nostr_shared::filter::Filter;
use rand::{seq::SliceRandom, thread_rng};
use serde::{Deserialize, Serialize};
use serde_json::{to_writer, Value};
use sha1::Sha1;
use sha2::Digest;

use crate::event::{Event, Tag};
use crate::files::FileStore;
use std::os::unix::fs as unix_fs;

const MUTATION_LOCK_STALE_AFTER: Duration = Duration::from_secs(120);
const MUTATION_LOCK_OWNER_FILE: &str = "owner.pid";

/// Persistent store for events and indexes rooted at `root`.
#[derive(Clone)]
pub struct Store {
    root: PathBuf,
    verify_sig: bool,
    max_stored_events: Option<usize>,
    max_stored_event_bytes: Option<u64>,
    pinned_pubkeys: HashSet<String>,
    pinned_event_ids: HashSet<String>,
    protect_pinned_from_deletes: bool,
}

/// Structured retention/size status stored under `runtime/`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetentionStatus {
    pub state: String,
    pub current_events: usize,
    pub current_bytes: u64,
    pub max_events: Option<usize>,
    pub max_bytes: Option<u64>,
    pub over_event_limit: bool,
    pub over_byte_limit: bool,
    pub warning: Option<String>,
    pub last_checked_at: u64,
    pub last_prune_at: Option<u64>,
    pub last_prune_removed: Option<usize>,
    pub last_error_at: Option<u64>,
    pub last_error: Option<String>,
}

/// Minimal authoritative store snapshot manifest used for backup/restore.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupManifest {
    pub format: String,
    pub version: String,
    pub created_at: u64,
    pub included_paths: Vec<String>,
}

impl Store {
    /// Create a new store rooted at `root`.
    #[allow(dead_code)]
    pub fn new(root: PathBuf, verify_sig: bool) -> Self {
        Self::with_limits(root, verify_sig, None, None)
    }

    /// Create a new store rooted at `root` with retention limits.
    pub fn with_limits(
        root: PathBuf,
        verify_sig: bool,
        max_stored_events: Option<usize>,
        max_stored_event_bytes: Option<u64>,
    ) -> Self {
        Self::with_limits_and_pins(
            root,
            verify_sig,
            max_stored_events,
            max_stored_event_bytes,
            Vec::new(),
            Vec::new(),
            true,
        )
    }

    /// Create a new store rooted at `root` with retention limits and protected content keys.
    pub fn with_limits_and_pins(
        root: PathBuf,
        verify_sig: bool,
        max_stored_events: Option<usize>,
        max_stored_event_bytes: Option<u64>,
        pinned_pubkeys: Vec<String>,
        pinned_event_ids: Vec<String>,
        protect_pinned_from_deletes: bool,
    ) -> Self {
        let pinned_pubkeys = pinned_pubkeys
            .into_iter()
            .filter(|value| !value.trim().is_empty())
            .collect::<HashSet<_>>();
        let pinned_event_ids = pinned_event_ids
            .into_iter()
            .filter(|value| !value.trim().is_empty())
            .collect::<HashSet<_>>();
        Self {
            root,
            verify_sig,
            max_stored_events,
            max_stored_event_bytes,
            pinned_pubkeys,
            pinned_event_ids,
            protect_pinned_from_deletes,
        }
    }

    /// Ensure the on-disk directory structure exists.
    ///
    /// Layout overview:
    /// - `events/` – each event as `<id>.json` nested by hash prefix
    /// - `log/` – append-only newline-delimited event log
    /// - `index/` – text files mapping authors, kinds, and tags to IDs
    /// - `latest/` – pointers for replaceable events (`kind` + `#d`)
    /// - `mirror/` – symlink trees for author and kind scans
    /// - `cursor/` – last mirrored timestamps per upstream relay
    /// - `runtime/` – cached counts/bytes and pid/log files managed by tooling
    pub fn init(&self) -> Result<()> {
        let dirs = [
            "events",
            "log",
            "latest",
            "index/by-author",
            "index/by-kind",
            "index/by-tag/d",
            "index/by-tag/t",
            "tombstones/by-id",
            "tombstones/by-address",
            "mirror/authors",
            "mirror/kinds",
            "cursor",
            "runtime",
        ];
        for d in dirs {
            fs::create_dir_all(self.root.join(d))?;
        }
        self.files().init()?;
        if !self.retention_status_path().exists() {
            self.write_retention_status_from_counts(0, 0, None, None)?;
        }
        Ok(())
    }

    /// Return the blob/file store rooted under the same relay root.
    pub fn files(&self) -> FileStore {
        FileStore::new(self.root.clone())
    }

    /// Ingest an event if it doesn't already exist on disk.
    ///
    /// Steps:
    /// 1. Optionally verify the event's Schnorr signature.
    /// 2. Write the event JSON under `events/<id>.json`.
    /// 3. Append the event to `log/events.ndjson`.
    /// 4. Update indexes and create mirror symlinks.
    #[allow(dead_code)]
    pub fn ingest(&self, ev: &Event) -> Result<()> {
        let _lock = self.mutation_lock()?;
        let _ = self.ingest_locked(ev)?;
        Ok(())
    }

    /// Ingest an event while enforcing delete and expiration policy.
    ///
    /// Returns `true` when a new event was stored and `false` when the event
    /// was skipped because it was already present or already expired.
    pub fn ingest_with_policy(
        &self,
        ev: &Event,
        delete_enabled: bool,
        expiration_enabled: bool,
    ) -> Result<bool> {
        let _lock = self.mutation_lock()?;
        if expiration_enabled && ev.kind != 5 && event_is_expired(ev, current_unix_ts()) {
            return Ok(false);
        }
        let stored = self.ingest_locked(ev)?;
        if delete_enabled && ev.kind == 5 {
            self.apply_delete_event_locked(ev)?;
        }
        Ok(stored)
    }

    fn ingest_locked(&self, ev: &Event) -> Result<bool> {
        // Optionally verify the event's Schnorr signature before writing.
        if self.verify_sig {
            verify_event(ev)?;
        }
        // Skip ingest if the event already exists on disk.
        let path = self.event_path(&ev.id);
        if path.exists() {
            return Ok(false);
        }
        // Write the event JSON atomically to its canonical path.
        let parent_dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        fs::create_dir_all(&parent_dir)?;
        let tmp = tempfile::NamedTempFile::new_in(&parent_dir)?;
        to_writer(&tmp, ev)?;
        tmp.persist(&path)?;

        // Append the event to a newline-delimited log for easy tailing.
        let log_path = self.root.join("log/events.ndjson");
        let mut log_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        serde_json::to_writer(&mut log_file, ev)?;
        log_file.write_all(b"\n")?;

        // Update lookup indexes and create mirror symlinks.
        self.index_event(ev)?;
        self.write_mirror_links(ev)?;
        let size_bytes = fs::metadata(&path)?.len();
        self.bump_stats_cache(size_bytes)?;
        Ok(true)
    }

    /// Verify Schnorr signatures for a random sample of stored events.
    pub fn verify_sample(&self, sample: usize) -> Result<usize> {
        let mut paths = vec![];
        for entry in walkdir::WalkDir::new(self.root.join("events")) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let path = entry.into_path();
                if Self::is_event_json_file(&path) {
                    paths.push(path);
                }
            }
        }
        let mut rng = thread_rng();
        paths.shuffle(&mut rng);
        let take = sample.min(paths.len());
        for p in paths.iter().take(take) {
            let data = fs::read_to_string(p)?;
            let ev: Event = serde_json::from_str(&data)?;
            verify_event(&ev)?;
        }
        Ok(take)
    }

    /// Rebuild all indexes and latest pointers from the `events/` tree.
    pub fn reindex(&self) -> Result<()> {
        let _lock = self.mutation_lock()?;
        self.rebuild_metadata()
    }

    /// Apply configured retention limits immediately.
    pub fn enforce_retention(&self) -> Result<usize> {
        let _lock = self.mutation_lock()?;
        self.enforce_retention_locked()
    }

    fn enforce_retention_locked(&self) -> Result<usize> {
        if self.max_stored_events.is_none() && self.max_stored_event_bytes.is_none() {
            let (current_events, current_bytes) = self.read_stats_cache()?.unwrap_or((0, 0));
            self.write_retention_status_from_counts(current_events, current_bytes, None, None)?;
            return Ok(0);
        }
        let mut records = self.load_event_records()?;
        let mut total_bytes: u64 = records.iter().map(|record| record.size_bytes).sum();
        let mut total_events = records.len();
        let mut removed = 0usize;
        records.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });

        for record in &records {
            let over_events = self
                .max_stored_events
                .map(|limit| total_events > limit)
                .unwrap_or(false);
            let over_bytes = self
                .max_stored_event_bytes
                .map(|limit| total_bytes > limit)
                .unwrap_or(false);
            if !over_events && !over_bytes {
                break;
            }
            if self.event_is_retention_protected(&record.event) {
                continue;
            }
            fs::remove_file(&record.path)?;
            total_events = total_events.saturating_sub(1);
            total_bytes = total_bytes.saturating_sub(record.size_bytes);
            removed += 1;
        }
        if removed > 0 {
            self.rewrite_event_log(records.into_iter().skip(removed))?;
            self.rebuild_metadata()?;
        } else {
            self.write_stats_cache(total_events, total_bytes)?;
        }
        self.write_retention_status_from_counts(
            total_events,
            total_bytes,
            Some(current_unix_ts()),
            Some(removed),
        )?;
        Ok(removed)
    }

    pub fn refresh_stats_cache(&self) -> Result<(usize, u64)> {
        let mut total_events = 0usize;
        let mut total_bytes = 0u64;
        for entry in walkdir::WalkDir::new(self.root.join("events")) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !Self::is_event_json_file(path) {
                continue;
            }
            match fs::metadata(path) {
                Ok(meta) => {
                    total_events += 1;
                    total_bytes += meta.len();
                }
                Err(error) => {
                    crate::log::warn(
                        "storage",
                        "skipping event stat",
                        serde_json::json!({
                            "path": path.display().to_string(),
                            "error": error.to_string(),
                        }),
                    );
                }
            }
        }
        self.write_stats_cache(total_events, total_bytes)?;
        Ok((total_events, total_bytes))
    }

    /// Return the latest structured retention status.
    pub fn retention_status(&self) -> Result<RetentionStatus> {
        let path = self.retention_status_path();
        if path.exists() {
            return Ok(serde_json::from_str(&fs::read_to_string(path)?)?);
        }
        let (events, bytes) = self.read_stats_cache()?.unwrap_or((0, 0));
        let status = self.write_retention_status_from_counts(events, bytes, None, None)?;
        Ok(status)
    }

    /// Record a retention enforcement failure for operator visibility.
    pub fn report_retention_error(&self, error: &str) -> Result<RetentionStatus> {
        let (current_events, current_bytes) = self.read_stats_cache()?.unwrap_or((0, 0));
        let mut status = self.build_retention_status(current_events, current_bytes);
        status.state = "error".into();
        status.last_error_at = Some(current_unix_ts());
        status.last_error = Some(error.to_string());
        self.write_retention_status_file(&status)?;
        Ok(status)
    }

    /// Create a minimal authoritative snapshot of the store at `destination`.
    pub fn backup_to(&self, destination: &Path) -> Result<BackupManifest> {
        let _lock = self.mutation_lock()?;
        if destination.exists() {
            let mut entries = fs::read_dir(destination)?;
            if entries.next().transpose()?.is_some() {
                return Err(anyhow!("backup destination must be empty"));
            }
        } else {
            fs::create_dir_all(destination)?;
        }
        for relative in authoritative_backup_paths() {
            copy_tree(&self.root.join(relative), &destination.join(relative))?;
        }
        let manifest = BackupManifest {
            format: "stonr-store-snapshot-v1".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            created_at: current_unix_ts(),
            included_paths: authoritative_backup_paths()
                .iter()
                .map(|value| value.to_string())
                .collect(),
        };
        fs::write(
            destination.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        Ok(manifest)
    }

    /// Restore a previously created snapshot into the current store root.
    pub fn restore_from(&self, source: &Path) -> Result<BackupManifest> {
        let manifest_path = source.join("manifest.json");
        let manifest: BackupManifest = serde_json::from_str(&fs::read_to_string(&manifest_path)?)?;
        if manifest.format != "stonr-store-snapshot-v1" {
            return Err(anyhow!("unsupported backup format"));
        }
        let _lock = self.mutation_lock()?;
        fs::create_dir_all(&self.root)?;
        for relative in [
            "events",
            "files",
            "admin",
            "cursor",
            "index",
            "latest",
            "tombstones",
            "mirror",
            "log",
        ] {
            remove_path_if_exists(&self.root.join(relative))?;
        }
        self.clear_runtime_for_restore()?;
        for relative in authoritative_backup_paths() {
            copy_tree(&source.join(relative), &self.root.join(relative))?;
        }
        self.init()?;
        self.rebuild_metadata()?;
        let events = self
            .load_event_records()?
            .into_iter()
            .map(|record| record.event)
            .collect::<Vec<_>>();
        self.files().rebuild_references(&events)?;
        self.refresh_stats_cache()?;
        Ok(manifest)
    }

    fn rebuild_metadata(&self) -> Result<()> {
        let index_dir = self.root.join("index");
        if index_dir.exists() {
            fs::remove_dir_all(&index_dir)?;
        }
        let latest_dir = self.root.join("latest");
        if latest_dir.exists() {
            fs::remove_dir_all(&latest_dir)?;
        }
        let tombstone_dir = self.root.join("tombstones");
        if tombstone_dir.exists() {
            fs::remove_dir_all(&tombstone_dir)?;
        }
        let mirror_dir = self.root.join("mirror");
        if mirror_dir.exists() {
            fs::remove_dir_all(&mirror_dir)?;
        }
        let log_dir = self.root.join("log");
        if log_dir.exists() {
            fs::remove_dir_all(&log_dir)?;
        }
        // recreate directory structure for indexes and latest
        fs::create_dir_all(self.root.join("index/by-author"))?;
        fs::create_dir_all(self.root.join("index/by-kind"))?;
        fs::create_dir_all(self.root.join("index/by-tag/d"))?;
        fs::create_dir_all(self.root.join("index/by-tag/t"))?;
        fs::create_dir_all(self.root.join("latest"))?;
        fs::create_dir_all(self.root.join("tombstones/by-id"))?;
        fs::create_dir_all(self.root.join("tombstones/by-address"))?;
        fs::create_dir_all(self.root.join("mirror/authors"))?;
        fs::create_dir_all(self.root.join("mirror/kinds"))?;
        fs::create_dir_all(self.root.join("log"))?;

        let mut records = self.load_event_records()?;
        records.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        let log_path = self.root.join("log/events.ndjson");
        let mut log_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(log_path)?;
        let total_events = records.len();
        let total_bytes: u64 = records.iter().map(|record| record.size_bytes).sum();
        for record in records {
            self.index_event(&record.event)?;
            self.write_mirror_links(&record.event)?;
            if record.event.kind == 5 {
                self.apply_delete_event_locked(&record.event)?;
            }
            serde_json::to_writer(&mut log_file, &record.event)?;
            log_file.write_all(b"\n")?;
        }
        self.write_stats_cache(total_events, total_bytes)?;
        Ok(())
    }

    fn rewrite_event_log<I>(&self, records: I) -> Result<()>
    where
        I: IntoIterator<Item = StoredEventRecord>,
    {
        let log_path = self.root.join("log/events.ndjson");
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut log_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(log_path)?;
        for record in records {
            serde_json::to_writer(&mut log_file, &record.event)?;
            log_file.write_all(b"\n")?;
        }
        Ok(())
    }

    fn bump_stats_cache(&self, added_bytes: u64) -> Result<()> {
        let count_path = self.root.join("runtime/events-count.cache");
        let bytes_path = self.root.join("runtime/events-bytes.cache");
        let count = fs::read_to_string(&count_path)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok());
        let bytes = fs::read_to_string(&bytes_path)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok());
        if let (Some(count), Some(bytes)) = (count, bytes) {
            let next = (count.saturating_add(1), bytes.saturating_add(added_bytes));
            self.write_stats_cache(next.0, next.1)?;
        }
        Ok(())
    }

    fn write_stats_cache(&self, total_events: usize, total_bytes: u64) -> Result<()> {
        fs::create_dir_all(self.root.join("runtime"))?;
        fs::write(
            self.root.join("runtime/events-count.cache"),
            total_events.to_string(),
        )?;
        fs::write(
            self.root.join("runtime/events-bytes.cache"),
            total_bytes.to_string(),
        )?;
        self.write_retention_status_from_counts(total_events, total_bytes, None, None)?;
        Ok(())
    }

    fn read_stats_cache(&self) -> Result<Option<(usize, u64)>> {
        let count_path = self.root.join("runtime/events-count.cache");
        let bytes_path = self.root.join("runtime/events-bytes.cache");
        let count = fs::read_to_string(&count_path)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok());
        let bytes = fs::read_to_string(&bytes_path)
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok());
        Ok(match (count, bytes) {
            (Some(count), Some(bytes)) => Some((count, bytes)),
            _ => None,
        })
    }

    fn retention_status_path(&self) -> PathBuf {
        self.root.join("runtime/retention-status.json")
    }

    fn build_retention_status(&self, current_events: usize, current_bytes: u64) -> RetentionStatus {
        let over_event_limit = self
            .max_stored_events
            .map(|limit| current_events > limit)
            .unwrap_or(false);
        let over_byte_limit = self
            .max_stored_event_bytes
            .map(|limit| current_bytes > limit)
            .unwrap_or(false);
        let warning = retention_warning(
            over_event_limit,
            over_byte_limit,
            self.max_stored_events,
            self.max_stored_event_bytes,
            current_events,
            current_bytes,
        );
        let state = if self.max_stored_events.is_none() && self.max_stored_event_bytes.is_none() {
            "disabled"
        } else if warning.is_some() {
            "warning"
        } else {
            "ok"
        };
        RetentionStatus {
            state: state.into(),
            current_events,
            current_bytes,
            max_events: self.max_stored_events,
            max_bytes: self.max_stored_event_bytes,
            over_event_limit,
            over_byte_limit,
            warning,
            last_checked_at: current_unix_ts(),
            last_prune_at: None,
            last_prune_removed: None,
            last_error_at: None,
            last_error: None,
        }
    }

    fn write_retention_status_from_counts(
        &self,
        current_events: usize,
        current_bytes: u64,
        last_prune_at: Option<u64>,
        last_prune_removed: Option<usize>,
    ) -> Result<RetentionStatus> {
        let existing = self.read_retention_status_file().ok().flatten();
        let mut status = self.build_retention_status(current_events, current_bytes);
        if let Some(existing) = existing {
            status.last_prune_at = last_prune_at.or(existing.last_prune_at);
            status.last_prune_removed = last_prune_removed.or(existing.last_prune_removed);
        } else {
            status.last_prune_at = last_prune_at;
            status.last_prune_removed = last_prune_removed;
        }
        self.write_retention_status_file(&status)?;
        Ok(status)
    }

    fn read_retention_status_file(&self) -> Result<Option<RetentionStatus>> {
        let path = self.retention_status_path();
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    fn write_retention_status_file(&self, status: &RetentionStatus) -> Result<()> {
        if let Some(parent) = self.retention_status_path().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(self.retention_status_path(), serde_json::to_vec(status)?)?;
        Ok(())
    }

    fn clear_runtime_for_restore(&self) -> Result<()> {
        let runtime_dir = self.root.join("runtime");
        fs::create_dir_all(&runtime_dir)?;
        for entry in fs::read_dir(&runtime_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_string_lossy() == "store.lock" {
                continue;
            }
            remove_path_if_exists(&entry.path())?;
        }
        Ok(())
    }

    fn mutation_lock(&self) -> Result<MutationLock> {
        let runtime_dir = self.root.join("runtime");
        fs::create_dir_all(&runtime_dir)?;
        let lock_path = runtime_dir.join("store.lock");
        for _ in 0..400 {
            match fs::create_dir(&lock_path) {
                Ok(()) => {
                    if let Err(error) = write_lock_owner_pid(&lock_path) {
                        let _ = fs::remove_dir_all(&lock_path);
                        return Err(error);
                    }
                    return Ok(MutationLock { path: lock_path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if lock_dir_should_break(&lock_path, MUTATION_LOCK_STALE_AFTER) {
                        let _ = fs::remove_dir_all(&lock_path);
                        continue;
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(anyhow!("timed out waiting for storage lock"))
    }

    fn load_event_records(&self) -> Result<Vec<StoredEventRecord>> {
        let mut records = vec![];
        for entry in walkdir::WalkDir::new(self.root.join("events")) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let path = entry.into_path();
                if !Self::is_event_json_file(&path) {
                    continue;
                }
                let size_bytes = fs::metadata(&path)?.len();
                let data = match fs::read_to_string(&path) {
                    Ok(data) => data,
                    Err(error) => {
                        crate::log::warn(
                            "storage",
                            "skipping unreadable event file",
                            serde_json::json!({
                                "path": path.display().to_string(),
                                "error": error.to_string(),
                            }),
                        );
                        continue;
                    }
                };
                let ev: Event = match serde_json::from_str(&data) {
                    Ok(event) => event,
                    Err(error) => {
                        crate::log::warn(
                            "storage",
                            "skipping malformed event file",
                            serde_json::json!({
                                "path": path.display().to_string(),
                                "error": error.to_string(),
                            }),
                        );
                        continue;
                    }
                };
                records.push(StoredEventRecord {
                    id: ev.id.clone(),
                    created_at: ev.created_at,
                    size_bytes,
                    path,
                    event: ev,
                });
            }
        }
        Ok(records)
    }

    /// Update text-file indexes and latest pointers for an event.
    fn index_event(&self, ev: &Event) -> Result<()> {
        // Author and kind indexes are simple append-only lists of IDs.
        self.append_index("index/by-author", &ev.pubkey, &ev.id)?;
        self.append_index("index/by-kind", &ev.kind.to_string(), &ev.id)?;
        for Tag(fields) in &ev.tags {
            if fields.len() >= 2 {
                match fields[0].as_str() {
                    "d" => {
                        // `#d` tags also update a latest pointer for replaceable events.
                        self.append_tag_index("d", &fields[1], &ev.id)?;
                        let latest = self
                            .root
                            .join("latest")
                            .join(format!("{}.{}.{}", ev.pubkey, ev.kind, fields[1]));
                        if let Some(parent) = latest.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::write(latest, &ev.id)?;
                    }
                    "t" => {
                        // Topic (`#t`) tags get their own index directory.
                        self.append_tag_index("t", &fields[1], &ev.id)?;
                    }
                    other => {
                        self.append_tag_index(other, &fields[1], &ev.id)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Create symlinks for author and kind mirrors.
    ///
    /// These allow fast listing of events by author or kind without scanning
    /// the entire `events/` tree.
    fn write_mirror_links(&self, ev: &Event) -> Result<()> {
        let rel_target = format!(
            "../../../events/{}/{}/{}.json",
            &ev.id[0..2],
            &ev.id[2..4],
            ev.id
        );
        // by author
        let author_dir = self.root.join("mirror/authors").join(&ev.pubkey);
        fs::create_dir_all(&author_dir)?;
        let author_link = author_dir.join(format!("{}-{}.json", ev.created_at, ev.id));
        if !author_link.exists() {
            unix_fs::symlink(&rel_target, &author_link)?;
        }

        // by kind
        let kind_dir = self.root.join("mirror/kinds").join(ev.kind.to_string());
        fs::create_dir_all(&kind_dir)?;
        let kind_link = kind_dir.join(format!("{}-{}.json", ev.created_at, ev.id));
        if !kind_link.exists() {
            unix_fs::symlink(rel_target, kind_link)?;
        }
        Ok(())
    }

    /// Append an event ID to the index file under `prefix/name.txt`.
    fn append_index(&self, prefix: &str, name: &str, id: &str) -> Result<()> {
        let path = self.root.join(prefix).join(format!("{}.txt", name));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(f, "{}", id)?;
        Ok(())
    }

    fn append_tag_index(&self, tag_key: &str, tag_value: &str, id: &str) -> Result<()> {
        let path = self
            .root
            .join("index/by-tag")
            .join(tag_key)
            .join(format!("{}.txt", hashed_index_name(tag_value)));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(f, "{}", id)?;
        Ok(())
    }

    /// Compute the canonical path for an event ID.
    fn event_path(&self, id: &str) -> PathBuf {
        let sub1 = &id[0..2];
        let sub2 = &id[2..4];
        self.root
            .join("events")
            .join(sub1)
            .join(sub2)
            .join(format!("{}.json", id))
    }

    /// Helper to load ID sets for a list of keys under `prefix`.
    fn load_ids(&self, prefix: &str, keys: &[String]) -> Result<std::collections::HashSet<String>> {
        let mut ids = std::collections::HashSet::new();
        for key in keys {
            let path = self.root.join(prefix).join(format!("{}.txt", key));
            ids.extend(read_ids(&path)?);
        }
        Ok(ids)
    }

    /// Execute a simple intersection-based query over indexes.
    ///
    /// A query like `authors=npub1&kinds=1` works by loading
    /// `index/by-author/npub1.txt` and `index/by-kind/1.txt`, intersecting the
    /// resulting ID sets, and then reading each matching event from disk. Time
    /// bounds and limits are applied after the intersection.
    #[allow(dead_code)]
    pub fn query(&self, q: Query) -> Result<Vec<Event>> {
        let events = self.query_candidates(&q)?;
        self.filter_sort_and_limit(events, &q, false, false)
    }

    /// Execute a query while enforcing delete and expiration policy.
    pub fn query_with_policy(
        &self,
        q: Query,
        delete_enabled: bool,
        expiration_enabled: bool,
    ) -> Result<Vec<Event>> {
        let events = self.query_candidates(&q)?;
        self.filter_sort_and_limit(events, &q, delete_enabled, expiration_enabled)
    }

    /// Return whether an event is visible under delete/expiration policy.
    pub fn event_visible_with_policy(
        &self,
        event: &Event,
        delete_enabled: bool,
        expiration_enabled: bool,
    ) -> Result<bool> {
        self.event_visible(event, delete_enabled, expiration_enabled)
    }

    fn query_candidates(&self, q: &Query) -> Result<Vec<Event>> {
        let since = q.since;
        let until = q.until;
        let limit = q.limit;
        let search = q.search.clone();
        // Collect ID sets for each filter category and intersect them below.
        let mut sets: Vec<std::collections::HashSet<String>> = vec![];
        if let Some(authors) = &q.authors {
            sets.push(self.load_ids("index/by-author", authors)?);
        }
        if let Some(kinds) = &q.kinds {
            let keys: Vec<String> = kinds.iter().map(|k| k.to_string()).collect();
            sets.push(self.load_ids("index/by-kind", &keys)?);
        }
        if let Some(d) = &q.d {
            let path = self
                .root
                .join("index/by-tag/d")
                .join(format!("{}.txt", hashed_index_name(d)));
            sets.push(read_ids(&path)?);
        }
        if let Some(t) = &q.t {
            let path = self
                .root
                .join("index/by-tag/t")
                .join(format!("{}.txt", hashed_index_name(t)));
            sets.push(read_ids(&path)?);
        }
        for (tag, values) in &q.tags {
            let mut ids = std::collections::HashSet::new();
            for value in values {
                let path = self
                    .root
                    .join("index/by-tag")
                    .join(tag)
                    .join(format!("{}.txt", hashed_index_name(value)));
                ids.extend(read_ids(&path)?);
            }
            sets.push(ids);
        }
        if sets.is_empty() {
            return self.load_all_events();
        }
        let mut iter = sets.into_iter();
        // Start with the first ID set and intersect each subsequent one.
        let mut ids = iter.next().unwrap();
        for s in iter {
            ids = ids.intersection(&s).cloned().collect();
        }

        // Load matching events and apply time-based filters.
        let events: Vec<Event> = ids
            .into_iter()
            .filter_map(|id| {
                let path = self.event_path(&id);
                let data = fs::read_to_string(path).ok()?;
                serde_json::from_str(&data).ok()
            })
            .collect();
        let _ = (since, until, limit, search);
        Ok(events)
    }

    fn load_all_events(&self) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        for entry in walkdir::WalkDir::new(self.root.join("events")) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.into_path();
            if !Self::is_event_json_file(&path) {
                continue;
            }
            let data = match fs::read_to_string(&path) {
                Ok(data) => data,
                Err(_) => continue,
            };
            if let Ok(event) = serde_json::from_str(&data) {
                events.push(event);
            }
        }
        Ok(events)
    }

    fn is_event_json_file(path: &Path) -> bool {
        if path.file_name().and_then(|name| name.to_str()) == Some(".DS_Store") {
            return false;
        }
        path.extension().and_then(|ext| ext.to_str()) == Some("json")
    }

    fn filter_sort_and_limit(
        &self,
        mut events: Vec<Event>,
        q: &Query,
        delete_enabled: bool,
        expiration_enabled: bool,
    ) -> Result<Vec<Event>> {
        let search = q.search.as_ref().map(|value| value.to_lowercase());
        let mut filtered = Vec::with_capacity(events.len());
        for event in events.drain(..) {
            if q.since.is_some_and(|since| event.created_at < since) {
                continue;
            }
            if q.until.is_some_and(|until| event.created_at > until) {
                continue;
            }
            if search
                .as_ref()
                .is_some_and(|term| !event.content.to_lowercase().contains(term))
            {
                continue;
            }
            if !self.event_visible(&event, delete_enabled, expiration_enabled)? {
                continue;
            }
            filtered.push(event);
        }
        // Sort newest-first so replaceable events keep the most recent version.
        filtered.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        // Drop older replaceable events sharing the same author, kind, and `#d` tag.
        let mut seen = std::collections::HashSet::new();
        filtered.retain(|event| {
            if let Some(address) = event_address(event) {
                seen.insert(address)
            } else {
                true
            }
        });
        if let Some(limit) = q.limit {
            filtered.truncate(limit);
        }
        Ok(filtered)
    }

    fn event_visible(
        &self,
        event: &Event,
        delete_enabled: bool,
        expiration_enabled: bool,
    ) -> Result<bool> {
        let protected = self.event_is_retention_protected(event);
        if protected && self.protect_pinned_from_deletes {
            return Ok(true);
        }
        if expiration_enabled && event_is_expired(event, current_unix_ts()) && !protected {
            return Ok(false);
        }
        if !delete_enabled || event.kind == 5 {
            return Ok(true);
        }
        if let Some(tombstone) = self.read_tombstone(&self.tombstone_id_path(&event.id))? {
            if tombstone.pubkey == event.pubkey && event.created_at <= tombstone.created_at {
                return Ok(false);
            }
        }
        if let Some(address) = event_address(event) {
            if let Some(tombstone) = self.read_tombstone(&self.tombstone_address_path(&address))? {
                if tombstone.pubkey == event.pubkey && event.created_at <= tombstone.created_at {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn event_is_retention_protected(&self, event: &Event) -> bool {
        self.pinned_pubkeys.contains(&event.pubkey) || self.pinned_event_ids.contains(&event.id)
    }

    fn apply_delete_event_locked(&self, event: &Event) -> Result<()> {
        if event.kind != 5 {
            return Ok(());
        }
        let tombstone = DeleteTombstone {
            pubkey: event.pubkey.clone(),
            created_at: event.created_at,
            delete_event_id: event.id.clone(),
        };
        for Tag(fields) in &event.tags {
            match fields.as_slice() {
                [tag, value, ..] if tag == "e" => {
                    self.write_tombstone(&self.tombstone_id_path(value), &tombstone)?;
                }
                [tag, value, ..] if tag == "a" => {
                    self.write_tombstone(&self.tombstone_address_path(value), &tombstone)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn tombstone_id_path(&self, event_id: &str) -> PathBuf {
        self.root
            .join("tombstones/by-id")
            .join(format!("{event_id}.json"))
    }

    fn tombstone_address_path(&self, address: &str) -> PathBuf {
        self.root
            .join("tombstones/by-address")
            .join(format!("{}.json", hashed_index_name(address)))
    }

    fn read_tombstone(&self, path: &Path) -> Result<Option<DeleteTombstone>> {
        if !path.exists() {
            return Ok(None);
        }
        let data = match fs::read_to_string(path) {
            Ok(data) => data,
            Err(error) => {
                crate::log::warn(
                    "storage",
                    "skipping unreadable tombstone",
                    serde_json::json!({
                        "path": path.display().to_string(),
                        "error": error.to_string(),
                    }),
                );
                return Ok(None);
            }
        };
        match serde_json::from_str::<DeleteTombstone>(&data) {
            Ok(tombstone) => Ok(Some(tombstone)),
            Err(error) => {
                crate::log::warn(
                    "storage",
                    "skipping malformed tombstone",
                    serde_json::json!({
                        "path": path.display().to_string(),
                        "error": error.to_string(),
                    }),
                );
                Ok(None)
            }
        }
    }

    fn write_tombstone(&self, path: &Path, tombstone: &DeleteTombstone) -> Result<()> {
        if let Some(existing) = self.read_tombstone(path)? {
            if existing.created_at > tombstone.created_at
                || (existing.created_at == tombstone.created_at
                    && existing.delete_event_id >= tombstone.delete_event_id)
            {
                return Ok(());
            }
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec(tombstone)?)?;
        Ok(())
    }
}

struct StoredEventRecord {
    id: String,
    created_at: u64,
    size_bytes: u64,
    path: PathBuf,
    event: Event,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeleteTombstone {
    pubkey: String,
    created_at: u64,
    delete_event_id: String,
}

struct MutationLock {
    path: PathBuf,
}

impl Drop for MutationLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_lock_owner_pid(path: &Path) -> Result<()> {
    fs::write(
        path.join(MUTATION_LOCK_OWNER_FILE),
        format!("{}\n", process::id()),
    )?;
    Ok(())
}

fn read_lock_owner_pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path.join(MUTATION_LOCK_OWNER_FILE))
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
}

fn lock_owner_pid_is_running(pid: u32) -> bool {
    // `kill(pid, 0)` checks process existence without sending a signal.
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(code) if code == libc::EPERM
    )
}

fn lock_dir_should_break(path: &Path, stale_after: Duration) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(value) => value,
        Err(_) => return false,
    };
    if !metadata.is_dir() {
        return false;
    }
    if let Some(owner_pid) = read_lock_owner_pid(path) {
        return !lock_owner_pid_is_running(owner_pid);
    }
    let modified = match metadata.modified() {
        Ok(value) => value,
        Err(_) => return false,
    };
    match modified.elapsed() {
        Ok(age) => age > stale_after,
        Err(_) => false,
    }
}

/// Read newline-separated IDs from a text file.
fn read_ids(path: &Path) -> Result<std::collections::HashSet<String>> {
    if !path.exists() {
        return Ok(Default::default());
    }
    let data = fs::read_to_string(path)?;
    Ok(data.lines().map(|s| s.to_string()).collect())
}

/// Query parameters accepted by both HTTP and WebSocket interfaces.
#[derive(Clone, Debug)]
pub struct Query {
    pub authors: Option<Vec<String>>,
    pub kinds: Option<Vec<u32>>,
    pub d: Option<String>,
    pub t: Option<String>,
    pub tags: Vec<(String, Vec<String>)>,
    pub search: Option<String>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<usize>,
}

impl Query {
    /// Build a `Query` from a Nostr filter JSON object used by HTTP and WS APIs.
    pub fn from_value(val: &Value) -> Self {
        let filter = Filter::from_value(val);
        Query {
            authors: filter.authors,
            kinds: filter.kinds,
            d: filter.d,
            t: filter.t,
            tags: filter.tags,
            search: filter.search,
            since: filter.since,
            until: filter.until,
            limit: filter.limit,
        }
    }

    pub fn has_tag_filters(&self) -> bool {
        self.d.is_some() || self.t.is_some() || !self.tags.is_empty()
    }
}

fn hashed_index_name(value: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn event_address(event: &Event) -> Option<String> {
    event
        .tags
        .iter()
        .find_map(|Tag(fields)| match fields.as_slice() {
            [tag, value, ..] if tag == "d" => {
                Some(format!("{}:{}:{}", event.kind, event.pubkey, value))
            }
            _ => None,
        })
}

fn event_expiration(event: &Event) -> Option<u64> {
    event
        .tags
        .iter()
        .find_map(|Tag(fields)| match fields.as_slice() {
            [tag, value, ..] if tag == "expiration" => value.parse::<u64>().ok(),
            _ => None,
        })
}

fn event_is_expired(event: &Event, now: u64) -> bool {
    event_expiration(event).is_some_and(|expiration| expiration <= now)
}

fn current_unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn authoritative_backup_paths() -> [&'static str; 4] {
    ["events", "files", "admin", "cursor"]
}

fn retention_warning(
    over_event_limit: bool,
    over_byte_limit: bool,
    max_events: Option<usize>,
    max_bytes: Option<u64>,
    current_events: usize,
    current_bytes: u64,
) -> Option<String> {
    match (over_event_limit, over_byte_limit) {
        (true, true) => Some(format!(
            "Retention is over both caps (events: {} > {}, bytes: {} > {}).",
            current_events,
            max_events.unwrap_or_default(),
            current_bytes,
            max_bytes.unwrap_or_default()
        )),
        (true, false) => Some(format!(
            "Retention is over the event cap ({} > {}).",
            current_events,
            max_events.unwrap_or_default()
        )),
        (false, true) => Some(format!(
            "Retention is over the byte cap ({} > {}).",
            current_bytes,
            max_bytes.unwrap_or_default()
        )),
        (false, false) => None,
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    for entry in walkdir::WalkDir::new(source) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Recompute the Nostr event hash from its fields.
pub(crate) fn event_hash(ev: &Event) -> Result<[u8; 32]> {
    nostr_shared::crypto::event_hash(ev)
}

/// Verify an event's ID and Schnorr signature.
pub fn verify_event(ev: &Event) -> Result<()> {
    nostr_shared::crypto::verify_event(ev)
}

pub(crate) fn verify_signed_event(ev: &Event) -> Result<()> {
    verify_event(ev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Keypair, Message, Secp256k1};
    use std::{fs, time::Instant};
    use tempfile::TempDir;

    fn sample_event(id: &str, pubkey: &str, kind: u32, dtag: Option<&str>, created: u64) -> Event {
        let mut tags = vec![];
        if let Some(d) = dtag {
            tags.push(Tag(vec!["d".into(), d.into()]));
        }
        Event {
            id: id.into(),
            pubkey: pubkey.into(),
            kind,
            created_at: created,
            tags,
            content: String::new(),
            sig: String::new(),
        }
    }

    fn signed_event(kind: u32) -> Event {
        let secp = Secp256k1::new();
        let sk = [1u8; 32];
        let kp = Keypair::from_seckey_slice(&secp, &sk).unwrap();
        let pubkey = kp.x_only_public_key().0;
        let mut ev = Event {
            id: String::new(),
            pubkey: hex::encode(pubkey.serialize()),
            kind,
            created_at: 1,
            tags: vec![],
            content: String::new(),
            sig: String::new(),
        };
        let hash = event_hash(&ev).unwrap();
        ev.id = hex::encode(hash);
        let msg = Message::from_digest_slice(&hash).unwrap();
        let sig = secp.sign_schnorr_no_aux_rand(&msg, &kp);
        ev.sig = hex::encode(sig.as_ref());
        ev
    }

    #[test]
    fn init_and_ingest() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev = sample_event("abcd", "pub", 1, Some("slug"), 1);
        store.ingest(&ev).unwrap();
        // ingest again should be idempotent
        store.ingest(&ev).unwrap();
        let id_path = store.root.join("index/by-author/pub.txt");
        let ids = fs::read_to_string(id_path).unwrap();
        assert_eq!(ids.lines().count(), 1);
        assert!(store.root.join("cursor").exists());
    }

    #[test]
    fn creates_mirror_symlinks() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev = sample_event("abcd", "pub", 30023, Some("slug"), 42);
        store.ingest(&ev).unwrap();
        let author_link = dir.path().join("mirror/authors/pub/42-abcd.json");
        assert!(author_link.exists());
        let target = fs::read_link(&author_link).unwrap();
        assert!(target.to_str().unwrap().ends_with("events/ab/cd/abcd.json"));
        let kind_link = dir.path().join("mirror/kinds/30023/42-abcd.json");
        assert!(kind_link.exists());
        let target2 = fs::read_link(kind_link).unwrap();
        assert!(target2
            .to_str()
            .unwrap()
            .ends_with("events/ab/cd/abcd.json"));
    }

    #[test]
    fn query_intersection() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let e1 = sample_event("aa11", "p1", 1, Some("s1"), 10);
        let e2 = sample_event("bb22", "p1", 30023, Some("s2"), 20);
        store.ingest(&e1).unwrap();
        store.ingest(&e2).unwrap();
        let res = store
            .query(Query {
                authors: Some(vec!["p1".into()]),
                kinds: Some(vec![30023]),
                d: Some("s2".into()),
                t: None,
                tags: vec![],
                search: None,
                since: None,
                until: None,
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].id, "bb22");
    }

    #[test]
    fn rebuild_indexes() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev = sample_event("abcd", "pub", 1, Some("slug"), 1);
        store.ingest(&ev).unwrap();
        // remove indexes and latest
        fs::remove_dir_all(dir.path().join("index")).unwrap();
        fs::remove_dir_all(dir.path().join("latest")).unwrap();
        // rebuild
        store.reindex().unwrap();
        let author_idx = fs::read_to_string(dir.path().join("index/by-author/pub.txt")).unwrap();
        assert_eq!(author_idx.trim(), "abcd");
        let latest = fs::read_to_string(dir.path().join("latest/pub.1.slug")).unwrap();
        assert_eq!(latest, "abcd");
    }

    #[test]
    fn tag_index_and_reindex() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev = Event {
            id: "aa11".into(),
            pubkey: "p1".into(),
            kind: 1,
            created_at: 1,
            tags: vec![Tag(vec!["t".into(), "tag1".into()])],
            content: String::new(),
            sig: String::new(),
        };
        store.ingest(&ev).unwrap();
        let tag_path = dir.path().join(format!(
            "index/by-tag/t/{}.txt",
            super::hashed_index_name("tag1")
        ));
        let contents = fs::read_to_string(&tag_path).unwrap();
        assert_eq!(contents.trim(), "aa11");
        fs::remove_file(tag_path).unwrap();
        store.reindex().unwrap();
        let rebuilt = fs::read_to_string(dir.path().join(format!(
            "index/by-tag/t/{}.txt",
            super::hashed_index_name("tag1")
        )))
        .unwrap();
        assert_eq!(rebuilt.trim(), "aa11");
    }

    #[test]
    fn latest_pointer_updates_and_query_returns_latest() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let e1 = sample_event("aa11", "p1", 30023, Some("slug"), 1);
        let e2 = sample_event("bb22", "p1", 30023, Some("slug"), 2);
        store.ingest(&e1).unwrap();
        store.ingest(&e2).unwrap();
        let latest_path = dir.path().join("latest/p1.30023.slug");
        let latest = fs::read_to_string(latest_path).unwrap();
        assert_eq!(latest, "bb22");
        let res = store
            .query(Query {
                authors: Some(vec!["p1".into()]),
                kinds: Some(vec![30023]),
                d: Some("slug".into()),
                t: None,
                tags: vec![],
                search: None,
                since: None,
                until: None,
                limit: None,
            })
            .unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].id, "bb22");
    }

    #[test]
    fn ingest_rejects_bad_sig() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), true);
        store.init().unwrap();
        let mut ev = signed_event(1);
        ev.sig.replace_range(0..2, "00");
        assert!(store.ingest(&ev).is_err());
    }

    #[test]
    fn verify_sample_checks_events() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev1 = signed_event(1);
        let ev2 = signed_event(2);
        store.ingest(&ev1).unwrap();
        store.ingest(&ev2).unwrap();
        assert_eq!(store.verify_sample(10).unwrap(), 2);
        // corrupt one event's signature
        let mut bad = ev1.clone();
        bad.sig = "00".repeat(64);
        let path = store.event_path(&bad.id);
        fs::write(path, serde_json::to_string(&bad).unwrap()).unwrap();
        assert!(store.verify_sample(10).is_err());
    }

    #[test]
    fn verify_sample_ignores_ds_store_files() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev1 = signed_event(1);
        let ev2 = signed_event(2);
        store.ingest(&ev1).unwrap();
        store.ingest(&ev2).unwrap();
        fs::write(dir.path().join("events/.DS_Store"), [0xff, 0xfe]).unwrap();
        fs::create_dir_all(dir.path().join("events/00")).unwrap();
        fs::write(dir.path().join("events/00/.DS_Store"), [0xff, 0xfe]).unwrap();

        assert_eq!(store.verify_sample(10).unwrap(), 2);
    }

    #[test]
    fn query_since_until_and_limit() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let e1 = sample_event("aa11", "p1", 1, None, 10);
        let e2 = sample_event("bb22", "p1", 1, None, 20);
        let e3 = sample_event("cc33", "p1", 1, None, 30);
        store.ingest(&e1).unwrap();
        store.ingest(&e2).unwrap();
        store.ingest(&e3).unwrap();
        let res = store
            .query(Query {
                authors: Some(vec!["p1".into()]),
                kinds: Some(vec![1]),
                d: None,
                t: None,
                tags: vec![],
                search: None,
                since: Some(15),
                until: Some(25),
                limit: Some(1),
            })
            .unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].id, "bb22");
    }

    #[test]
    fn query_without_filters_returns_recent_events() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let e1 = sample_event("aa11", "p1", 1, None, 10);
        let e2 = sample_event("bb22", "p2", 1, None, 20);
        store.ingest(&e1).unwrap();
        store.ingest(&e2).unwrap();
        let res = store
            .query(Query {
                authors: None,
                kinds: None,
                d: None,
                t: None,
                tags: vec![],
                search: None,
                since: None,
                until: None,
                limit: None,
            })
            .unwrap();
        let ids: Vec<String> = res.into_iter().map(|event| event.id).collect();
        assert_eq!(ids, vec!["bb22".to_string(), "aa11".to_string()]);
    }

    #[test]
    fn ingest_rejects_id_mismatch() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), true);
        store.init().unwrap();
        let mut ev = signed_event(1);
        ev.id.replace_range(0..2, "ff");
        assert!(store.ingest(&ev).is_err());
    }

    #[test]
    fn read_ids_returns_empty_for_missing_file() {
        let path = std::path::PathBuf::from("missing.txt");
        let ids = super::read_ids(&path).unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn lock_dir_breaks_when_owner_pid_is_dead() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("store.lock");
        fs::create_dir_all(&lock_path).unwrap();
        fs::write(lock_path.join(super::MUTATION_LOCK_OWNER_FILE), "999999\n").unwrap();
        assert!(super::lock_dir_should_break(
            &lock_path,
            Duration::from_secs(3600)
        ));
    }

    #[test]
    fn lock_dir_keeps_live_owner_even_if_old() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("store.lock");
        fs::create_dir_all(&lock_path).unwrap();
        fs::write(
            lock_path.join(super::MUTATION_LOCK_OWNER_FILE),
            format!("{}\n", std::process::id()),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(10));
        assert!(!super::lock_dir_should_break(
            &lock_path,
            Duration::from_secs(0)
        ));
    }

    #[test]
    fn lock_dir_without_owner_uses_stale_age() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("store.lock");
        fs::create_dir_all(&lock_path).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        assert!(super::lock_dir_should_break(
            &lock_path,
            Duration::from_secs(0)
        ));
    }

    #[test]
    fn mirror_symlinks_not_duplicated() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev = sample_event("abcd", "pub", 30023, Some("slug"), 42);
        store.ingest(&ev).unwrap();
        store.ingest(&ev).unwrap();
        let author_dir = dir.path().join("mirror/authors/pub");
        let author_count = std::fs::read_dir(author_dir).unwrap().count();
        assert_eq!(author_count, 1);
        let kind_dir = dir.path().join("mirror/kinds/30023");
        let kind_count = std::fs::read_dir(kind_dir).unwrap().count();
        assert_eq!(kind_count, 1);
    }

    #[test]
    fn event_hash_matches_reference() {
        use sha2::{Digest, Sha256};
        let ev = Event {
            id: String::new(),
            pubkey: "00".repeat(32),
            kind: 1,
            created_at: 1,
            tags: vec![],
            content: String::new(),
            sig: String::new(),
        };
        let expected = {
            let obj =
                serde_json::json!([0, ev.pubkey, ev.created_at, ev.kind, ev.tags, ev.content]);
            let mut hasher = Sha256::new();
            hasher.update(serde_json::to_vec(&obj).unwrap());
            let bytes = hasher.finalize();
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        };
        assert_eq!(event_hash(&ev).unwrap(), expected);
    }

    #[test]
    fn enforce_retention_deletes_oldest_events_first() {
        let dir = TempDir::new().unwrap();
        let store = Store::with_limits(dir.path().to_path_buf(), false, Some(2), None);
        store.init().unwrap();
        let e1 = sample_event("aa11", "p1", 1, None, 10);
        let e2 = sample_event("bb22", "p1", 1, None, 20);
        let e3 = sample_event("cc33", "p1", 1, None, 30);
        store.ingest(&e1).unwrap();
        store.ingest(&e2).unwrap();
        store.ingest(&e3).unwrap();
        store.enforce_retention().unwrap();

        let res = store
            .query(Query {
                authors: Some(vec!["p1".into()]),
                kinds: Some(vec![1]),
                d: None,
                t: None,
                tags: vec![],
                search: None,
                since: None,
                until: None,
                limit: Some(10),
            })
            .unwrap();
        let ids: Vec<String> = res.into_iter().map(|event| event.id).collect();
        assert_eq!(ids, vec!["cc33".to_string(), "bb22".to_string()]);
        assert!(!store.event_path("aa11").exists());
        assert!(store.event_path("bb22").exists());
        assert!(store.event_path("cc33").exists());
    }

    #[test]
    fn enforce_retention_keeps_pinned_pubkeys_and_event_ids() {
        let dir = TempDir::new().unwrap();
        let store = Store::with_limits_and_pins(
            dir.path().to_path_buf(),
            false,
            Some(2),
            None,
            vec!["owner".into()],
            vec!["pin22".into()],
            true,
        );
        store.init().unwrap();

        store
            .ingest(&sample_event("old11", "untrusted", 1, None, 10))
            .unwrap();
        store
            .ingest(&sample_event("pin22", "untrusted", 1, None, 20))
            .unwrap();
        store
            .ingest(&sample_event("own33", "owner", 1, None, 30))
            .unwrap();
        store.enforce_retention().unwrap();

        let events = store
            .query_with_policy(
                Query {
                    authors: None,
                    kinds: Some(vec![1]),
                    d: None,
                    t: None,
                    tags: vec![],
                    search: None,
                    since: None,
                    until: None,
                    limit: Some(10),
                },
                true,
                true,
            )
            .unwrap();
        let ids: Vec<String> = events.into_iter().map(|event| event.id).collect();
        assert_eq!(ids, vec!["own33".to_string(), "pin22".to_string()]);
        assert!(!store.event_path("old11").exists());
    }

    #[test]
    fn enforce_retention_skips_corrupt_event_files() {
        let dir = TempDir::new().unwrap();
        let store = Store::with_limits(dir.path().to_path_buf(), false, Some(10), None);
        store.init().unwrap();

        let good = sample_event("aa11", "p1", 1, None, 10);
        store.ingest(&good).unwrap();

        let bad_path = store.event_path("bb22");
        fs::create_dir_all(bad_path.parent().unwrap()).unwrap();
        fs::write(&bad_path, [0xff, 0xfe, 0xfd]).unwrap();

        let removed = store.enforce_retention().unwrap();
        assert_eq!(removed, 0);
        assert!(store.event_path("aa11").exists());
    }

    #[test]
    fn refresh_stats_cache_counts_json_event_files_only() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        let ev1 = sample_event("aa11", "p1", 1, None, 10);
        let ev2 = sample_event("bb22", "p2", 1, None, 20);
        store.ingest(&ev1).unwrap();
        store.ingest(&ev2).unwrap();
        fs::write(dir.path().join("events/.DS_Store"), [0xff, 0xfe]).unwrap();

        let (count, bytes) = store.refresh_stats_cache().unwrap();
        assert_eq!(count, 2);
        assert!(bytes > 0);
        assert_eq!(
            fs::read_to_string(dir.path().join("runtime/events-count.cache"))
                .unwrap()
                .trim(),
            "2"
        );
    }

    #[test]
    fn ingest_with_limits_updates_stats_cache_without_full_recount() {
        let dir = TempDir::new().unwrap();
        let store = Store::with_limits(
            dir.path().to_path_buf(),
            false,
            Some(2_000),
            Some(1_000_000),
        );
        store.init().unwrap();
        fs::write(dir.path().join("runtime/events-count.cache"), "100").unwrap();
        fs::write(dir.path().join("runtime/events-bytes.cache"), "1000").unwrap();

        store
            .ingest(&sample_event("aa11", "p1", 1, None, 10))
            .unwrap();

        let count = fs::read_to_string(dir.path().join("runtime/events-count.cache"))
            .unwrap()
            .trim()
            .parse::<usize>()
            .unwrap();
        assert_eq!(count, 101);
    }

    #[test]
    fn retention_status_records_last_prune_result() {
        let dir = TempDir::new().unwrap();
        let store = Store::with_limits(dir.path().to_path_buf(), false, Some(2), None);
        store.init().unwrap();

        store
            .ingest(&sample_event("aa11", "p1", 1, None, 10))
            .unwrap();
        store
            .ingest(&sample_event("bb22", "p1", 1, None, 20))
            .unwrap();
        store
            .ingest(&sample_event("cc33", "p1", 1, None, 30))
            .unwrap();
        store.enforce_retention().unwrap();

        let status = store.retention_status().unwrap();
        assert_eq!(status.state, "ok");
        assert_eq!(status.current_events, 2);
        assert_eq!(status.max_events, Some(2));
        assert_eq!(status.last_prune_removed, Some(1));
        assert!(status.last_prune_at.is_some());
        assert!(status.warning.is_none());
    }

    #[test]
    fn backup_and_restore_round_trip_authoritative_store_data() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("source");
        let restore_root = dir.path().join("restore");
        let backup_root = dir.path().join("backup");

        let source = Store::new(source_root.clone(), false);
        source.init().unwrap();
        let event = sample_event("aa11", "p1", 1, None, 10);
        source.ingest(&event).unwrap();
        fs::create_dir_all(source_root.join("admin")).unwrap();
        fs::write(source_root.join("admin/pubkeys.allow"), "p1\n").unwrap();
        fs::create_dir_all(source_root.join("cursor")).unwrap();
        fs::write(source_root.join("cursor/example.since"), "123\n").unwrap();

        let manifest = source.backup_to(&backup_root).unwrap();
        assert_eq!(manifest.format, "stonr-store-snapshot-v1");
        assert!(backup_root.join("events").exists());
        assert!(backup_root.join("admin/pubkeys.allow").exists());
        assert!(backup_root.join("cursor/example.since").exists());

        let restored = Store::new(restore_root.clone(), false);
        restored.restore_from(&backup_root).unwrap();
        let events = restored
            .query(Query {
                authors: Some(vec!["p1".into()]),
                kinds: Some(vec![1]),
                d: None,
                t: None,
                tags: vec![],
                search: None,
                since: None,
                until: None,
                limit: Some(10),
            })
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "aa11");
        assert_eq!(
            fs::read_to_string(restore_root.join("admin/pubkeys.allow")).unwrap(),
            "p1\n"
        );
        assert_eq!(
            fs::read_to_string(restore_root.join("cursor/example.since")).unwrap(),
            "123\n"
        );
        assert_eq!(
            fs::read_to_string(restore_root.join("runtime/events-count.cache"))
                .unwrap()
                .trim(),
            "1"
        );
    }

    #[test]
    fn delete_event_hides_target_event_by_id() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        let target = sample_event("aa11", "p1", 1, None, 10);
        let delete = Event {
            id: "dd55".into(),
            pubkey: "p1".into(),
            kind: 5,
            created_at: 20,
            tags: vec![Tag(vec!["e".into(), "aa11".into()])],
            content: String::new(),
            sig: String::new(),
        };
        store.ingest(&target).unwrap();
        store.ingest_with_policy(&delete, true, true).unwrap();

        let visible = store
            .query_with_policy(
                Query {
                    authors: Some(vec!["p1".into()]),
                    kinds: Some(vec![1]),
                    d: None,
                    t: None,
                    tags: vec![],
                    search: None,
                    since: None,
                    until: None,
                    limit: None,
                },
                true,
                true,
            )
            .unwrap();
        assert!(visible.is_empty());
        assert!(store.event_path("aa11").exists());
    }

    #[test]
    fn delete_event_does_not_hide_pinned_owner_content() {
        let dir = TempDir::new().unwrap();
        let store = Store::with_limits_and_pins(
            dir.path().to_path_buf(),
            false,
            None,
            None,
            vec!["p1".into()],
            vec![],
            true,
        );
        store.init().unwrap();

        let target = sample_event("aa11", "p1", 1, None, 10);
        let delete = Event {
            id: "dd55".into(),
            pubkey: "p1".into(),
            kind: 5,
            created_at: 20,
            tags: vec![Tag(vec!["e".into(), "aa11".into()])],
            content: String::new(),
            sig: String::new(),
        };
        store.ingest(&target).unwrap();
        store.ingest_with_policy(&delete, true, true).unwrap();

        let visible = store
            .query_with_policy(
                Query {
                    authors: Some(vec!["p1".into()]),
                    kinds: Some(vec![1, 5]),
                    d: None,
                    t: None,
                    tags: vec![],
                    search: None,
                    since: None,
                    until: None,
                    limit: None,
                },
                true,
                true,
            )
            .unwrap();
        let ids: Vec<String> = visible.into_iter().map(|event| event.id).collect();
        assert_eq!(ids, vec!["dd55".to_string(), "aa11".to_string()]);
    }

    #[test]
    fn delete_by_address_hides_old_replaceable_but_keeps_newer_one() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        let old_post = sample_event("aa11", "p1", 30023, Some("slug"), 10);
        let delete = Event {
            id: "dd55".into(),
            pubkey: "p1".into(),
            kind: 5,
            created_at: 20,
            tags: vec![Tag(vec!["a".into(), "30023:p1:slug".into()])],
            content: String::new(),
            sig: String::new(),
        };
        let new_post = sample_event("bb22", "p1", 30023, Some("slug"), 30);

        store.ingest(&old_post).unwrap();
        store.ingest_with_policy(&delete, true, true).unwrap();
        store.ingest(&new_post).unwrap();

        let visible = store
            .query_with_policy(
                Query {
                    authors: Some(vec!["p1".into()]),
                    kinds: Some(vec![30023]),
                    d: Some("slug".into()),
                    t: None,
                    tags: vec![],
                    search: None,
                    since: None,
                    until: None,
                    limit: None,
                },
                true,
                true,
            )
            .unwrap();
        let ids: Vec<String> = visible.into_iter().map(|event| event.id).collect();
        assert_eq!(ids, vec!["bb22".to_string()]);
    }

    #[test]
    fn expiration_hides_expired_events() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        let expired = Event {
            id: "aa11".into(),
            pubkey: "p1".into(),
            kind: 1,
            created_at: 1,
            tags: vec![Tag(vec!["expiration".into(), "1".into()])],
            content: String::new(),
            sig: String::new(),
        };
        store.ingest(&expired).unwrap();

        let visible = store
            .query_with_policy(
                Query {
                    authors: Some(vec!["p1".into()]),
                    kinds: Some(vec![1]),
                    d: None,
                    t: None,
                    tags: vec![],
                    search: None,
                    since: None,
                    until: None,
                    limit: None,
                },
                true,
                true,
            )
            .unwrap();
        assert!(visible.is_empty());
    }

    #[test]
    fn ingest_with_policy_skips_already_expired_events() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        let expired = Event {
            id: "aa11".into(),
            pubkey: "p1".into(),
            kind: 1,
            created_at: 1,
            tags: vec![Tag(vec!["expiration".into(), "1".into()])],
            content: String::new(),
            sig: String::new(),
        };

        let stored = store.ingest_with_policy(&expired, true, true).unwrap();
        assert!(!stored);
        assert!(!store.event_path("aa11").exists());
    }

    #[test]
    #[ignore = "performance smoke; run in release with --ignored --nocapture"]
    fn large_store_perf_smoke() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        let total = 20_000usize;
        for idx in 0..total {
            let id = format!("{idx:064x}");
            let created_at = idx as u64 + 1;
            let mut event = Event {
                id,
                pubkey: "author".into(),
                kind: if idx % 50 == 0 { 30023 } else { 1 },
                created_at,
                tags: vec![],
                content: if idx % 97 == 0 {
                    format!("keyword event payload {idx}")
                } else {
                    format!("plain event payload {idx}")
                },
                sig: String::new(),
            };
            if idx % 50 == 0 {
                event
                    .tags
                    .push(Tag(vec!["d".into(), format!("slug-{idx}")]));
            }
            if idx % 17 == 0 {
                event.tags.push(Tag(vec!["t".into(), "topic".into()]));
            }
            store.ingest(&event).unwrap();
        }

        let stats_start = Instant::now();
        let (count, bytes) = store.refresh_stats_cache().unwrap();
        let stats_elapsed = stats_start.elapsed();

        let recent_start = Instant::now();
        let recent = store
            .query_with_policy(
                Query {
                    authors: None,
                    kinds: None,
                    d: None,
                    t: None,
                    tags: vec![],
                    search: None,
                    since: None,
                    until: None,
                    limit: Some(60),
                },
                false,
                false,
            )
            .unwrap();
        let recent_elapsed = recent_start.elapsed();

        let search_start = Instant::now();
        let searched = store
            .query_with_policy(
                Query {
                    authors: None,
                    kinds: None,
                    d: None,
                    t: None,
                    tags: vec![],
                    search: Some("keyword".into()),
                    since: None,
                    until: None,
                    limit: Some(60),
                },
                false,
                false,
            )
            .unwrap();
        let search_elapsed = search_start.elapsed();

        println!(
            "large_store_perf_smoke total_events={} total_bytes={} refresh_stats_ms={} recent_query_ms={} search_query_ms={}",
            count,
            bytes,
            stats_elapsed.as_millis(),
            recent_elapsed.as_millis(),
            search_elapsed.as_millis()
        );

        assert_eq!(count, total);
        assert_eq!(recent.len(), 60);
        assert!(!searched.is_empty());
        assert!(
            stats_elapsed.as_secs_f64() < 5.0,
            "refresh_stats_cache too slow: {:?}",
            stats_elapsed
        );
        assert!(
            recent_elapsed.as_secs_f64() < 2.0,
            "recent query too slow: {:?}",
            recent_elapsed
        );
        assert!(
            search_elapsed.as_secs_f64() < 4.0,
            "search query too slow: {:?}",
            search_elapsed
        );
    }
}
