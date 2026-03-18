//! Configuration loading from `.env` files.

use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

/// Runtime settings derived from environment variables.
#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    /// Root directory for all storage.
    pub store_root: PathBuf,
    /// HTTP bind address, e.g. `127.0.0.1:7777`.
    pub bind_http: String,
    /// WebSocket bind address, e.g. `127.0.0.1:7778`.
    pub bind_ws: String,
    /// Enable Schnorr signature verification on ingest.
    pub verify_sig: bool,
    /// Relay profile name exposed over NIP-11.
    pub relay_name: String,
    /// Relay profile description exposed over NIP-11.
    pub relay_description: String,
    /// Publish a relay information document over HTTP.
    pub enable_nip11: bool,
    /// Allow read/query access.
    pub enable_query: bool,
    /// Allow EVENT publish over relay interfaces.
    pub enable_publish: bool,
    /// Keep WS subscriptions open for live fanout.
    pub enable_live_subscriptions: bool,
    /// Allow COUNT requests.
    pub enable_count: bool,
    /// Allow tag-based queries like `#a`, `#e`, `#p`, `#t`.
    pub enable_tag_queries: bool,
    /// Allow relay-side content search.
    pub enable_search: bool,
    /// Enable upstream mirroring when relays are configured.
    pub enable_mirroring: bool,
    /// Master switch for NIP-11.
    pub support_nip11: bool,
    /// Master switch for NIP-09 delete handling.
    pub support_nip09: bool,
    /// Master switch for NIP-12.
    pub support_nip12: bool,
    /// Master switch for NIP-40 expiration handling.
    pub support_nip40: bool,
    /// Master switch for NIP-45.
    pub support_nip45: bool,
    /// Master switch for NIP-50.
    pub support_nip50: bool,
    /// Reject encrypted private-message kinds before storing them.
    pub filter_private_messages: bool,
    /// Upstream relays to mirror events from.
    pub relays_upstream: Vec<String>,
    /// Optional Tor SOCKS proxy (host:port).
    pub tor_socks: Option<String>,
    /// Optional author filters for mirroring.
    pub filter_authors: Option<Vec<String>>,
    /// Optional kind filters for mirroring.
    pub filter_kinds: Option<Vec<u32>>,
    /// Optional `#t` tag filters for mirroring.
    pub filter_tag_t: Option<Vec<String>>,
    /// Optional `#a` tag filters for mirroring.
    pub filter_tag_a: Option<Vec<String>>,
    /// Strategy for determining the starting timestamp when mirroring.
    pub filter_since_mode: SinceMode,
    /// High-level mirror behavior.
    pub mirror_mode: MirrorMode,
    /// In site mirror mode, the site author's pubkey.
    pub mirror_site_author: Option<String>,
    /// In site mirror mode, whether to mirror kind 1 comments that reference imported posts.
    pub mirror_site_include_comments: bool,
    /// Optional maximum number of stored events.
    pub max_stored_events: Option<usize>,
    /// Optional maximum total bytes for stored event files.
    pub max_stored_event_bytes: Option<u64>,
}

/// Determines how the mirroring process derives the `since` value for subscriptions.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SinceMode {
    /// Resume from the last cursor stored per relay.
    Cursor,
    /// Start from a fixed Unix timestamp.
    Fixed(u64),
}

/// High-level mirroring behavior.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum MirrorMode {
    Broad,
    Site,
}

impl Settings {
    /// Load settings from the specified `.env` file.
    pub fn from_env(path: &str) -> Result<Self> {
        let env = parse_env_file(path).context("reading env file")?;
        let store_root = PathBuf::from(required_env(&env, "STORE_ROOT")?);
        let bind_http = required_env(&env, "BIND_HTTP")?;
        let bind_ws = required_env(&env, "BIND_WS")?;
        let verify_sig = env_value(&env, "VERIFY_SIG").unwrap_or("0") == "1";
        let relay_name = env_value(&env, "RELAY_NAME").unwrap_or("stonr").to_string();
        let relay_description = env_value(&env, "RELAY_DESCRIPTION")
            .unwrap_or("File-backed Nostr relay")
            .to_string();
        let enable_nip11 = env_value(&env, "ENABLE_NIP11").unwrap_or("1") == "1";
        let enable_query = env_value(&env, "ENABLE_QUERY").unwrap_or("1") == "1";
        let enable_publish = env_value(&env, "ENABLE_PUBLISH").unwrap_or("1") == "1";
        let enable_live_subscriptions =
            env_value(&env, "ENABLE_LIVE_SUBSCRIPTIONS").unwrap_or("1") == "1";
        let enable_count = env_value(&env, "ENABLE_COUNT").unwrap_or("1") == "1";
        let enable_tag_queries = env_value(&env, "ENABLE_TAG_QUERIES").unwrap_or("1") == "1";
        let enable_search = env_value(&env, "ENABLE_SEARCH").unwrap_or("1") == "1";
        let enable_mirroring = env_value(&env, "ENABLE_MIRRORING").unwrap_or("1") == "1";
        let support_nip11 = env_value(&env, "SUPPORT_NIP11").unwrap_or("1") == "1";
        let support_nip09 = env_value(&env, "SUPPORT_NIP09").unwrap_or("1") == "1";
        let support_nip12 = env_value(&env, "SUPPORT_NIP12").unwrap_or("1") == "1";
        let support_nip40 = env_value(&env, "SUPPORT_NIP40").unwrap_or("1") == "1";
        let support_nip45 = env_value(&env, "SUPPORT_NIP45").unwrap_or("1") == "1";
        let support_nip50 = env_value(&env, "SUPPORT_NIP50").unwrap_or("1") == "1";
        let filter_private_messages = env_value(&env, "FILTER_PRIVATE_MESSAGES").unwrap_or("1") == "1";
        let relays_upstream = csv_strings(env_value(&env, "RELAYS_UPSTREAM").unwrap_or_default());
        let tor_socks = env_value(&env, "TOR_SOCKS").filter(|s| !s.is_empty()).map(str::to_string);
        let filter_authors = env_value(&env, "FILTER_AUTHORS").and_then(|s| {
            let v = csv_strings(s);
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        });
        let filter_kinds = env_value(&env, "FILTER_KINDS").and_then(|s| {
            let v = csv_u32(s);
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        });
        let filter_tag_t = env_value(&env, "FILTER_TAG_T").and_then(|s| {
            let v = csv_strings(s);
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        });
        let filter_tag_a = env_value(&env, "FILTER_TAG_A").and_then(|s| {
            let v = csv_strings(s);
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        });
        let since_str = env_value(&env, "FILTER_SINCE_MODE").unwrap_or("cursor");
        let filter_since_mode = if let Some(rest) = since_str.strip_prefix("fixed:") {
            SinceMode::Fixed(rest.parse().unwrap_or(0))
        } else {
            SinceMode::Cursor
        };
        let mirror_mode = match env_value(&env, "MIRROR_MODE").unwrap_or("broad") {
            "site" => MirrorMode::Site,
            _ => MirrorMode::Broad,
        };
        let mirror_site_author = env_value(&env, "MIRROR_SITE_AUTHOR")
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let mirror_site_include_comments =
            env_value(&env, "MIRROR_SITE_INCLUDE_COMMENTS").unwrap_or("1") == "1";
        let max_stored_events = env_value(&env, "MAX_STORED_EVENTS")
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|value| *value > 0);
        let max_stored_event_bytes = env_value(&env, "MAX_STORED_EVENT_BYTES")
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|value| *value > 0);
        Ok(Self {
            store_root,
            bind_http,
            bind_ws,
            verify_sig,
            relay_name,
            relay_description,
            enable_nip11,
            enable_query,
            enable_publish,
            enable_live_subscriptions,
            enable_count,
            enable_tag_queries,
            enable_search,
            enable_mirroring,
            support_nip11,
            support_nip09,
            support_nip12,
            support_nip40,
            support_nip45,
            support_nip50,
            filter_private_messages,
            relays_upstream,
            tor_socks,
            filter_authors,
            filter_kinds,
            filter_tag_t,
            filter_tag_a,
            filter_since_mode,
            mirror_mode,
            mirror_site_author,
            mirror_site_include_comments,
            max_stored_events,
            max_stored_event_bytes,
        })
    }

    pub fn relay_info_enabled(&self) -> bool {
        self.enable_nip11 && self.support_nip11
    }

    pub fn delete_enabled(&self) -> bool {
        self.support_nip09
    }

    pub fn expiration_enabled(&self) -> bool {
        self.support_nip40
    }

    pub fn query_enabled(&self) -> bool {
        self.enable_query
    }

    pub fn publish_enabled(&self) -> bool {
        self.enable_publish
    }

    pub fn live_subscriptions_enabled(&self) -> bool {
        self.enable_live_subscriptions
    }

    pub fn count_enabled(&self) -> bool {
        self.enable_count && self.support_nip45
    }

    pub fn tag_queries_enabled(&self) -> bool {
        self.enable_tag_queries && self.support_nip12
    }

    pub fn search_enabled(&self) -> bool {
        self.enable_search && self.support_nip50
    }
}

fn parse_env_file(path: &str) -> Result<HashMap<String, String>> {
    let data = fs::read_to_string(path)?;
    let mut values = HashMap::new();
    for line in data.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            values.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    Ok(values)
}

fn env_value<'a>(env: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    env.get(key).map(String::as_str)
}

fn required_env(env: &HashMap<String, String>, key: &str) -> Result<String> {
    env.get(key)
        .cloned()
        .context(format!("missing required field: {key}"))
}

/// Split a comma-separated string into trimmed string values.
pub fn csv_strings(input: impl AsRef<str>) -> Vec<String> {
    let s = input.as_ref();
    s.split(',')
        .filter_map(|s| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
        .collect()
}

/// Split a comma-separated string into `u32` values, skipping invalid entries.
pub fn csv_u32(input: impl AsRef<str>) -> Vec<u32> {
    let s = input.as_ref();
    s.split(',').filter_map(|s| s.trim().parse().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, sync::Mutex};
    use tempfile::tempdir;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn loads_env() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n",
                "VERIFY_SIG=1\n",
                "RELAYS_UPSTREAM=ws://r1,ws://r2\n",
                "TOR_SOCKS=\n",
                "FILTER_AUTHORS=npub1\n",
                "FILTER_KINDS=1,30023\n",
                "FILTER_TAG_T=essay\n",
                "FILTER_TAG_A=30023:pubkey:slug\n",
                "FILTER_SINCE_MODE=fixed:1700000000\n"
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.bind_http, "127.0.0.1:8080");
        assert_eq!(cfg.bind_ws, "127.0.0.1:8081");
        assert_eq!(cfg.store_root, PathBuf::from("/tmp"));
        assert!(cfg.verify_sig);
        assert_eq!(cfg.relay_name, "stonr");
        assert_eq!(cfg.relay_description, "File-backed Nostr relay");
        assert!(cfg.enable_nip11);
        assert!(cfg.enable_query);
        assert!(cfg.enable_publish);
        assert!(cfg.enable_live_subscriptions);
        assert!(cfg.enable_count);
        assert!(cfg.enable_tag_queries);
        assert!(cfg.enable_search);
        assert!(cfg.enable_mirroring);
        assert!(cfg.support_nip11);
        assert!(cfg.support_nip12);
        assert!(cfg.support_nip45);
        assert!(cfg.support_nip50);
        assert!(cfg.filter_private_messages);
        assert_eq!(cfg.relays_upstream.len(), 2);
        assert_eq!(
            cfg.filter_authors.as_ref().unwrap(),
            &vec![String::from("npub1")]
        );
        assert_eq!(cfg.filter_kinds.as_ref().unwrap(), &vec![1, 30023]);
        assert_eq!(
            cfg.filter_tag_t.as_ref().unwrap(),
            &vec![String::from("essay")]
        );
        assert_eq!(
            cfg.filter_tag_a.as_ref().unwrap(),
            &vec![String::from("30023:pubkey:slug")]
        );
        assert_eq!(cfg.filter_since_mode, SinceMode::Fixed(1700000000));
        assert_eq!(cfg.mirror_mode, MirrorMode::Broad);
        assert!(cfg.mirror_site_author.is_none());
        assert!(cfg.mirror_site_include_comments);
        assert_eq!(cfg.max_stored_events, None);
        assert_eq!(cfg.max_stored_event_bytes, None);
    }

    #[test]
    fn csv_helpers() {
        assert_eq!(csv_strings("a, b , ,c"), vec!["a", "b", "c"]);
        assert!(csv_strings("").is_empty());
        assert_eq!(csv_u32("1, 2, x,3"), vec![1, 2, 3]);
        assert!(csv_u32("").is_empty());
    }

    #[test]
    fn tor_socks_parsed() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n",
                "TOR_SOCKS=127.0.0.1:9050\n",
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.tor_socks, Some("127.0.0.1:9050".into()));
    }

    #[test]
    fn defaults_when_optional_absent() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n"
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert!(cfg.relays_upstream.is_empty());
        assert!(cfg.tor_socks.is_none());
        assert_eq!(cfg.relay_name, "stonr");
        assert_eq!(cfg.relay_description, "File-backed Nostr relay");
        assert!(cfg.enable_nip11);
        assert!(cfg.enable_query);
        assert!(cfg.enable_publish);
        assert!(cfg.enable_live_subscriptions);
        assert!(cfg.enable_count);
        assert!(cfg.enable_tag_queries);
        assert!(cfg.enable_search);
        assert!(cfg.enable_mirroring);
        assert!(cfg.support_nip11);
        assert!(cfg.support_nip12);
        assert!(cfg.support_nip45);
        assert!(cfg.support_nip50);
        assert!(cfg.filter_private_messages);
        assert!(cfg.filter_authors.is_none());
        assert!(cfg.filter_kinds.is_none());
        assert!(cfg.filter_tag_t.is_none());
        assert!(cfg.filter_tag_a.is_none());
        assert_eq!(cfg.filter_since_mode, SinceMode::Cursor);
        assert_eq!(cfg.mirror_mode, MirrorMode::Broad);
        assert!(cfg.mirror_site_author.is_none());
        assert!(cfg.mirror_site_include_comments);
        assert!(cfg.max_stored_events.is_none());
        assert!(cfg.max_stored_event_bytes.is_none());
    }

    #[test]
    fn empty_filters_are_none() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n",
                "FILTER_AUTHORS=\n",
                "FILTER_KINDS=\n",
                "FILTER_TAG_T=\n",
                "FILTER_TAG_A=\n",
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert!(cfg.filter_authors.is_none());
        assert!(cfg.filter_kinds.is_none());
        assert!(cfg.filter_tag_t.is_none());
        assert!(cfg.filter_tag_a.is_none());
    }

    #[test]
    fn missing_required_fields_error() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!("BIND_HTTP=127.0.0.1:8080\n", "BIND_WS=127.0.0.1:8081\n"),
        )
        .unwrap();
        assert!(Settings::from_env(env_path.to_str().unwrap()).is_err());
    }

    #[test]
    fn invalid_fixed_since_mode_defaults_to_zero() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n",
                "FILTER_SINCE_MODE=fixed:notanumber\n",
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.filter_since_mode, SinceMode::Fixed(0));
    }

    #[test]
    fn retention_limits_parse() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n",
                "MAX_STORED_EVENTS=12\n",
                "MAX_STORED_EVENT_BYTES=1048576\n",
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.max_stored_events, Some(12));
        assert_eq!(cfg.max_stored_event_bytes, Some(1_048_576));
    }

    #[test]
    fn unquoted_spaces_in_values_are_accepted() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n",
                "RELAY_DESCRIPTION=First file-backed relay!\n",
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.bind_http, "127.0.0.1:8080");
        assert_eq!(cfg.relay_description, "First file-backed relay!");
    }

    #[test]
    fn filter_private_messages_can_be_disabled() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n",
                "FILTER_PRIVATE_MESSAGES=0\n"
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert!(!cfg.filter_private_messages);
    }

    #[test]
    fn site_mirror_mode_parses() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n",
                "MIRROR_MODE=site\n",
                "MIRROR_SITE_AUTHOR=abcdef\n",
                "MIRROR_SITE_INCLUDE_COMMENTS=0\n"
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.mirror_mode, MirrorMode::Site);
        assert_eq!(cfg.mirror_site_author.as_deref(), Some("abcdef"));
        assert!(!cfg.mirror_site_include_comments);
    }

    #[test]
    fn relay_capability_toggles_parse() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n",
                "RELAY_NAME=My relay\n",
                "ENABLE_NIP11=0\n",
                "ENABLE_QUERY=0\n",
                "ENABLE_PUBLISH=0\n",
                "ENABLE_LIVE_SUBSCRIPTIONS=0\n",
                "ENABLE_COUNT=0\n",
                "ENABLE_TAG_QUERIES=0\n",
                "ENABLE_SEARCH=0\n",
                "ENABLE_MIRRORING=0\n",
                "SUPPORT_NIP11=0\n",
                "SUPPORT_NIP12=0\n",
                "SUPPORT_NIP45=0\n",
                "SUPPORT_NIP50=0\n",
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.relay_name, "My relay");
        assert!(!cfg.relay_info_enabled());
        assert!(!cfg.query_enabled());
        assert!(!cfg.publish_enabled());
        assert!(!cfg.live_subscriptions_enabled());
        assert!(!cfg.count_enabled());
        assert!(!cfg.tag_queries_enabled());
        assert!(!cfg.search_enabled());
        assert!(!cfg.enable_mirroring);
    }
}
