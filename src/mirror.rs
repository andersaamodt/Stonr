//! Upstream relay mirroring for importing events into the local store.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::broadcast,
};
use tokio_socks::tcp::Socks5Stream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{client_async, connect_async, tungstenite::Message, WebSocketStream};
use url::Url;

use crate::{
    config::{MirrorMode, Settings, SinceMode},
    event::{Event, Tag},
    policy::{current_unix_ts, validate_event_with_files},
    storage::{Query, Store},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MirrorCursorValue {
    pub relay: String,
    pub scope: String,
    pub cursor_key: String,
    pub since: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MirrorStatus {
    pub cursor_key: String,
    pub relay: String,
    pub scope: String,
    pub state: String,
    pub last_connect_at: Option<u64>,
    pub last_event_at: Option<u64>,
    pub last_seen_event_created_at: Option<u64>,
    pub last_eose_at: Option<u64>,
    pub last_success_at: Option<u64>,
    pub last_error_at: Option<u64>,
    pub last_error: Option<String>,
}

pub(crate) fn read_statuses(root: &Path) -> Result<Vec<MirrorStatus>> {
    let mut statuses = Vec::new();
    let status_dir = mirror_status_dir(root);
    if !status_dir.exists() {
        return Ok(statuses);
    }
    for entry in fs::read_dir(status_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let data = match fs::read_to_string(entry.path()) {
            Ok(data) => data,
            Err(error) => {
                crate::log::warn(
                    "mirror",
                    "skipping unreadable status file",
                    serde_json::json!({
                        "path": entry.path().display().to_string(),
                        "error": error.to_string(),
                    }),
                );
                continue;
            }
        };
        match serde_json::from_str::<MirrorStatus>(&data) {
            Ok(status) => statuses.push(status),
            Err(error) => {
                crate::log::warn(
                    "mirror",
                    "skipping malformed status file",
                    serde_json::json!({
                        "path": entry.path().display().to_string(),
                        "error": error.to_string(),
                    }),
                );
            }
        }
    }
    statuses.sort_by(|left, right| {
        left.scope
            .cmp(&right.scope)
            .then_with(|| left.relay.cmp(&right.relay))
    });
    Ok(statuses)
}

/// Spawn a mirroring task for each configured upstream relay.
pub async fn run(cfg: Settings, store: Store, events_tx: broadcast::Sender<Event>) {
    for relay in cfg.relays_upstream.clone() {
        let cfg_clone = cfg.clone();
        let store_clone = store.clone();
        let relay_events_tx = events_tx.clone();
        match cfg.mirror_mode {
            MirrorMode::Broad => {
                tokio::spawn(async move {
                    mirror_relay_forever(relay, cfg_clone, store_clone, relay_events_tx).await;
                });
            }
            MirrorMode::Site => {
                let posts_relay = relay.clone();
                let posts_cfg = cfg_clone.clone();
                let posts_store = store_clone.clone();
                let posts_events_tx = relay_events_tx.clone();
                tokio::spawn(async move {
                    mirror_site_posts_forever(posts_relay, posts_cfg, posts_store, posts_events_tx)
                        .await;
                });
                if cfg.mirror_site_include_comments {
                    let comments_relay = relay.clone();
                    let comments_cfg = cfg_clone.clone();
                    let comments_store = store_clone.clone();
                    let comments_events_tx = relay_events_tx.clone();
                    tokio::spawn(async move {
                        mirror_site_comments_forever(
                            comments_relay,
                            comments_cfg,
                            comments_store,
                            comments_events_tx,
                        )
                        .await;
                    });
                }
            }
        }
    }
}

/// Connect to a relay, subscribe, and persist received events.
///
/// The mirroring workflow is:
/// 1. Determine the starting timestamp (`since`) from a stored cursor or fixed
///    configuration.
/// 2. Build a Nostr filter and open a WebSocket connection to the upstream
///    relay (optionally via Tor).
/// 3. Send a `REQ` subscription and process incoming `EVENT` messages,
///    updating the latest timestamp seen.
/// 4. After receiving `EOSE`, write the cursor so the next run resumes from the
///    newest event.
async fn mirror_relay_forever(
    relay: String,
    cfg: Settings,
    store: Store,
    events_tx: broadcast::Sender<Event>,
) {
    loop {
        if let Err(e) = mirror_relay_once(relay.clone(), cfg.clone(), store.clone(), events_tx.clone()).await {
            crate::log::error(
                "mirror",
                "broad mirror cycle failed",
                serde_json::json!({
                    "relay": relay.clone(),
                    "error": e.to_string(),
                }),
            );
            let _ = write_mirror_error(&cfg.store_root, &relay, &relay, "broad", &e.to_string());
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn mirror_relay_once(
    relay: String,
    cfg: Settings,
    store: Store,
    events_tx: broadcast::Sender<Event>,
) -> Result<()> {
    write_mirror_connecting(&cfg.store_root, &relay, &relay, "broad")?;
    // Determine the starting timestamp either from a stored cursor or a fixed
    // configuration value.
    let since = match cfg.filter_since_mode {
        SinceMode::Cursor => read_cursor(&cfg.store_root, &relay).unwrap_or(0),
        SinceMode::Fixed(ts) => ts,
    };
    // Assemble the filter sent in the REQ message based on config options.
    let mut filter = serde_json::Map::new();
    if let Some(a) = cfg.filter_authors.clone() {
        filter.insert(
            "authors".into(),
            Value::Array(a.into_iter().map(Value::String).collect()),
        );
    }
    if let Some(k) = cfg.filter_kinds.clone() {
        filter.insert(
            "kinds".into(),
            Value::Array(k.into_iter().map(|v| Value::Number(v.into())).collect()),
        );
    }
    if let Some(t) = cfg.filter_tag_t.clone() {
        filter.insert(
            "#t".into(),
            Value::Array(t.into_iter().map(Value::String).collect()),
        );
    }
    if let Some(a) = cfg.filter_tag_a.clone() {
        filter.insert(
            "#a".into(),
            Value::Array(a.into_iter().map(Value::String).collect()),
        );
    }
    if since > 0 {
        filter.insert("since".into(), Value::Number(since.into()));
    }
    let req = json!(["REQ", "mirror", Value::Object(filter)]);
    // Open the WebSocket (optionally through Tor) and send the subscription.
    let latest = if let Some(proxy) = cfg.tor_socks.as_deref() {
        let ws = connect_ws_via_proxy(&relay, proxy).await?;
        mirror_stream(
            ws,
            vec![req],
            &store,
            &events_tx,
            MirrorStreamOptions {
                since,
                store_root: &cfg.store_root,
                cursor_key: &relay,
                relay: &relay,
                scope: "broad",
                settings: &cfg,
                delete_enabled: cfg.delete_enabled(),
                expiration_enabled: cfg.expiration_enabled(),
                keep_running_after_eose: true,
            },
        )
        .await?
    } else {
        let (ws, _) = connect_async(&relay).await?;
        mirror_stream(
            ws,
            vec![req],
            &store,
            &events_tx,
            MirrorStreamOptions {
                since,
                store_root: &cfg.store_root,
                cursor_key: &relay,
                relay: &relay,
                scope: "broad",
                settings: &cfg,
                delete_enabled: cfg.delete_enabled(),
                expiration_enabled: cfg.expiration_enabled(),
                keep_running_after_eose: true,
            },
        )
        .await?
    };
    // Persist the cursor so the next run resumes from where we left off.
    write_cursor(&cfg.store_root, &relay, latest)?;
    Ok(())
}

#[cfg(test)]
async fn mirror_relay(relay: String, cfg: Settings, store: Store) -> Result<()> {
    let (events_tx, _) = broadcast::channel(256);
    mirror_relay_once(relay, cfg, store, events_tx).await
}

async fn mirror_site_posts_forever(
    relay: String,
    cfg: Settings,
    store: Store,
    events_tx: broadcast::Sender<Event>,
) {
    loop {
        if let Err(e) = mirror_site_posts_once(
            relay.clone(),
            cfg.clone(),
            store.clone(),
            events_tx.clone(),
        )
        .await
        {
            crate::log::error(
                "mirror",
                "site post mirror cycle failed",
                serde_json::json!({
                    "relay": relay.clone(),
                    "error": e.to_string(),
                }),
            );
            let cursor_key = cursor_key_for(&relay, "site-posts");
            let _ = write_mirror_error(
                &cfg.store_root,
                &cursor_key,
                &relay,
                "site-posts",
                &e.to_string(),
            );
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn mirror_site_posts_once(
    relay: String,
    cfg: Settings,
    store: Store,
    events_tx: broadcast::Sender<Event>,
) -> Result<()> {
    let author = match cfg.mirror_site_author.clone() {
        Some(author) if !author.is_empty() => author,
        _ => return Ok(()),
    };
    let cursor_key = cursor_key_for(&relay, "site-posts");
    write_mirror_connecting(&cfg.store_root, &cursor_key, &relay, "site-posts")?;
    let since = match cfg.filter_since_mode {
        SinceMode::Cursor => read_cursor(&cfg.store_root, &cursor_key).unwrap_or(0),
        SinceMode::Fixed(ts) => ts,
    };
    let mut filter = serde_json::Map::new();
    filter.insert("authors".into(), Value::Array(vec![Value::String(author)]));
    filter.insert("kinds".into(), Value::Array(vec![Value::Number(30023u32.into())]));
    if since > 0 {
        filter.insert("since".into(), Value::Number(since.into()));
    }
    let req = json!(["REQ", "site-posts", Value::Object(filter)]);
    let latest = if let Some(proxy) = cfg.tor_socks.as_deref() {
        let ws = connect_ws_via_proxy(&relay, proxy).await?;
        mirror_stream(
            ws,
            vec![req],
            &store,
            &events_tx,
            MirrorStreamOptions {
                since,
                store_root: &cfg.store_root,
                cursor_key: &cursor_key,
                relay: &relay,
                scope: "site-posts",
                settings: &cfg,
                delete_enabled: cfg.delete_enabled(),
                expiration_enabled: cfg.expiration_enabled(),
                keep_running_after_eose: true,
            },
        )
        .await?
    } else {
        let (ws, _) = connect_async(&relay).await?;
        mirror_stream(
            ws,
            vec![req],
            &store,
            &events_tx,
            MirrorStreamOptions {
                since,
                store_root: &cfg.store_root,
                cursor_key: &cursor_key,
                relay: &relay,
                scope: "site-posts",
                settings: &cfg,
                delete_enabled: cfg.delete_enabled(),
                expiration_enabled: cfg.expiration_enabled(),
                keep_running_after_eose: true,
            },
        )
        .await?
    };
    write_cursor(&cfg.store_root, &cursor_key, latest)?;
    Ok(())
}

async fn mirror_site_comments_forever(
    relay: String,
    cfg: Settings,
    store: Store,
    events_tx: broadcast::Sender<Event>,
) {
    loop {
        if let Err(e) = mirror_site_comments_once(
            relay.clone(),
            cfg.clone(),
            store.clone(),
            events_tx.clone(),
        )
        .await
        {
            crate::log::error(
                "mirror",
                "site comment mirror cycle failed",
                serde_json::json!({
                    "relay": relay.clone(),
                    "error": e.to_string(),
                }),
            );
            let cursor_key = cursor_key_for(&relay, "site-comments");
            let _ = write_mirror_error(
                &cfg.store_root,
                &cursor_key,
                &relay,
                "site-comments",
                &e.to_string(),
            );
        }
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

async fn mirror_site_comments_once(
    relay: String,
    cfg: Settings,
    store: Store,
    events_tx: broadcast::Sender<Event>,
) -> Result<()> {
    let author = match cfg.mirror_site_author.clone() {
        Some(author) if !author.is_empty() => author,
        _ => return Ok(()),
    };
    let cursor_key = cursor_key_for(&relay, "site-comments");
    write_mirror_connecting(&cfg.store_root, &cursor_key, &relay, "site-comments")?;
    let post_events = store.query_with_policy(
        Query {
        authors: Some(vec![author.clone()]),
        kinds: Some(vec![30023]),
        d: None,
        t: None,
        tags: vec![],
        search: None,
        since: None,
        until: None,
        limit: Some(5000),
        },
        cfg.delete_enabled(),
        cfg.expiration_enabled(),
    )?;
    let mut addresses = vec![];
    for ev in post_events {
        if let Some(d_tag) = ev.tags.iter().find_map(|Tag(fields)| match fields.as_slice() {
            [tag, value, ..] if tag == "d" => Some(value.clone()),
            _ => None,
        }) {
            addresses.push(format!("30023:{}:{}", ev.pubkey, d_tag));
        }
    }
    addresses.sort();
    addresses.dedup();
    if addresses.is_empty() {
        return Ok(());
    }
    let since = match cfg.filter_since_mode {
        SinceMode::Cursor => read_cursor(&cfg.store_root, &cursor_key).unwrap_or(0),
        SinceMode::Fixed(ts) => ts,
    };
    let mut filter = serde_json::Map::new();
    filter.insert("kinds".into(), Value::Array(vec![Value::Number(1u32.into())]));
    filter.insert(
        "#a".into(),
        Value::Array(addresses.into_iter().map(Value::String).collect()),
    );
    if since > 0 {
        filter.insert("since".into(), Value::Number(since.into()));
    }
    let req = json!(["REQ", "site-comments", Value::Object(filter)]);
    let latest = if let Some(proxy) = cfg.tor_socks.as_deref() {
        let ws = connect_ws_via_proxy(&relay, proxy).await?;
        mirror_stream(
            ws,
            vec![req],
            &store,
            &events_tx,
            MirrorStreamOptions {
                since,
                store_root: &cfg.store_root,
                cursor_key: &cursor_key,
                relay: &relay,
                scope: "site-comments",
                settings: &cfg,
                delete_enabled: cfg.delete_enabled(),
                expiration_enabled: cfg.expiration_enabled(),
                keep_running_after_eose: false,
            },
        )
        .await?
    } else {
        let (ws, _) = connect_async(&relay).await?;
        mirror_stream(
            ws,
            vec![req],
            &store,
            &events_tx,
            MirrorStreamOptions {
                since,
                store_root: &cfg.store_root,
                cursor_key: &cursor_key,
                relay: &relay,
                scope: "site-comments",
                settings: &cfg,
                delete_enabled: cfg.delete_enabled(),
                expiration_enabled: cfg.expiration_enabled(),
                keep_running_after_eose: false,
            },
        )
        .await?
    };
    write_cursor(&cfg.store_root, &cursor_key, latest)?;
    Ok(())
}

struct MirrorStreamOptions<'a> {
    since: u64,
    store_root: &'a Path,
    cursor_key: &'a str,
    relay: &'a str,
    scope: &'a str,
    settings: &'a Settings,
    delete_enabled: bool,
    expiration_enabled: bool,
    keep_running_after_eose: bool,
}

async fn mirror_stream<S>(
    mut ws: WebSocketStream<S>,
    reqs: Vec<Value>,
    store: &Store,
    events_tx: &broadcast::Sender<Event>,
    options: MirrorStreamOptions<'_>,
) -> Result<u64>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    for req in reqs {
        ws.send(Message::Text(req.to_string())).await?;
    }
    let mut latest = options.since;
    while let Some(msg) = ws.next().await {
        match msg {
            Err(_) => break,
            Ok(msg) => match msg {
            Message::Text(txt) => {
                if let Ok(val) = serde_json::from_str::<Value>(&txt) {
                    if let Some(arr) = val.as_array() {
                        match arr.first().and_then(|v| v.as_str()) {
                            Some("EVENT") if arr.len() >= 3 => {
                                if let Ok(ev) = serde_json::from_value::<Event>(arr[2].clone()) {
                                    latest = latest.max(ev.created_at);
                                    if validate_event_with_files(
                                        options.settings,
                                        &store.files(),
                                        &ev,
                                        current_unix_ts(),
                                    )
                                    .is_err()
                                    {
                                        let _ = write_mirror_success(
                                            options.store_root,
                                            options.cursor_key,
                                            options.relay,
                                            options.scope,
                                            Some(ev.created_at),
                                            false,
                                        );
                                        let _ =
                                            write_cursor(options.store_root, options.cursor_key, latest);
                                        continue;
                                    }
                                    if let Err(e) = store.ingest_with_policy(
                                        &ev,
                                        options.delete_enabled,
                                        options.expiration_enabled,
                                    ) {
                                        crate::log::warn(
                                            "mirror",
                                            "failed to ingest mirrored event",
                                            serde_json::json!({
                                                "relay": options.relay,
                                                "scope": options.scope,
                                                "event_id": ev.id,
                                                "error": e.to_string(),
                                            }),
                                        );
                                    } else {
                                        let _ = store.files().add_event_references(&ev);
                                    }
                                    if store
                                        .event_visible_with_policy(
                                            &ev,
                                            options.delete_enabled,
                                            options.expiration_enabled,
                                        )
                                        .unwrap_or(false)
                                    {
                                        let _ = events_tx.send(ev.clone());
                                    }
                                    let _ = write_mirror_success(
                                        options.store_root,
                                        options.cursor_key,
                                        options.relay,
                                        options.scope,
                                        Some(ev.created_at),
                                        false,
                                    );
                                    let _ =
                                        write_cursor(options.store_root, options.cursor_key, latest);
                                }
                            }
                            Some("EOSE") => {
                                let _ = write_mirror_success(
                                    options.store_root,
                                    options.cursor_key,
                                    options.relay,
                                    options.scope,
                                    None,
                                    !options.keep_running_after_eose,
                                );
                                if !options.keep_running_after_eose {
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }}
    }
    Ok(latest)
}

/// Establish a WebSocket connection via a SOCKS5 proxy.
///
/// The underlying stream is boxed as a trait object because `Socks5Stream`
/// has a different concrete type from the direct `TcpStream`/TLS path used by
/// `connect_async`. Any network or handshake errors bubble up to the caller.
async fn connect_ws_via_proxy(
    relay: &str,
    tor_socks: &str,
) -> Result<WebSocketStream<Box<dyn AsyncReadWrite + Unpin + Send>>> {
    let url = Url::parse(relay)?;
    let host = url.host_str().ok_or_else(|| anyhow!("missing host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow!("missing port"))?;
    let req = relay.into_client_request()?;
    let stream: Box<dyn AsyncReadWrite + Unpin + Send> =
        Box::new(Socks5Stream::connect(tor_socks, (host, port)).await?);
    let (ws, _) = client_async(req, stream).await?;
    Ok(ws)
}

/// Blanket trait for boxed async read/write streams.
///
/// `TcpStream` and `Socks5Stream` implement the standard `AsyncRead` and
/// `AsyncWrite` traits but have different concrete types. Boxing them behind a
/// trait object lets `connect_ws` return a single stream type regardless of how
/// the connection was established.
trait AsyncReadWrite: AsyncRead + AsyncWrite {}
impl<T: AsyncRead + AsyncWrite> AsyncReadWrite for T {}

/// Compute the cursor file path for a relay URL.
///
/// Each upstream relay gets a SHA1-hashed filename under `cursor/` so that
/// timestamps persist across runs without leaking the relay URL itself.
fn cursor_path(root: &Path, relay: &str) -> PathBuf {
    let mut hasher = Sha1::new();
    hasher.update(relay.as_bytes());
    let hash = hex::encode(hasher.finalize());
    root.join("cursor").join(format!("{}.since", hash))
}

pub fn cursor_key_for(relay: &str, scope: &str) -> String {
    if scope.is_empty() || scope == "broad" {
        relay.to_string()
    } else {
        format!("{relay}::{scope}")
    }
}

/// Read the last seen timestamp for a relay.
///
/// Returns `None` if no cursor file exists or if the contents fail to parse.
fn read_cursor(root: &Path, relay: &str) -> Option<u64> {
    let path = cursor_path(root, relay);
    std::fs::read_to_string(path).ok()?.parse().ok()
}

/// Persist the last seen timestamp for a relay.
///
/// Any I/O error while creating directories or writing the file is returned
/// to the caller.
fn write_cursor(root: &Path, relay: &str, ts: u64) -> Result<()> {
    let path = cursor_path(root, relay);
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, ts.to_string())?;
    Ok(())
}

pub fn get_cursor(root: &Path, relay: &str, scope: &str) -> MirrorCursorValue {
    let cursor_key = cursor_key_for(relay, scope);
    MirrorCursorValue {
        relay: relay.to_string(),
        scope: normalized_scope(scope).to_string(),
        cursor_key: cursor_key.clone(),
        since: read_cursor(root, &cursor_key),
    }
}

pub fn set_cursor(root: &Path, relay: &str, scope: &str, since: u64) -> Result<MirrorCursorValue> {
    let cursor_key = cursor_key_for(relay, scope);
    write_cursor(root, &cursor_key, since)?;
    Ok(MirrorCursorValue {
        relay: relay.to_string(),
        scope: normalized_scope(scope).to_string(),
        cursor_key,
        since: Some(since),
    })
}

pub fn clear_cursor(root: &Path, relay: &str, scope: &str) -> Result<MirrorCursorValue> {
    let cursor_key = cursor_key_for(relay, scope);
    let path = cursor_path(root, &cursor_key);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(MirrorCursorValue {
        relay: relay.to_string(),
        scope: normalized_scope(scope).to_string(),
        cursor_key,
        since: None,
    })
}

fn mirror_status_dir(root: &Path) -> PathBuf {
    root.join("runtime/mirror")
}

fn mirror_status_path(root: &Path, cursor_key: &str) -> PathBuf {
    let mut hasher = Sha1::new();
    hasher.update(cursor_key.as_bytes());
    let hash = hex::encode(hasher.finalize());
    mirror_status_dir(root).join(format!("{hash}.json"))
}

fn read_status(root: &Path, cursor_key: &str) -> Result<Option<MirrorStatus>> {
    let path = mirror_status_path(root, cursor_key);
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&data)?))
}

fn normalized_scope(scope: &str) -> &str {
    if scope.is_empty() {
        "broad"
    } else {
        scope
    }
}

pub(crate) fn write_status(root: &Path, status: &MirrorStatus) -> Result<()> {
    let path = mirror_status_path(root, &status.cursor_key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec(status)?)?;
    Ok(())
}

fn update_mirror_status<F>(
    root: &Path,
    cursor_key: &str,
    relay: &str,
    scope: &str,
    mutator: F,
) -> Result<()>
where
    F: FnOnce(&mut MirrorStatus),
{
    let mut status = read_status(root, cursor_key)?.unwrap_or(MirrorStatus {
        cursor_key: cursor_key.to_string(),
        relay: relay.to_string(),
        scope: scope.to_string(),
        state: "idle".into(),
        last_connect_at: None,
        last_event_at: None,
        last_seen_event_created_at: None,
        last_eose_at: None,
        last_success_at: None,
        last_error_at: None,
        last_error: None,
    });
    status.relay = relay.to_string();
    status.scope = scope.to_string();
    mutator(&mut status);
    write_status(root, &status)
}

fn write_mirror_connecting(root: &Path, cursor_key: &str, relay: &str, scope: &str) -> Result<()> {
    let now = current_unix_ts();
    update_mirror_status(root, cursor_key, relay, scope, |status| {
        status.state = "connecting".into();
        status.last_connect_at = Some(now);
        status.last_error = None;
    })
}

fn write_mirror_success(
    root: &Path,
    cursor_key: &str,
    relay: &str,
    scope: &str,
    event_created_at: Option<u64>,
    idle_after_success: bool,
) -> Result<()> {
    let now = current_unix_ts();
    update_mirror_status(root, cursor_key, relay, scope, |status| {
        status.state = if idle_after_success {
            "idle".into()
        } else {
            "running".into()
        };
        status.last_success_at = Some(now);
        if event_created_at.is_some() {
            status.last_event_at = Some(now);
            status.last_seen_event_created_at = event_created_at;
        }
        if idle_after_success {
            status.last_eose_at = Some(now);
        }
        status.last_error = None;
    })
}

fn write_mirror_error(
    root: &Path,
    cursor_key: &str,
    relay: &str,
    scope: &str,
    error: &str,
) -> Result<()> {
    let now = current_unix_ts();
    update_mirror_status(root, cursor_key, relay, scope, |status| {
        status.state = "error".into();
        status.last_error_at = Some(now);
        status.last_error = Some(error.to_string());
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{MirrorMode, Settings, SinceMode},
        event::{Event, Tag},
    };
    use tempfile::TempDir;
    use tokio_tungstenite::{accept_async, tungstenite::Message as TMsg};

    fn test_events_tx() -> broadcast::Sender<Event> {
        let (events_tx, _) = broadcast::channel(256);
        events_tx
    }

    fn base_settings(root: &std::path::Path) -> Settings {
        Settings {
            store_root: root.to_path_buf(),
            bind_http: String::new(),
            bind_ws: String::new(),
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
            filter_private_messages: false,
            relays_upstream: vec![],
            tor_socks: None,
            filter_authors: None,
            filter_kinds: None,
            filter_tag_t: None,
            filter_tag_a: None,
            filter_since_mode: SinceMode::Fixed(0),
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

    #[tokio::test]
    async fn mirror_ingests_and_updates_cursor() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        // prepare events
        let ev1 = Event {
            id: "aa11".into(),
            pubkey: "p".into(),
            kind: 1,
            created_at: 1,
            tags: vec![Tag(vec!["d".into(), "s".into()])],
            content: String::new(),
            sig: String::new(),
        };
        let ev2 = Event {
            id: "bb22".into(),
            pubkey: "p".into(),
            kind: 1,
            created_at: 2,
            tags: vec![Tag(vec!["d".into(), "s".into()])],
            content: String::new(),
            sig: String::new(),
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            // read req
            let _ = ws.next().await;
            ws.send(TMsg::Text(json!(["EVENT", "s", ev1]).to_string()))
                .await
                .unwrap();
            ws.send(TMsg::Text(json!(["EVENT", "s", ev2]).to_string()))
                .await
                .unwrap();
            ws.send(TMsg::Text(serde_json::json!(["EOSE", "s"]).to_string()))
                .await
                .unwrap();
        });

        let relay_url = format!("ws://{}", addr);
        let mut cfg = base_settings(dir.path());
        cfg.relays_upstream = vec![relay_url.clone()];
        mirror_relay(relay_url, cfg.clone(), store.clone())
            .await
            .unwrap();
        server.abort();

        assert!(dir.path().join("events/aa/11/aa11.json").exists());
        assert!(dir.path().join("events/bb/22/bb22.json").exists());
        let mut hasher = Sha1::new();
        hasher.update(cfg.relays_upstream[0].as_bytes());
        let hash = hex::encode(hasher.finalize());
        let cursor = dir.path().join(format!("cursor/{}.since", hash));
        let ts = std::fs::read_to_string(cursor).unwrap();
        assert_eq!(ts.trim(), "2");
    }

    #[tokio::test]
    async fn mirror_skips_private_message_kinds_when_filter_enabled() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ws_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_addr = ws_listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = ws_listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let _ = ws.next().await;
            let private = Event {
                id: "aa11".into(),
                pubkey: "p".into(),
                kind: 1059,
                created_at: 1,
                tags: vec![],
                content: "cipher".into(),
                sig: String::new(),
            };
            let public = Event {
                id: "bb22".into(),
                pubkey: "p".into(),
                kind: 1,
                created_at: 2,
                tags: vec![],
                content: "hello".into(),
                sig: String::new(),
            };
            ws.send(TMsg::Text(json!(["EVENT", "mirror", private]).to_string()))
                .await
                .unwrap();
            ws.send(TMsg::Text(json!(["EVENT", "mirror", public]).to_string()))
                .await
                .unwrap();
            ws.send(TMsg::Text(json!(["EOSE", "mirror"]).to_string()))
                .await
                .unwrap();
        });

        let mut cfg = base_settings(dir.path());
        cfg.bind_http = "127.0.0.1:0".into();
        cfg.bind_ws = "127.0.0.1:0".into();
        cfg.filter_private_messages = true;
        cfg.relays_upstream = vec![format!("ws://{}", ws_addr)];
        cfg.filter_since_mode = SinceMode::Cursor;
        super::run(cfg, store.clone(), test_events_tx()).await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        server.await.unwrap();

        assert!(!dir.path().join("events/aa/11/aa11.json").exists());
        assert!(dir.path().join("events/bb/22/bb22.json").exists());
    }

    #[tokio::test]
    async fn mirror_skips_blocked_authors() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ws_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_addr = ws_listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = ws_listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let _ = ws.next().await;
            let blocked = Event {
                id: "aa11".into(),
                pubkey: "blocked".into(),
                kind: 1,
                created_at: 1,
                tags: vec![],
                content: "hello".into(),
                sig: String::new(),
            };
            ws.send(TMsg::Text(json!(["EVENT", "mirror", blocked]).to_string()))
                .await
                .unwrap();
            ws.send(TMsg::Text(json!(["EOSE", "mirror"]).to_string()))
                .await
                .unwrap();
        });

        let mut cfg = base_settings(dir.path());
        cfg.blocked_pubkeys = Some(vec!["blocked".into()]);
        cfg.relays_upstream = vec![format!("ws://{}", ws_addr)];
        cfg.filter_since_mode = SinceMode::Cursor;
        super::run(cfg, store.clone(), test_events_tx()).await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        server.await.unwrap();

        assert!(!dir.path().join("events/aa/11/aa11.json").exists());
    }
    #[tokio::test]
    async fn mirror_resumes_from_cursor() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let relay_url = format!("ws://{}", addr);
        super::write_cursor(dir.path(), &relay_url, 5).unwrap();

        let ev = Event {
            id: "aa11".into(),
            pubkey: "p".into(),
            kind: 1,
            created_at: 6,
            tags: vec![Tag(vec!["d".into(), "s".into()])],
            content: String::new(),
            sig: String::new(),
        };
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            if let Some(Ok(TMsg::Text(txt))) = ws.next().await {
                assert!(txt.contains("\"since\":5"));
            }
            ws.send(TMsg::Text(json!(["EVENT", "s", ev]).to_string()))
                .await
                .unwrap();
            ws.send(TMsg::Text(json!(["EOSE", "s"]).to_string()))
                .await
                .unwrap();
        });

        let mut cfg = base_settings(dir.path());
        cfg.relays_upstream = vec![relay_url.clone()];
        cfg.filter_since_mode = SinceMode::Cursor;
        mirror_relay(relay_url.clone(), cfg, store.clone())
            .await
            .unwrap();
        server.abort();
        assert!(dir.path().join("events/aa/11/aa11.json").exists());
        assert_eq!(
            super::read_cursor(dir.path(), &relay_url),
            Some(6)
        );
    }

    async fn spawn_socks_proxy(target: std::net::SocketAddr) -> std::net::SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut inbound, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2];
            inbound.read_exact(&mut buf).await.unwrap();
            let nmethods = buf[1] as usize;
            let mut methods = vec![0u8; nmethods];
            inbound.read_exact(&mut methods).await.unwrap();
            inbound.write_all(&[0x05, 0x00]).await.unwrap();

            let mut req = [0u8; 4];
            inbound.read_exact(&mut req).await.unwrap();
            match req[3] {
                0x01 => {
                    let mut _addr = [0u8; 4];
                    inbound.read_exact(&mut _addr).await.unwrap();
                }
                0x03 => {
                    let mut len = [0u8; 1];
                    inbound.read_exact(&mut len).await.unwrap();
                    let mut name = vec![0u8; len[0] as usize];
                    inbound.read_exact(&mut name).await.unwrap();
                }
                0x04 => {
                    let mut _addr = [0u8; 16];
                    inbound.read_exact(&mut _addr).await.unwrap();
                }
                _ => {}
            }
            let mut _port = [0u8; 2];
            inbound.read_exact(&mut _port).await.unwrap();
            let mut outbound = tokio::net::TcpStream::connect(target).await.unwrap();
            inbound
                .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
            tokio::io::copy_bidirectional(&mut inbound, &mut outbound)
                .await
                .ok();
        });
        addr
    }

    #[tokio::test]
    async fn mirror_via_socks_proxy() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev = Event {
            id: "aa11".into(),
            pubkey: "p".into(),
            kind: 1,
            created_at: 1,
            tags: vec![Tag(vec!["d".into(), "s".into()])],
            content: String::new(),
            sig: String::new(),
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let _ = ws.next().await;
            ws.send(TMsg::Text(json!(["EVENT", "s", ev]).to_string()))
                .await
                .unwrap();
            ws.send(TMsg::Text(json!(["EOSE", "s"]).to_string()))
                .await
                .unwrap();
        });

        let proxy = spawn_socks_proxy(addr).await;
        let relay_url = format!("ws://{}", addr);
        let mut cfg = base_settings(dir.path());
        cfg.relays_upstream = vec![relay_url.clone()];
        cfg.tor_socks = Some(proxy.to_string());
        mirror_relay(relay_url, cfg, store.clone()).await.unwrap();
        server.abort();
        assert!(dir.path().join("events/aa/11/aa11.json").exists());
    }

    #[tokio::test]
    async fn mirror_sends_filters_in_req() {
        use serde_json::Value;
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            if let Some(Ok(TMsg::Text(txt))) = ws.next().await {
                let val: Value = serde_json::from_str(&txt).unwrap();
                let filt = &val[2];
                assert_eq!(filt["authors"][0], "a1");
                assert_eq!(filt["kinds"][0], 1);
                assert_eq!(filt["#t"][0], "tag1");
                assert_eq!(filt["since"], 5);
            }
            ws.send(TMsg::Text(json!(["EOSE", "s"]).to_string()))
                .await
                .unwrap();
        });
        let relay_url = format!("ws://{}", addr);
        let mut cfg = base_settings(dir.path());
        cfg.relays_upstream = vec![relay_url.clone()];
        cfg.filter_authors = Some(vec!["a1".into()]);
        cfg.filter_kinds = Some(vec![1]);
        cfg.filter_tag_t = Some(vec!["tag1".into()]);
        cfg.filter_since_mode = SinceMode::Fixed(5);
        mirror_relay(relay_url, cfg, store.clone()).await.unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn mirror_cursor_mode_without_file_starts_at_zero() {
        use serde_json::Value;
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            if let Some(Ok(TMsg::Text(txt))) = ws.next().await {
                let v: Value = serde_json::from_str(&txt).unwrap();
                assert!(v[2]["since"].is_null());
            }
            ws.send(TMsg::Text(
                json!(["EVENT", "s", {
                    "id": "aa11",
                    "pubkey": "p",
                    "kind": 1,
                    "created_at": 1,
                    "tags": [],
                    "content": "",
                    "sig": ""
                }])
                .to_string(),
            ))
            .await
            .unwrap();
            ws.send(TMsg::Text(json!(["EOSE", "s"]).to_string()))
                .await
                .unwrap();
        });
        let relay_url = format!("ws://{}", addr);
        let mut cfg = base_settings(dir.path());
        cfg.relays_upstream = vec![relay_url.clone()];
        cfg.filter_since_mode = SinceMode::Cursor;
        mirror_relay(relay_url.clone(), cfg, store.clone())
            .await
            .unwrap();
        server.abort();
        let mut hasher = Sha1::new();
        hasher.update(relay_url.as_bytes());
        let hash = hex::encode(hasher.finalize());
        let cursor_path = dir.path().join(format!("cursor/{}.since", hash));
        assert_eq!(std::fs::read_to_string(cursor_path).unwrap().trim(), "1");
    }

    #[tokio::test]
    async fn mirror_ignores_non_text_messages() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev = Event {
            id: "aa11".into(),
            pubkey: "p".into(),
            kind: 1,
            created_at: 1,
            tags: vec![Tag(vec!["d".into(), "s".into()])],
            content: String::new(),
            sig: String::new(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let _ = ws.next().await;
            ws.send(TMsg::Binary(vec![1, 2, 3])).await.unwrap();
            ws.send(TMsg::Text(json!(["EVENT", "s", ev]).to_string()))
                .await
                .unwrap();
            ws.send(TMsg::Text(json!(["EOSE", "s"]).to_string()))
                .await
                .unwrap();
        });
        let relay_url = format!("ws://{}", addr);
        let mut cfg = base_settings(dir.path());
        cfg.relays_upstream = vec![relay_url.clone()];
        mirror_relay(relay_url, cfg, store.clone()).await.unwrap();
        server.abort();
        assert!(dir.path().join("events/aa/11/aa11.json").exists());
    }

    #[tokio::test]
    async fn site_post_mirror_matches_nostr_blog_author_scope() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let message = ws.next().await.unwrap().unwrap();
            let TMsg::Text(text) = message else {
                panic!("expected text request");
            };
            tx.send(text.to_string()).unwrap();
            ws.send(TMsg::Text(json!(["EOSE", "site-posts"]).to_string()))
                .await
                .unwrap();
        });

        let mut cfg = base_settings(dir.path());
        cfg.mirror_site_author = Some("site-author".into());
        mirror_site_posts_once(
            format!("ws://{}", addr),
            cfg,
            store,
            test_events_tx(),
        )
        .await
        .unwrap();
        server.await.unwrap();

        let req: Value = serde_json::from_str(&rx.await.unwrap()).unwrap();
        assert_eq!(req[0], "REQ");
        assert_eq!(req[1], "site-posts");
        assert_eq!(req[2]["authors"][0], "site-author");
        assert_eq!(req[2]["kinds"][0], 30023);
    }

    #[tokio::test]
    async fn site_comment_mirror_matches_nostr_blog_address_scope() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        let post = Event {
            id: "post1".into(),
            pubkey: "site-author".into(),
            kind: 30023,
            created_at: 10,
            tags: vec![Tag(vec!["d".into(), "hello-world".into()])],
            content: "post".into(),
            sig: String::new(),
        };
        store.ingest(&post).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let message = ws.next().await.unwrap().unwrap();
            let TMsg::Text(text) = message else {
                panic!("expected text request");
            };
            tx.send(text.to_string()).unwrap();
            ws.send(TMsg::Text(json!(["EOSE", "site-comments"]).to_string()))
                .await
                .unwrap();
        });

        let mut cfg = base_settings(dir.path());
        cfg.mirror_site_author = Some("site-author".into());
        mirror_site_comments_once(
            format!("ws://{}", addr),
            cfg,
            store,
            test_events_tx(),
        )
        .await
        .unwrap();
        server.await.unwrap();

        let req: Value = serde_json::from_str(&rx.await.unwrap()).unwrap();
        assert_eq!(req[0], "REQ");
        assert_eq!(req[1], "site-comments");
        assert_eq!(req[2]["kinds"][0], 1);
        assert_eq!(req[2]["#a"][0], "30023:site-author:hello-world");
    }

    #[test]
    fn cursor_round_trip() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        write_cursor(&root, "ws://example", 42).unwrap();
        assert_eq!(read_cursor(&root, "ws://example"), Some(42));
    }

    #[tokio::test]
    async fn connect_ws_invalid_url_errors() {
        assert!(Url::parse("not a url").is_err());
        assert!(
            super::connect_ws_via_proxy("not a url", "127.0.0.1:9050")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn connect_ws_unreachable_host_errors() {
        assert!(connect_async("ws://127.0.0.1:1").await.is_err());
        assert!(
            super::connect_ws_via_proxy("ws://127.0.0.1:1", "127.0.0.1:9")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn run_spawns_tasks() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let mut cfg = base_settings(dir.path());
        cfg.relays_upstream = vec!["ws://127.0.0.1:1".into()];
        super::run(cfg, store, test_events_tx()).await;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn mirror_logs_ingest_errors() {
        use tokio_tungstenite::tungstenite::protocol::Message as TMsg;
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), true);
        store.init().unwrap();

        let bad_ev = serde_json::json!({
            "id": "bad", "pubkey": "p", "kind": 1,
            "created_at": 1, "tags": [], "content": "", "sig": ""
        });

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let _ = ws.next().await;
            ws.send(TMsg::Text(json!(["EVENT", "s", bad_ev]).to_string()))
                .await
                .unwrap();
            ws.send(TMsg::Text(json!(["EOSE", "s"]).to_string()))
                .await
                .unwrap();
        });
        let relay_url = format!("ws://{}", addr);
        let mut cfg = base_settings(dir.path());
        cfg.verify_sig = true;
        cfg.relays_upstream = vec![relay_url.clone()];
        mirror_relay(relay_url, cfg, store.clone()).await.unwrap();
        server.abort();
        assert!(!dir.path().join("events/ba/d0/bad.json").exists());
    }
}
