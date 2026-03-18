//! HTTP endpoints for health checks, relay info, and queries.

use anyhow::Result;
use axum::{
    body::Body,
    extract::{Query as AxumQuery, State},
    http::header,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{future::Future, net::SocketAddr, sync::Arc};

use crate::{
    config::Settings,
    mirror::{read_statuses, MirrorStatus},
    storage::{Query, Store},
};

#[derive(Clone)]
struct AppState {
    store: Store,
    settings: Settings,
}

/// Response body for the `/healthz` endpoint.
#[derive(Serialize, Deserialize)]
struct Health {
    /// Always "ok" when the server is running.
    status: String,
    mirrors_total: usize,
    mirrors_healthy: usize,
    last_mirror_success_at: Option<u64>,
}

/// Response body for the `/count` endpoint.
#[derive(Serialize, Deserialize)]
struct CountResponse {
    count: usize,
}

#[derive(Serialize, Deserialize)]
struct MirrorHealth {
    total: usize,
    healthy: usize,
    mirrors: Vec<MirrorHealthEntry>,
}

#[derive(Serialize, Deserialize)]
struct MirrorHealthEntry {
    cursor_key: String,
    relay: String,
    scope: String,
    state: String,
    last_connect_at: Option<u64>,
    last_event_at: Option<u64>,
    last_seen_event_created_at: Option<u64>,
    last_success_at: Option<u64>,
    seconds_since_last_success: Option<u64>,
    lag_seconds: Option<u64>,
    last_error: Option<String>,
}

/// Start an HTTP server exposing `/healthz`, `/query`, and relay info.
pub async fn serve_http(
    addr: SocketAddr,
    store: Store,
    settings: Settings,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let app = Router::new()
        .route("/", get(relay_info))
        .route("/healthz", get(healthz))
        .route("/mirror-health", get(mirror_health))
        .route("/query", get(query))
        .route("/count", get(count))
        .with_state(Arc::new(AppState { store, settings }));
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

/// Health check endpoint.
async fn healthz(State(state): State<Arc<AppState>>) -> Json<Health> {
    let statuses = read_statuses(&state.settings.store_root).unwrap_or_default();
    let now = unix_now();
    Json(Health {
        status: "ok".to_string(),
        mirrors_total: statuses.len(),
        mirrors_healthy: statuses
            .iter()
            .filter(|status| mirror_status_is_healthy(status, now))
            .count(),
        last_mirror_success_at: statuses.iter().filter_map(|status| status.last_success_at).max(),
    })
}

async fn mirror_health(State(state): State<Arc<AppState>>) -> Json<MirrorHealth> {
    let now = unix_now();
    let statuses = read_statuses(&state.settings.store_root).unwrap_or_default();
    let mirrors = statuses
        .iter()
        .map(|status| MirrorHealthEntry {
            cursor_key: status.cursor_key.clone(),
            relay: status.relay.clone(),
            scope: status.scope.clone(),
            state: status.state.clone(),
            last_connect_at: status.last_connect_at,
            last_event_at: status.last_event_at,
            last_seen_event_created_at: status.last_seen_event_created_at,
            last_success_at: status.last_success_at,
            seconds_since_last_success: status
                .last_success_at
                .map(|ts| now.saturating_sub(ts)),
            lag_seconds: status
                .last_seen_event_created_at
                .map(|ts| now.saturating_sub(ts)),
            last_error: status.last_error.clone(),
        })
        .collect::<Vec<_>>();
    let healthy = statuses
        .iter()
        .filter(|status| mirror_status_is_healthy(status, now))
        .count();
    Json(MirrorHealth {
        total: mirrors.len(),
        healthy,
        mirrors,
    })
}

/// Minimal NIP-11 relay information document.
#[derive(Serialize, Deserialize)]
struct RelayInfo {
    /// Human-readable relay name.
    name: String,
    /// Human-readable relay description.
    description: String,
    /// Software identifier (here it is always "stonr").
    software: String,
    /// Semantic version string such as "0.1.0".
    version: String,
    /// Supported NIP numbers advertised by this relay.
    supported_nips: Vec<u32>,
}

/// Basic NIP-11 relay information document.
async fn relay_info(
    State(state): State<Arc<AppState>>,
) -> axum::response::Response {
    if !state.settings.relay_info_enabled() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::NOT_FOUND)
            .body(Body::from("relay profile disabled"))
            .unwrap();
    }
    let mut supported_nips = vec![];
    if state.settings.relay_info_enabled() {
        supported_nips.push(11);
    }
    if state.settings.delete_enabled() {
        supported_nips.push(9);
    }
    if state.settings.tag_queries_enabled() {
        supported_nips.push(12);
    }
    if state.settings.expiration_enabled() {
        supported_nips.push(40);
    }
    if state.settings.count_enabled() {
        supported_nips.push(45);
    }
    if state.settings.search_enabled() {
        supported_nips.push(50);
    }
    axum::response::Response::builder()
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CONTENT_TYPE, "application/nostr+json")
        .body(Body::from(
            serde_json::to_vec(&RelayInfo {
                name: state.settings.relay_name.clone(),
                description: state.settings.relay_description.clone(),
                software: "stonr".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                supported_nips,
            })
            .unwrap(),
        ))
        .unwrap()
}

/// URL query parameters accepted by the `/query` endpoint.
#[derive(Deserialize)]
struct QueryParams {
    /// Comma-separated hex public keys.
    authors: Option<String>,
    /// Comma-separated kind numbers (e.g. `1,30023`).
    kinds: Option<String>,
    /// Single `#d` tag value.
    d: Option<String>,
    /// Single `#t` topic value.
    t: Option<String>,
    /// Minimum `created_at` timestamp.
    since: Option<String>,
    /// Maximum `created_at` timestamp.
    until: Option<String>,
    /// Maximum number of events to return.
    limit: Option<String>,
    /// Relay-side text search term.
    search: Option<String>,
}

/// Convert query string parameters into a [`Query`] understood by the store.
///
/// Supported URL parameters mirror Nostr filter fields:
/// - `authors` – comma-separated list of public keys
/// - `kinds` – comma-separated list of kind numbers
/// - `d` / `t` – single `#d` or `#t` tag value
/// - `since` / `until` – Unix timestamps bounding `created_at`
/// - `limit` – maximum number of events to return
///
/// Example: `/query?authors=npub1&kinds=1,30023&since=1700000000`
fn params_to_query(params: QueryParams) -> Query {
    use serde_json::Value;
    let mut obj = serde_json::Map::new();
    if let Some(a) = params.authors {
        let arr = a.split(',').map(|s| Value::String(s.to_string())).collect();
        obj.insert("authors".into(), Value::Array(arr));
    }
    if let Some(k) = params.kinds {
        let arr = k
            .split(',')
            .filter_map(|v| v.parse::<u32>().ok())
            .map(|v| Value::Number(v.into()))
            .collect();
        obj.insert("kinds".into(), Value::Array(arr));
    }
    if let Some(d) = params.d {
        obj.insert("#d".into(), Value::Array(vec![Value::String(d)]));
    }
    if let Some(t) = params.t {
        obj.insert("#t".into(), Value::Array(vec![Value::String(t)]));
    }
    if let Some(s) = params.since.and_then(|v| v.parse::<u64>().ok()) {
        obj.insert("since".into(), Value::Number(s.into()));
    }
    if let Some(u) = params.until.and_then(|v| v.parse::<u64>().ok()) {
        obj.insert("until".into(), Value::Number(u.into()));
    }
    if let Some(l) = params.limit.and_then(|v| v.parse::<u64>().ok()) {
        obj.insert("limit".into(), Value::Number(l.into()));
    }
    if let Some(search) = params.search {
        obj.insert("search".into(), Value::String(search));
    }
    Query::from_value(&Value::Object(obj))
}

/// Parse query parameters and return matching events as NDJSON.
async fn query(
    State(state): State<Arc<AppState>>,
    AxumQuery(params): AxumQuery<QueryParams>,
) -> axum::response::Response {
    if !state.settings.query_enabled() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(Body::from("read access disabled"))
            .unwrap();
    }
    // Translate URL parameters into a `Query` structure shared with the WS API.
    let q = params_to_query(params);
    if q.has_tag_filters() && !state.settings.tag_queries_enabled() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(Body::from("tag queries disabled"))
            .unwrap();
    }
    if q.search.is_some() && !state.settings.search_enabled() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(Body::from("text search disabled"))
            .unwrap();
    }
    let events = state
        .store
        .query_with_policy(
            q,
            state.settings.delete_enabled(),
            state.settings.expiration_enabled(),
        )
        .unwrap_or_default();
    // Return newline-delimited JSON so clients can stream and parse incrementally.
    let body = events
        .into_iter()
        .map(|e| serde_json::to_string(&e).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    axum::response::Response::builder()
        .header("Content-Type", "application/x-ndjson")
        .body(Body::from(body))
        .unwrap()
}

/// Parse query parameters and return only the number of matching events.
async fn count(
    State(state): State<Arc<AppState>>,
    AxumQuery(params): AxumQuery<QueryParams>,
) -> axum::response::Response {
    if !state.settings.query_enabled() || !state.settings.count_enabled() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(Body::from("count queries disabled"))
            .unwrap();
    }
    let q = params_to_query(params);
    if q.has_tag_filters() && !state.settings.tag_queries_enabled() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(Body::from("tag queries disabled"))
            .unwrap();
    }
    if q.search.is_some() && !state.settings.search_enabled() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(Body::from("text search disabled"))
            .unwrap();
    }
    axum::response::Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::to_vec(&CountResponse {
                count: state
                    .store
                    .query_with_policy(
                        q,
                        state.settings.delete_enabled(),
                        state.settings.expiration_enabled(),
                    )
                    .map(|events| events.len())
                    .unwrap_or(0),
            })
            .unwrap(),
        ))
        .unwrap()
}

fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn mirror_status_is_healthy(status: &MirrorStatus, now: u64) -> bool {
    if status.state == "error" {
        return false;
    }
    status
        .last_success_at
        .is_some_and(|ts| now.saturating_sub(ts) <= 600)
}

#[cfg(test)]
#[allow(clippy::single_match)]
mod tests {
    use super::*;
    use crate::{event::Event, mirror::{MirrorStatus, write_status}};
    use reqwest::{self, header::ACCESS_CONTROL_ALLOW_ORIGIN};
    use tempfile::TempDir;
    use tokio::task;

    fn test_settings(root: &std::path::Path) -> Settings {
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
            support_nip11: true,
            support_nip09: true,
            support_nip12: true,
            support_nip40: true,
            support_nip45: true,
            support_nip50: true,
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
        }
    }

    fn test_state(store: Store, root: &std::path::Path) -> Arc<AppState> {
        Arc::new(AppState {
            store,
            settings: test_settings(root),
        })
    }

    #[tokio::test]
    async fn health_endpoint() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/healthz", get(super::healthz))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/healthz", addr);
        let resp = reqwest::get(&url).await.unwrap();
        let body: super::Health = resp.json().await.unwrap();
        assert_eq!(body.status, "ok");
        assert_eq!(body.mirrors_total, 0);
        assert_eq!(body.mirrors_healthy, 0);
        assert!(body.last_mirror_success_at.is_none());
        handle.abort();
    }

    #[tokio::test]
    async fn mirror_health_endpoint_reports_runtime_status() {
        let dir = TempDir::new().unwrap();
        write_status(
            dir.path(),
            &MirrorStatus {
                cursor_key: "wss://relay.example".into(),
                relay: "wss://relay.example".into(),
                scope: "broad".into(),
                state: "running".into(),
                last_connect_at: Some(10),
                last_event_at: Some(20),
                last_seen_event_created_at: Some(30),
                last_eose_at: None,
                last_success_at: Some(super::unix_now()),
                last_error_at: None,
                last_error: None,
            },
        )
        .unwrap();

        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/mirror-health", get(super::mirror_health))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/mirror-health", addr);
        let body: super::MirrorHealth = reqwest::get(&url).await.unwrap().json().await.unwrap();
        assert_eq!(body.total, 1);
        assert_eq!(body.healthy, 1);
        assert_eq!(body.mirrors[0].relay, "wss://relay.example");
        assert_eq!(body.mirrors[0].scope, "broad");
        handle.abort();
    }

    #[tokio::test]
    async fn relay_info_endpoint() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/", get(super::relay_info))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/", addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(
            resp.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
            "*"
        );
        let info: super::RelayInfo = resp.json().await.unwrap();
        assert_eq!(info.name, "stonr");
        assert_eq!(info.description, "File-backed Nostr relay");
        assert_eq!(info.supported_nips, vec![11, 9, 12, 40, 45, 50]);
        handle.abort();
    }

    #[tokio::test]
    async fn relay_info_can_be_disabled() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.enable_nip11 = false;
        let app = Router::new()
            .route("/", get(super::relay_info))
            .with_state(Arc::new(AppState { store, settings }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/", addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
        handle.abort();
    }

    #[tokio::test]
    async fn query_endpoint_filters() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let events = vec![
            Event {
                id: "aa11".into(),
                pubkey: "p1".into(),
                kind: 1,
                created_at: 1,
                tags: vec![],
                content: String::new(),
                sig: String::new(),
            },
            Event {
                id: "bb22".into(),
                pubkey: "p1".into(),
                kind: 1,
                created_at: 2,
                tags: vec![],
                content: String::new(),
                sig: String::new(),
            },
            Event {
                id: "cc33".into(),
                pubkey: "p2".into(),
                kind: 1,
                created_at: 3,
                tags: vec![],
                content: String::new(),
                sig: String::new(),
            },
            Event {
                id: "dd44".into(),
                pubkey: "p1".into(),
                kind: 2,
                created_at: 4,
                tags: vec![],
                content: String::new(),
                sig: String::new(),
            },
        ];
        for ev in &events {
            store.ingest(ev).unwrap();
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });
        let url = format!(
            "http://{}/query?authors=p1,p2&kinds=1&since=2&until=3&limit=2",
            addr
        );
        let resp = reqwest::get(&url).await.unwrap().text().await.unwrap();
        let lines: Vec<_> = resp.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("cc33"));
        assert!(lines[1].contains("bb22"));
        handle.abort();
    }

    #[tokio::test]
    async fn count_endpoint_filters() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        for event in [
            Event {
                id: "aa11".into(),
                pubkey: "p1".into(),
                kind: 1,
                created_at: 1,
                tags: vec![],
                content: String::new(),
                sig: String::new(),
            },
            Event {
                id: "bb22".into(),
                pubkey: "p1".into(),
                kind: 1,
                created_at: 2,
                tags: vec![],
                content: String::new(),
                sig: String::new(),
            },
            Event {
                id: "cc33".into(),
                pubkey: "p2".into(),
                kind: 1,
                created_at: 3,
                tags: vec![],
                content: String::new(),
                sig: String::new(),
            },
        ] {
            store.ingest(&event).unwrap();
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/count", get(super::count))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/count?authors=p1&kinds=1", addr);
        let body: super::CountResponse = reqwest::get(&url).await.unwrap().json().await.unwrap();
        assert_eq!(body.count, 2);
        handle.abort();
    }

    #[tokio::test]
    async fn query_endpoint_hides_deleted_events() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let target = Event {
            id: "aa11".into(),
            pubkey: "p1".into(),
            kind: 1,
            created_at: 1,
            tags: vec![],
            content: String::new(),
            sig: String::new(),
        };
        let delete = Event {
            id: "dd44".into(),
            pubkey: "p1".into(),
            kind: 5,
            created_at: 2,
            tags: vec![crate::event::Tag(vec!["e".into(), "aa11".into()])],
            content: String::new(),
            sig: String::new(),
        };
        store.ingest(&target).unwrap();
        store.ingest_with_policy(&delete, true, true).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/query?authors=p1&kinds=1", addr);
        let body = reqwest::get(&url).await.unwrap().text().await.unwrap();
        assert!(body.trim().is_empty());
        handle.abort();
    }

    #[tokio::test]
    async fn query_can_be_disabled() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.enable_query = false;
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(Arc::new(AppState { store, settings }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/query", addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
        handle.abort();
    }

    #[tokio::test]
    async fn search_can_be_disabled() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.enable_search = false;
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(Arc::new(AppState { store, settings }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/query?search=hello", addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
        handle.abort();
    }

    #[tokio::test]
    async fn query_search_filters_content() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        for event in [
            Event {
                id: "aa11".into(),
                pubkey: "p1".into(),
                kind: 1,
                created_at: 1,
                tags: vec![],
                content: "hello world".into(),
                sig: String::new(),
            },
            Event {
                id: "bb22".into(),
                pubkey: "p1".into(),
                kind: 1,
                created_at: 2,
                tags: vec![],
                content: "goodbye".into(),
                sig: String::new(),
            },
        ] {
            store.ingest(&event).unwrap();
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/query?search=world", addr);
        let resp = reqwest::get(&url).await.unwrap().text().await.unwrap();
        let lines: Vec<_> = resp.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("aa11"));
        handle.abort();
    }

    #[tokio::test]
    async fn query_d_and_t_params() {
        use crate::event::Tag;
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev1 = Event {
            id: "aa11".into(),
            pubkey: "p1".into(),
            kind: 1,
            created_at: 1,
            tags: vec![
                Tag(vec!["d".into(), "slug1".into()]),
                Tag(vec!["t".into(), "tag1".into()]),
            ],
            content: String::new(),
            sig: String::new(),
        };
        let ev2 = Event {
            id: "bb22".into(),
            pubkey: "p2".into(),
            kind: 1,
            created_at: 2,
            tags: vec![
                Tag(vec!["d".into(), "slug2".into()]),
                Tag(vec!["t".into(), "tag2".into()]),
            ],
            content: String::new(),
            sig: String::new(),
        };
        store.ingest(&ev1).unwrap();
        store.ingest(&ev2).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });
        let url = format!("http://{}/query?d=slug1&t=tag1", addr);
        let resp = reqwest::get(&url).await.unwrap().text().await.unwrap();
        let lines: Vec<_> = resp.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("aa11"));
        handle.abort();
    }

    #[tokio::test]
    async fn query_replaceable_returns_latest() {
        use crate::event::Tag;
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let e1 = Event {
            id: "aa11".into(),
            pubkey: "p1".into(),
            kind: 30023,
            created_at: 1,
            tags: vec![Tag(vec!["d".into(), "slug".into()])],
            content: String::new(),
            sig: String::new(),
        };
        let e2 = Event {
            id: "bb22".into(),
            pubkey: "p1".into(),
            kind: 30023,
            created_at: 2,
            tags: vec![Tag(vec!["d".into(), "slug".into()])],
            content: String::new(),
            sig: String::new(),
        };
        store.ingest(&e1).unwrap();
        store.ingest(&e2).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });
        let url = format!(
            "http://{}/query?authors=p1&kinds=30023&d=slug&limit=10",
            addr
        );
        let resp = reqwest::get(&url).await.unwrap().text().await.unwrap();
        let lines: Vec<_> = resp.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("bb22"));
        handle.abort();
    }

    #[tokio::test]
    async fn query_no_matches() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev = Event {
            id: "aa11".into(),
            pubkey: "p1".into(),
            kind: 1,
            created_at: 1,
            tags: vec![],
            content: String::new(),
            sig: String::new(),
        };
        store.ingest(&ev).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });
        let url = format!("http://{}/query?authors=p2", addr);
        let resp = reqwest::get(&url).await.unwrap().text().await.unwrap();
        assert!(resp.is_empty());
        handle.abort();
    }

    #[tokio::test]
    async fn query_no_params_returns_empty() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });
        let url = format!("http://{}/query", addr);
        let resp = reqwest::get(&url).await.unwrap().text().await.unwrap();
        assert!(resp.is_empty());
        handle.abort();
    }

    #[tokio::test]
    async fn query_invalid_numbers_are_ignored() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });
        let url = format!("http://{}/query?since=oops&limit=nah", addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.text().await.unwrap();
        assert!(body.is_empty());
        handle.abort();
    }

    #[tokio::test]
    async fn serve_http_serves_health() {
        use std::time::Duration;
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let store_clone = store.clone();
        let settings = test_settings(dir.path());
        let shutdown = tokio::time::sleep(Duration::from_millis(1000));
        let handle = tokio::spawn(async move {
            super::serve_http(addr, store_clone, settings, shutdown)
                .await
                .unwrap();
        });
        let url = format!("http://{}/healthz", addr);
        let client = reqwest::Client::new();
        let mut body = None;
        for _ in 0..10 {
            match client.get(&url).send().await {
                Ok(resp) => match resp.json::<super::Health>().await {
                    Ok(health) => {
                        body = Some(health);
                        break;
                    }
                    Err(_) => {}
                },
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let resp = body.expect("health endpoint never became ready");
        assert_eq!(resp.status, "ok");
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn serve_http_bind_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        let settings = test_settings(dir.path());
        // binding to the same address should error because it's already taken
        assert!(super::serve_http(addr, store, settings, std::future::pending())
            .await
            .is_err());
    }
}
