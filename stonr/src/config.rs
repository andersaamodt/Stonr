//! Configuration loading from `.env` files.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FileKeepMode {
    Referenced,
    All,
}

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
    /// Allow only these event kinds when set.
    pub allowed_kinds: Option<Vec<u32>>,
    /// Reject these event kinds when set.
    pub blocked_kinds: Option<Vec<u32>>,
    /// Allow only these author pubkeys when set.
    pub allowed_pubkeys: Option<Vec<String>>,
    /// Reject these author pubkeys when set.
    pub blocked_pubkeys: Option<Vec<String>>,
    /// Owner pubkeys that are privileged on write and always retained.
    pub owner_pubkeys: Option<Vec<String>>,
    /// Followed pubkeys that should be mirrored and retained.
    pub follow_pubkeys: Option<Vec<String>>,
    /// Explicit event IDs that should never be removed by retention.
    pub pinned_event_ids: Option<Vec<String>>,
    /// Keep pinned content visible even when delete events target it.
    pub protect_pinned_from_deletes: bool,
    /// Enable NIP-42 relay authentication.
    pub enable_nip42: bool,
    /// Require authenticated sessions for read/query access.
    pub require_auth_for_query: bool,
    /// Require authenticated sessions for COUNT access.
    pub require_auth_for_count: bool,
    /// Require authenticated sessions for EVENT publish.
    pub require_auth_for_publish: bool,
    /// When publishing with relay auth, require the session pubkey to match the event pubkey.
    pub auth_must_match_event_pubkey: bool,
    /// Maximum acceptable AUTH event age in seconds.
    pub auth_max_age_secs: u64,
    /// Master switch for NIP-11.
    pub support_nip11: bool,
    /// Master switch for NIP-09 delete handling.
    pub support_nip09: bool,
    /// Master switch for NIP-12.
    pub support_nip12: bool,
    /// Master switch for NIP-42 relay authentication.
    pub support_nip42: bool,
    /// Master switch for NIP-40 expiration handling.
    pub support_nip40: bool,
    /// Master switch for NIP-45.
    pub support_nip45: bool,
    /// Master switch for NIP-50.
    pub support_nip50: bool,
    /// Master switch for NIP-94 file metadata events.
    pub support_nip94: bool,
    /// Master switch for NIP-96 compatibility file API.
    pub support_nip96: bool,
    /// Master switch for NIP-98 HTTP auth.
    pub support_nip98: bool,
    /// Master switch for NIP-B7 Blossom blob APIs.
    pub support_nip_b7: bool,
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
    /// Optional maximum number of events returned by a single read query.
    pub max_limit: Option<usize>,
    /// Optional maximum serialized event size accepted for ingest.
    pub max_event_bytes: Option<usize>,
    /// Optional maximum accepted event age in seconds.
    pub max_event_age_secs: Option<u64>,
    /// Optional maximum future skew accepted for event timestamps.
    pub max_event_future_secs: Option<u64>,
    /// Optional rate-limit window in seconds.
    pub rate_limit_window_secs: Option<u64>,
    /// Optional maximum read queries per actor within one rate-limit window.
    pub max_queries_per_window: Option<usize>,
    /// Optional maximum COUNT queries per actor within one rate-limit window.
    pub max_counts_per_window: Option<usize>,
    /// Optional maximum publishes per actor within one rate-limit window.
    pub max_publishes_per_window: Option<usize>,
    /// Store file metadata events such as kind 1063.
    pub enable_file_metadata: bool,
    /// Offer the `/files` compatibility upload API.
    pub enable_file_api: bool,
    /// Offer the Blossom blob API.
    pub enable_blossom: bool,
    /// Allow authenticated owners to list stored blobs.
    pub enable_blossom_list: bool,
    /// Allow the server to copy remote blobs into local storage.
    pub enable_blossom_mirror: bool,
    /// Require NIP-98 HTTP auth for compatibility uploads.
    pub require_nip98_auth: bool,
    /// Require Blossom auth for upload/delete/list/mirror actions.
    pub require_blossom_auth: bool,
    /// Require Blossom auth for blob downloads.
    pub require_blossom_get_auth: bool,
    /// Public compatibility API URL override.
    pub file_api_url: Option<String>,
    /// Public Blossom origin override.
    pub blossom_public_url: Option<String>,
    /// Maximum accepted upload size in bytes.
    pub file_max_bytes: usize,
    /// Allow only these MIME patterns when set.
    pub file_allowed_mime: Option<Vec<String>>,
    /// Always reject these MIME patterns when set.
    pub file_blocked_mime: Option<Vec<String>>,
    /// Exact blob hashes that should always be rejected.
    pub file_hash_denylist: Option<Vec<String>>,
    /// Storage retention policy for local blobs.
    pub file_keep_mode: FileKeepMode,
    /// Maximum stored blob bytes per owner pubkey.
    pub max_blob_bytes_per_pubkey: Option<u64>,
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
        let mut env = parse_env_file(path).context("reading env file")?;
        crate::autoconfig::apply_env_overrides(Path::new(path), &mut env)
            .context("applying app support overrides")?;
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
        let allowed_kinds = env_value(&env, "ALLOW_KINDS").and_then(csv_u32_opt);
        let blocked_kinds = env_value(&env, "DENY_KINDS").and_then(csv_u32_opt);
        let allowed_pubkeys = env_value(&env, "ALLOW_PUBKEYS").and_then(csv_strings_opt);
        let blocked_pubkeys = env_value(&env, "DENY_PUBKEYS").and_then(csv_strings_opt);
        let owner_pubkeys = env_value(&env, "OWNER_PUBKEYS").and_then(csv_strings_opt);
        let follow_pubkeys = env_value(&env, "FOLLOW_PUBKEYS").and_then(csv_strings_opt);
        let pinned_event_ids = env_value(&env, "PIN_EVENT_IDS").and_then(csv_strings_opt);
        let protect_pinned_from_deletes =
            env_value(&env, "PIN_PROTECT_FROM_DELETES").unwrap_or("1") == "1";
        let enable_nip42 = env_value(&env, "ENABLE_NIP42").unwrap_or("0") == "1";
        let require_auth_for_query =
            env_value(&env, "REQUIRE_AUTH_FOR_QUERY").unwrap_or("0") == "1";
        let require_auth_for_count =
            env_value(&env, "REQUIRE_AUTH_FOR_COUNT").unwrap_or("0") == "1";
        let require_auth_for_publish =
            env_value(&env, "REQUIRE_AUTH_FOR_PUBLISH").unwrap_or("0") == "1";
        let auth_must_match_event_pubkey =
            env_value(&env, "AUTH_MUST_MATCH_EVENT_PUBKEY").unwrap_or("0") == "1";
        let auth_max_age_secs = env_value(&env, "AUTH_MAX_AGE_SECS")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(600);
        let support_nip11 = env_value(&env, "SUPPORT_NIP11").unwrap_or("1") == "1";
        let support_nip09 = env_value(&env, "SUPPORT_NIP09").unwrap_or("1") == "1";
        let support_nip12 = env_value(&env, "SUPPORT_NIP12").unwrap_or("1") == "1";
        let support_nip42 = env_value(&env, "SUPPORT_NIP42").unwrap_or("1") == "1";
        let support_nip40 = env_value(&env, "SUPPORT_NIP40").unwrap_or("1") == "1";
        let support_nip45 = env_value(&env, "SUPPORT_NIP45").unwrap_or("1") == "1";
        let support_nip50 = env_value(&env, "SUPPORT_NIP50").unwrap_or("1") == "1";
        let support_nip94 = env_value(&env, "SUPPORT_NIP94").unwrap_or("1") == "1";
        let support_nip96 = env_value(&env, "SUPPORT_NIP96").unwrap_or("1") == "1";
        let support_nip98 = env_value(&env, "SUPPORT_NIP98").unwrap_or("1") == "1";
        let support_nip_b7 = env_value(&env, "SUPPORT_NIP_B7").unwrap_or("1") == "1";
        let filter_private_messages =
            env_value(&env, "FILTER_PRIVATE_MESSAGES").unwrap_or("1") == "1";
        let relays_upstream = csv_strings(env_value(&env, "RELAYS_UPSTREAM").unwrap_or_default());
        let tor_socks = env_value(&env, "TOR_SOCKS")
            .filter(|s| !s.is_empty())
            .map(str::to_string);
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
        let max_limit = env_value(&env, "MAX_LIMIT")
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|value| *value > 0);
        let max_event_bytes = env_value(&env, "MAX_EVENT_BYTES")
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|value| *value > 0);
        let max_event_age_secs = env_value(&env, "MAX_EVENT_AGE_SECS")
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|value| *value > 0);
        let max_event_future_secs = env_value(&env, "MAX_EVENT_FUTURE_SECS")
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|value| *value > 0);
        let rate_limit_window_secs = env_value(&env, "RATE_LIMIT_WINDOW_SECS")
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|value| *value > 0);
        let max_queries_per_window = env_value(&env, "MAX_QUERIES_PER_WINDOW")
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|value| *value > 0);
        let max_counts_per_window = env_value(&env, "MAX_COUNTS_PER_WINDOW")
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|value| *value > 0);
        let max_publishes_per_window = env_value(&env, "MAX_PUBLISHES_PER_WINDOW")
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|value| *value > 0);
        let enable_file_metadata = env_value(&env, "ENABLE_FILE_METADATA").unwrap_or("1") == "1";
        let enable_file_api = env_value(&env, "ENABLE_FILE_API").unwrap_or("1") == "1";
        let enable_blossom = env_value(&env, "ENABLE_BLOSSOM").unwrap_or("1") == "1";
        let enable_blossom_list = env_value(&env, "ENABLE_BLOSSOM_LIST").unwrap_or("1") == "1";
        let enable_blossom_mirror = env_value(&env, "ENABLE_BLOSSOM_MIRROR").unwrap_or("0") == "1";
        let require_nip98_auth = env_value(&env, "REQUIRE_NIP98_AUTH").unwrap_or("0") == "1";
        let require_blossom_auth = env_value(&env, "REQUIRE_BLOSSOM_AUTH").unwrap_or("0") == "1";
        let require_blossom_get_auth =
            env_value(&env, "REQUIRE_BLOSSOM_GET_AUTH").unwrap_or("0") == "1";
        let file_api_url = env_value(&env, "FILE_API_URL")
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let blossom_public_url = env_value(&env, "BLOSSOM_PUBLIC_URL")
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let file_max_bytes = env_value(&env, "FILE_MAX_BYTES")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(32 * 1024 * 1024);
        let file_allowed_mime = env_value(&env, "FILE_ALLOW_MIME")
            .and_then(csv_strings_opt)
            .map(|values| {
                values
                    .into_iter()
                    .map(|value| value.to_ascii_lowercase())
                    .collect()
            });
        let file_blocked_mime = env_value(&env, "FILE_DENY_MIME")
            .and_then(csv_strings_opt)
            .map(|values| {
                values
                    .into_iter()
                    .map(|value| value.to_ascii_lowercase())
                    .collect()
            });
        let file_hash_denylist = load_hash_denylist(&env);
        let file_keep_mode = parse_file_keep_mode(env_value(&env, "FILE_KEEP_MODE"));
        let max_blob_bytes_per_pubkey = env_value(&env, "MAX_BLOB_BYTES_PER_PUBKEY")
            .and_then(|value| value.parse::<u64>().ok())
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
            allowed_kinds,
            blocked_kinds,
            allowed_pubkeys,
            blocked_pubkeys,
            owner_pubkeys,
            follow_pubkeys,
            pinned_event_ids,
            protect_pinned_from_deletes,
            enable_nip42,
            require_auth_for_query,
            require_auth_for_count,
            require_auth_for_publish,
            auth_must_match_event_pubkey,
            auth_max_age_secs,
            support_nip11,
            support_nip09,
            support_nip12,
            support_nip42,
            support_nip40,
            support_nip45,
            support_nip50,
            support_nip94,
            support_nip96,
            support_nip98,
            support_nip_b7,
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
            max_limit,
            max_event_bytes,
            max_event_age_secs,
            max_event_future_secs,
            rate_limit_window_secs,
            max_queries_per_window,
            max_counts_per_window,
            max_publishes_per_window,
            enable_file_metadata,
            enable_file_api,
            enable_blossom,
            enable_blossom_list,
            enable_blossom_mirror,
            require_nip98_auth,
            require_blossom_auth,
            require_blossom_get_auth,
            file_api_url,
            blossom_public_url,
            file_max_bytes,
            file_allowed_mime,
            file_blocked_mime,
            file_hash_denylist,
            file_keep_mode,
            max_blob_bytes_per_pubkey,
        })
    }

    pub fn relay_info_enabled(&self) -> bool {
        self.enable_nip11 && self.support_nip11
    }

    pub fn nip42_enabled(&self) -> bool {
        self.enable_nip42 && self.support_nip42
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

    pub fn file_metadata_enabled(&self) -> bool {
        self.enable_file_metadata && self.support_nip94
    }

    pub fn file_api_enabled(&self) -> bool {
        self.enable_file_api && self.support_nip96
    }

    pub fn blossom_enabled(&self) -> bool {
        self.enable_blossom && self.support_nip_b7
    }

    pub fn blossom_list_enabled(&self) -> bool {
        self.blossom_enabled() && self.enable_blossom_list
    }

    pub fn blossom_mirror_enabled(&self) -> bool {
        self.blossom_enabled() && self.enable_blossom_mirror
    }

    pub fn nip98_auth_required(&self) -> bool {
        self.file_api_enabled() && self.support_nip98 && self.require_nip98_auth
    }

    pub fn blossom_write_auth_required(&self) -> bool {
        self.blossom_enabled() && self.require_blossom_auth
    }

    pub fn blossom_get_auth_required(&self) -> bool {
        self.blossom_enabled() && self.require_blossom_get_auth
    }

    pub fn file_hash_allowed(&self, hash: &str) -> bool {
        !self
            .file_hash_denylist
            .as_ref()
            .is_some_and(|values| values.iter().any(|value| value.eq_ignore_ascii_case(hash)))
    }

    pub fn file_mime_allowed(&self, mime: &str) -> bool {
        let mime = mime.trim().to_ascii_lowercase();
        if self.file_blocked_mime.as_ref().is_some_and(|patterns| {
            patterns
                .iter()
                .any(|pattern| mime_pattern_matches(pattern, &mime))
        }) {
            return false;
        }
        self.file_allowed_mime
            .as_ref()
            .map(|patterns| {
                patterns
                    .iter()
                    .any(|pattern| mime_pattern_matches(pattern, &mime))
            })
            .unwrap_or(true)
    }

    pub fn query_auth_required(&self) -> bool {
        self.nip42_enabled() && self.require_auth_for_query
    }

    pub fn count_auth_required(&self) -> bool {
        self.nip42_enabled() && self.require_auth_for_count
    }

    pub fn publish_auth_required(&self) -> bool {
        self.nip42_enabled() && (self.require_auth_for_publish || self.auth_must_match_event_pubkey)
    }

    pub fn is_owner_pubkey(&self, pubkey: &str) -> bool {
        self.owner_pubkeys
            .as_ref()
            .is_some_and(|owners| owners.iter().any(|value| value == pubkey))
    }

    pub fn mirror_authors(&self) -> Option<Vec<String>> {
        let mut merged = Vec::new();
        if let Some(values) = &self.filter_authors {
            merged.extend(values.iter().cloned());
        }
        if let Some(values) = &self.follow_pubkeys {
            merged.extend(values.iter().cloned());
        }
        if merged.is_empty() {
            return None;
        }
        merged.sort();
        merged.dedup();
        Some(merged)
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

fn csv_strings_opt(input: &str) -> Option<Vec<String>> {
    let values = csv_strings(input);
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn csv_u32_opt(input: &str) -> Option<Vec<u32>> {
    let values = csv_u32(input);
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn load_hash_denylist(env: &HashMap<String, String>) -> Option<Vec<String>> {
    let mut values = csv_strings(env_value(env, "FILE_HASH_DENYLIST").unwrap_or_default())
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if let Some(path) = env_value(env, "FILE_HASH_DENYLIST_PATH") {
        if !path.trim().is_empty() {
            match fs::read_to_string(path) {
                Ok(data) => {
                    for line in data.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        values.push(line.to_ascii_lowercase());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {}
            }
        }
    }
    values.sort();
    values.dedup();
    (!values.is_empty()).then_some(values)
}

fn parse_file_keep_mode(value: Option<&str>) -> FileKeepMode {
    match value
        .unwrap_or("referenced")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "all" => FileKeepMode::All,
        _ => FileKeepMode::Referenced,
    }
}

fn mime_pattern_matches(pattern: &str, mime: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return mime.starts_with(&format!("{prefix}/"));
    }
    pattern == mime
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
                "ALLOW_KINDS=1,30023\n",
                "DENY_KINDS=4,1059\n",
                "ALLOW_PUBKEYS=npub1,npub2\n",
                "DENY_PUBKEYS=npub3\n",
                "RELAYS_UPSTREAM=ws://r1,ws://r2\n",
                "TOR_SOCKS=\n",
                "FILTER_AUTHORS=npub1\n",
                "FILTER_KINDS=1,30023\n",
                "FILTER_TAG_T=essay\n",
                "FILTER_TAG_A=30023:pubkey:slug\n",
                "FILTER_SINCE_MODE=fixed:1700000000\n",
                "MAX_LIMIT=1000\n",
                "MAX_EVENT_BYTES=262144\n",
                "MAX_EVENT_AGE_SECS=31536000\n",
                "MAX_EVENT_FUTURE_SECS=900\n",
                "RATE_LIMIT_WINDOW_SECS=60\n",
                "MAX_QUERIES_PER_WINDOW=120\n",
                "MAX_COUNTS_PER_WINDOW=120\n",
                "MAX_PUBLISHES_PER_WINDOW=60\n",
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
        assert_eq!(cfg.allowed_kinds.as_ref().unwrap(), &vec![1, 30023]);
        assert_eq!(cfg.blocked_kinds.as_ref().unwrap(), &vec![4, 1059]);
        assert_eq!(
            cfg.allowed_pubkeys.as_ref().unwrap(),
            &vec![String::from("npub1"), String::from("npub2")]
        );
        assert_eq!(
            cfg.blocked_pubkeys.as_ref().unwrap(),
            &vec![String::from("npub3")]
        );
        assert!(!cfg.enable_nip42);
        assert!(!cfg.require_auth_for_query);
        assert!(!cfg.require_auth_for_count);
        assert!(!cfg.require_auth_for_publish);
        assert!(!cfg.auth_must_match_event_pubkey);
        assert_eq!(cfg.auth_max_age_secs, 600);
        assert!(cfg.support_nip11);
        assert!(cfg.support_nip09);
        assert!(cfg.support_nip12);
        assert!(cfg.support_nip42);
        assert!(cfg.support_nip40);
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
        assert_eq!(cfg.max_limit, Some(1000));
        assert_eq!(cfg.max_event_bytes, Some(262144));
        assert_eq!(cfg.max_event_age_secs, Some(31_536_000));
        assert_eq!(cfg.max_event_future_secs, Some(900));
        assert_eq!(cfg.rate_limit_window_secs, Some(60));
        assert_eq!(cfg.max_queries_per_window, Some(120));
        assert_eq!(cfg.max_counts_per_window, Some(120));
        assert_eq!(cfg.max_publishes_per_window, Some(60));
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
        assert!(cfg.allowed_kinds.is_none());
        assert!(cfg.blocked_kinds.is_none());
        assert!(cfg.allowed_pubkeys.is_none());
        assert!(cfg.blocked_pubkeys.is_none());
        assert!(!cfg.enable_nip42);
        assert!(!cfg.require_auth_for_query);
        assert!(!cfg.require_auth_for_count);
        assert!(!cfg.require_auth_for_publish);
        assert!(!cfg.auth_must_match_event_pubkey);
        assert_eq!(cfg.auth_max_age_secs, 600);
        assert!(cfg.support_nip11);
        assert!(cfg.support_nip09);
        assert!(cfg.support_nip12);
        assert!(cfg.support_nip42);
        assert!(cfg.support_nip40);
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
        assert!(cfg.max_limit.is_none());
        assert!(cfg.max_event_bytes.is_none());
        assert!(cfg.max_event_age_secs.is_none());
        assert!(cfg.max_event_future_secs.is_none());
        assert!(cfg.rate_limit_window_secs.is_none());
        assert!(cfg.max_queries_per_window.is_none());
        assert!(cfg.max_counts_per_window.is_none());
        assert!(cfg.max_publishes_per_window.is_none());
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
    fn owner_follow_and_pin_settings_parse_and_merge_mirror_authors() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let env_path = dir.path().join(".env");
        fs::write(
            &env_path,
            concat!(
                "STORE_ROOT=/tmp\n",
                "BIND_HTTP=127.0.0.1:8080\n",
                "BIND_WS=127.0.0.1:8081\n",
                "FILTER_AUTHORS=alice\n",
                "OWNER_PUBKEYS=owner1,owner2\n",
                "FOLLOW_PUBKEYS=alice,bob\n",
                "PIN_EVENT_IDS=aa11,bb22\n",
                "PIN_PROTECT_FROM_DELETES=1\n"
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert_eq!(
            cfg.owner_pubkeys.as_ref().unwrap(),
            &vec![String::from("owner1"), String::from("owner2")]
        );
        assert_eq!(
            cfg.follow_pubkeys.as_ref().unwrap(),
            &vec![String::from("alice"), String::from("bob")]
        );
        assert_eq!(
            cfg.pinned_event_ids.as_ref().unwrap(),
            &vec![String::from("aa11"), String::from("bb22")]
        );
        assert!(cfg.protect_pinned_from_deletes);
        assert!(cfg.is_owner_pubkey("owner1"));
        assert_eq!(
            cfg.mirror_authors().unwrap(),
            vec!["alice".to_string(), "bob".to_string()]
        );
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
                "ENABLE_NIP42=1\n",
                "REQUIRE_AUTH_FOR_QUERY=1\n",
                "REQUIRE_AUTH_FOR_COUNT=1\n",
                "REQUIRE_AUTH_FOR_PUBLISH=1\n",
                "AUTH_MUST_MATCH_EVENT_PUBKEY=1\n",
                "AUTH_MAX_AGE_SECS=42\n",
                "SUPPORT_NIP11=0\n",
                "SUPPORT_NIP09=0\n",
                "SUPPORT_NIP12=0\n",
                "SUPPORT_NIP42=0\n",
                "SUPPORT_NIP40=0\n",
                "SUPPORT_NIP45=0\n",
                "SUPPORT_NIP50=0\n",
            ),
        )
        .unwrap();
        let cfg = Settings::from_env(env_path.to_str().unwrap()).unwrap();
        assert_eq!(cfg.relay_name, "My relay");
        assert!(!cfg.relay_info_enabled());
        assert!(!cfg.delete_enabled());
        assert!(!cfg.expiration_enabled());
        assert!(!cfg.query_enabled());
        assert!(!cfg.publish_enabled());
        assert!(!cfg.live_subscriptions_enabled());
        assert!(!cfg.count_enabled());
        assert!(!cfg.tag_queries_enabled());
        assert!(!cfg.search_enabled());
        assert!(!cfg.enable_mirroring);
        assert!(!cfg.nip42_enabled());
        assert!(!cfg.query_auth_required());
        assert!(!cfg.count_auth_required());
        assert!(!cfg.publish_auth_required());
        assert_eq!(cfg.auth_max_age_secs, 42);
    }
}
