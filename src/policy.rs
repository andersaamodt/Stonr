//! Backend enforcement for relay event policy and file-backed client rate limits.

use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Result};
use serde_json::to_vec;
use sha1::{Digest, Sha1};

use crate::{config::Settings, event::Event, files::FileStore, storage::Query};

#[derive(Clone, Copy)]
pub enum RateLimitAction {
    Query,
    Count,
    Publish,
}

impl RateLimitAction {
    fn key(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Count => "count",
            Self::Publish => "publish",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Query => "reads",
            Self::Count => "counts",
            Self::Publish => "writes",
        }
    }

    fn max_per_window(self, settings: &Settings) -> Option<usize> {
        match self {
            Self::Query => settings.max_queries_per_window,
            Self::Count => settings.max_counts_per_window,
            Self::Publish => settings.max_publishes_per_window,
        }
    }
}

pub fn apply_query_policy(settings: &Settings, mut query: Query) -> Query {
    if let Some(max_limit) = settings.max_limit {
        let requested = query.limit.unwrap_or(max_limit);
        query.limit = Some(requested.min(max_limit));
    }
    query
}

pub fn validate_event(settings: &Settings, event: &Event, now: u64) -> Result<()> {
    if settings.filter_private_messages && is_private_message_kind(event.kind) {
        return Err(anyhow!("encrypted private messages are filtered"));
    }
    if settings
        .blocked_pubkeys
        .as_ref()
        .is_some_and(|pubkeys| pubkeys.iter().any(|pubkey| pubkey == &event.pubkey))
    {
        return Err(anyhow!("author is blocked"));
    }
    if settings
        .allowed_pubkeys
        .as_ref()
        .is_some_and(|pubkeys| !pubkeys.iter().any(|pubkey| pubkey == &event.pubkey))
    {
        return Err(anyhow!("author is not allowed"));
    }
    if settings
        .blocked_kinds
        .as_ref()
        .is_some_and(|kinds| kinds.contains(&event.kind))
    {
        return Err(anyhow!("event kind is blocked"));
    }
    if settings
        .allowed_kinds
        .as_ref()
        .is_some_and(|kinds| !kinds.contains(&event.kind))
    {
        return Err(anyhow!("event kind is not allowed"));
    }
    if let Some(max_event_bytes) = settings.max_event_bytes {
        let size = to_vec(event)?.len();
        if size > max_event_bytes {
            return Err(anyhow!("event exceeds max size"));
        }
    }
    if let Some(max_event_age_secs) = settings.max_event_age_secs {
        if now.saturating_sub(event.created_at) > max_event_age_secs {
            return Err(anyhow!("event is too old"));
        }
    }
    if let Some(max_event_future_secs) = settings.max_event_future_secs {
        if event.created_at > now.saturating_add(max_event_future_secs) {
            return Err(anyhow!("event is too far in the future"));
        }
    }
    Ok(())
}

pub fn validate_event_with_files(
    settings: &Settings,
    file_store: &FileStore,
    event: &Event,
    now: u64,
) -> Result<()> {
    validate_event(settings, event, now)?;
    if event.kind == 1063 {
        file_store.validate_metadata_event(event, settings)?;
    }
    Ok(())
}

pub fn enforce_rate_limit(
    settings: &Settings,
    action: RateLimitAction,
    actor: &str,
    now: u64,
) -> Result<()> {
    let Some(window_secs) = settings.rate_limit_window_secs else {
        return Ok(());
    };
    let Some(max_per_window) = action.max_per_window(settings) else {
        return Ok(());
    };
    let runtime_dir = settings
        .store_root
        .join("runtime")
        .join("rate-limit")
        .join(action.key());
    fs::create_dir_all(&runtime_dir)?;
    let actor_key = hashed_actor_key(actor);
    let bucket = now / window_secs;
    let count_path = runtime_dir.join(format!("{bucket}-{actor_key}.txt"));
    let _lock = rate_limit_lock(runtime_dir.join(format!(".{actor_key}.lock")))?;
    let current = fs::read_to_string(&count_path)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    if current >= max_per_window {
        return Err(anyhow!("rate limit exceeded for {}", action.label()));
    }
    fs::write(&count_path, current.saturating_add(1).to_string())?;
    Ok(())
}

pub fn client_actor_label(peer_addr: Option<std::net::SocketAddr>, auth_actor: Option<&str>) -> String {
    if let Some(actor) = auth_actor {
        return format!("pubkey:{actor}");
    }
    peer_addr
        .map(|addr| format!("ip:{}", addr.ip()))
        .unwrap_or_else(|| "local".to_string())
}

pub fn is_private_message_kind(kind: u32) -> bool {
    matches!(kind, 4 | 13 | 14 | 15 | 1059)
}

pub fn current_unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

struct RateLimitLock {
    path: PathBuf,
}

impl Drop for RateLimitLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

fn rate_limit_lock(path: PathBuf) -> Result<RateLimitLock> {
    for _ in 0..200 {
        match fs::create_dir(&path) {
            Ok(()) => return Ok(RateLimitLock { path }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(anyhow!("timed out waiting for rate-limit lock"))
}

fn hashed_actor_key(actor: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(actor.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::config::{MirrorMode, SinceMode};

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
            filter_since_mode: SinceMode::Cursor,
            mirror_mode: MirrorMode::Broad,
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
            file_keep_mode: crate::config::FileKeepMode::Referenced,
            max_blob_bytes_per_pubkey: None,
        }
    }

    fn event(pubkey: &str, kind: u32, created_at: u64, content: &str) -> Event {
        Event {
            id: "id".into(),
            pubkey: pubkey.into(),
            kind,
            created_at,
            tags: vec![],
            content: content.into(),
            sig: "sig".into(),
        }
    }

    #[test]
    fn apply_query_policy_caps_limit() {
        let dir = tempdir().unwrap();
        let mut settings = settings(dir.path());
        settings.max_limit = Some(10);
        let capped = apply_query_policy(
            &settings,
            Query {
                authors: None,
                kinds: None,
                d: None,
                t: None,
                tags: vec![],
                search: None,
                since: None,
                until: None,
                limit: Some(50),
            },
        );
        assert_eq!(capped.limit, Some(10));
    }

    #[test]
    fn validate_event_rejects_disallowed_author_kind_and_size() {
        let dir = tempdir().unwrap();
        let mut settings = settings(dir.path());
        settings.allowed_pubkeys = Some(vec!["allowed".into()]);
        settings.blocked_kinds = Some(vec![1]);
        settings.max_event_bytes = Some(10);
        let event = event("blocked", 1, 100, "payload that is too large");
        let error = validate_event(&settings, &event, 100).unwrap_err().to_string();
        assert!(error.contains("author is not allowed"));
    }

    #[test]
    fn validate_event_rejects_old_and_future_events() {
        let dir = tempdir().unwrap();
        let mut settings = settings(dir.path());
        settings.max_event_age_secs = Some(10);
        let error = validate_event(&settings, &event("pub", 1, 1, "x"), 20)
            .unwrap_err()
            .to_string();
        assert!(error.contains("too old"));

        settings.max_event_age_secs = None;
        settings.max_event_future_secs = Some(5);
        let error = validate_event(&settings, &event("pub", 1, 30, "x"), 20)
            .unwrap_err()
            .to_string();
        assert!(error.contains("too far in the future"));
    }

    #[test]
    fn enforce_rate_limit_blocks_after_max() {
        let dir = tempdir().unwrap();
        let mut settings = settings(dir.path());
        settings.rate_limit_window_secs = Some(60);
        settings.max_queries_per_window = Some(2);
        enforce_rate_limit(&settings, RateLimitAction::Query, "ip:127.0.0.1", 120).unwrap();
        enforce_rate_limit(&settings, RateLimitAction::Query, "ip:127.0.0.1", 120).unwrap();
        let error =
            enforce_rate_limit(&settings, RateLimitAction::Query, "ip:127.0.0.1", 120)
                .unwrap_err()
                .to_string();
        assert!(error.contains("rate limit exceeded"));
    }
}
