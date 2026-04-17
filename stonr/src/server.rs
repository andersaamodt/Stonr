//! HTTP endpoints for health checks, relay info, and queries.

use anyhow::Result;
use axum::{
    body::Body,
    extract::{
        ConnectInfo, Multipart, OriginalUri, Path, Query as AxumQuery, RawQuery, Request, State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, options},
    Json, Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, future::Future, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::io::AsyncWriteExt;
use url::{form_urlencoded, Url};

use crate::{
    blossom::{self, Action as BlossomAction, VerifiedAuth},
    config::Settings,
    files::{self, BlobInfo, BlobMeta, UploadCandidate},
    mirror::{read_statuses, MirrorStatus},
    nip98,
    policy::{
        apply_query_policy, client_actor_label, current_unix_ts, enforce_rate_limit,
        RateLimitAction,
    },
    storage::{Query, RetentionStatus, Store},
};
use nostr_shared::parity;

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

/// Response body for the `/readyz` endpoint.
#[derive(Serialize, Deserialize)]
struct Readiness {
    status: String,
    issues: Vec<String>,
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

#[derive(Serialize, Deserialize)]
struct Nip96Info {
    api_url: String,
    download_url: String,
    supported_nips: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_types: Option<Vec<String>>,
    plans: BTreeMap<String, Nip96Plan>,
}

#[derive(Serialize, Deserialize)]
struct Nip96Plan {
    name: String,
    is_nip98_required: bool,
    max_byte_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Nip94EventDoc {
    tags: Vec<Vec<String>>,
    content: String,
    created_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileApiResponse {
    status: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    nip94_event: Option<Nip94EventDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    processing_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileListResponse {
    count: usize,
    total: usize,
    page: usize,
    files: Vec<Nip94EventDoc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MirrorRequest {
    url: String,
}

#[derive(Debug, Clone, Copy)]
struct Pagination {
    count: usize,
    page: usize,
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
        .route("/readyz", get(readyz))
        .route("/retention-health", get(retention_health))
        .route("/mirror-health", get(mirror_health))
        .route("/query", get(query))
        .route("/count", get(count))
        .route("/.well-known/nostr/nip96.json", get(nip96_info))
        .route(
            "/files",
            options(cors_preflight).get(list_files).post(upload_file),
        )
        .route(
            "/files/:blob_id",
            options(cors_preflight)
                .get(download_file)
                .delete(delete_file),
        )
        .route(
            "/upload",
            options(cors_preflight)
                .head(blossom_check_upload)
                .put(blossom_upload),
        )
        .route("/list/:pubkey", options(cors_preflight).get(blossom_list))
        .route("/mirror", options(cors_preflight).put(blossom_mirror))
        .route(
            "/:blob_id",
            options(cors_preflight)
                .head(blossom_head_blob)
                .get(blossom_get_blob)
                .delete(blossom_delete_blob),
        )
        .with_state(Arc::new(AppState { store, settings }));
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
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
        last_mirror_success_at: statuses
            .iter()
            .filter_map(|status| status.last_success_at)
            .max(),
    })
}

async fn readyz(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let issues = readiness_issues(&state);
    let status = if issues.is_empty() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = Readiness {
        status: if status == StatusCode::OK {
            "ready".into()
        } else {
            "not-ready".into()
        },
        issues,
    };
    (status, Json(body))
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
            seconds_since_last_success: status.last_success_at.map(|ts| now.saturating_sub(ts)),
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

fn readiness_issues(state: &AppState) -> Vec<String> {
    let mut issues = Vec::new();
    for rel in ["events", "log", "latest", "index", "cursor", "runtime"] {
        let path = state.settings.store_root.join(rel);
        if !path.is_dir() {
            issues.push(format!("missing store path: {rel}"));
        }
    }
    if state.settings.file_metadata_enabled()
        || state.settings.file_api_enabled()
        || state.settings.blossom_enabled()
    {
        for rel in [
            "files/blobs",
            "files/meta",
            "files/refs",
            "files/tmp",
            "files/quarantine",
        ] {
            let path = state.settings.store_root.join(rel);
            if !path.is_dir() {
                issues.push(format!("missing file path: {rel}"));
            }
        }
    }
    if let Err(error) = state.store.retention_status() {
        issues.push(format!("retention status unreadable: {error}"));
    }
    issues
}

async fn retention_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.store.retention_status() {
        Ok(status) => Json(status).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RetentionStatus {
                state: "error".into(),
                current_events: 0,
                current_bytes: 0,
                max_events: state.settings.max_stored_events,
                max_bytes: state.settings.max_stored_event_bytes,
                over_event_limit: false,
                over_byte_limit: false,
                warning: Some("Failed to read retention status.".into()),
                last_checked_at: unix_now(),
                last_prune_at: None,
                last_prune_removed: None,
                last_error_at: Some(unix_now()),
                last_error: Some(error.to_string()),
            }),
        )
            .into_response(),
    }
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
async fn relay_info(State(state): State<Arc<AppState>>) -> axum::response::Response {
    if !state.settings.relay_info_enabled() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::NOT_FOUND)
            .body(Body::from("relay profile disabled"))
            .unwrap();
    }
    let mut supported_nips = vec![];
    if let Ok(manifest) = parity::manifest() {
        for capability in manifest.required {
            let Some(number) = capability.number else {
                continue;
            };
            if nip_capability_enabled(&state.settings, &capability.id) {
                supported_nips.push(number);
            }
        }
    } else {
        // Fallback to the previous hard-coded subset if the shared manifest cannot be parsed.
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
        if state.settings.nip42_enabled() {
            supported_nips.push(42);
        }
        if state.settings.count_enabled() {
            supported_nips.push(45);
        }
        if state.settings.search_enabled() {
            supported_nips.push(50);
        }
        if state.settings.file_metadata_enabled() {
            supported_nips.push(94);
        }
        if state.settings.file_api_enabled() {
            supported_nips.push(96);
        }
        if state.settings.file_api_enabled() && state.settings.support_nip98 {
            supported_nips.push(98);
        }
    }
    supported_nips.sort_unstable();
    supported_nips.dedup();
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

fn nip_capability_enabled(settings: &Settings, id: &str) -> bool {
    match id {
        "NIP-09" => settings.delete_enabled(),
        "NIP-11" => settings.relay_info_enabled(),
        "NIP-12" => settings.tag_queries_enabled(),
        "NIP-40" => settings.expiration_enabled(),
        "NIP-42" => settings.nip42_enabled(),
        "NIP-45" => settings.count_enabled(),
        "NIP-50" => settings.search_enabled(),
        "NIP-94" => settings.file_metadata_enabled(),
        "NIP-96" => settings.file_api_enabled(),
        "NIP-98" => settings.file_api_enabled() && settings.support_nip98,
        "NIP-B7" => settings.blossom_enabled(),
        _ => false,
    }
}

/// URL query parameters accepted by the `/query` endpoint.
#[derive(Deserialize)]
struct QueryParams {
    /// Comma-separated event ids.
    ids: Option<String>,
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
/// - `ids` – comma-separated event ids
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
    if let Some(ids) = params.ids {
        let arr = ids.split(',').map(|s| Value::String(s.to_string())).collect();
        obj.insert("ids".into(), Value::Array(arr));
    }
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
    connect: Option<ConnectInfo<SocketAddr>>,
    AxumQuery(params): AxumQuery<QueryParams>,
) -> axum::response::Response {
    if !state.settings.query_enabled() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(Body::from("read access disabled"))
            .unwrap();
    }
    if state.settings.query_auth_required() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(Body::from("relay auth required over websocket"))
            .unwrap();
    }
    let actor = client_actor_label(connect.map(|info| info.0), None);
    if let Err(error) = enforce_rate_limit(
        &state.settings,
        RateLimitAction::Query,
        &actor,
        current_unix_ts(),
    ) {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::TOO_MANY_REQUESTS)
            .body(Body::from(error.to_string()))
            .unwrap();
    }
    // Translate URL parameters into a `Query` structure shared with the WS API.
    let q = apply_query_policy(&state.settings, params_to_query(params));
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
    connect: Option<ConnectInfo<SocketAddr>>,
    AxumQuery(params): AxumQuery<QueryParams>,
) -> axum::response::Response {
    if !state.settings.query_enabled() || !state.settings.count_enabled() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(Body::from("count queries disabled"))
            .unwrap();
    }
    if state.settings.count_auth_required() {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::FORBIDDEN)
            .body(Body::from("relay auth required over websocket"))
            .unwrap();
    }
    let actor = client_actor_label(connect.map(|info| info.0), None);
    if let Err(error) = enforce_rate_limit(
        &state.settings,
        RateLimitAction::Count,
        &actor,
        current_unix_ts(),
    ) {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::TOO_MANY_REQUESTS)
            .body(Body::from(error.to_string()))
            .unwrap();
    }
    let q = apply_query_policy(&state.settings, params_to_query(params));
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

async fn nip96_info(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !state.settings.file_api_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let api_url = advertised_file_api_url(&headers, &state.settings);
    let mut plans = BTreeMap::new();
    plans.insert(
        "free".into(),
        Nip96Plan {
            name: "free".into(),
            is_nip98_required: state.settings.nip98_auth_required(),
            max_byte_size: state.settings.file_max_bytes,
        },
    );
    let content_types = state.settings.file_allowed_mime.as_ref().map(|values| {
        let mut values = values.clone();
        values.sort();
        values
    });
    json_response(
        StatusCode::OK,
        Nip96Info {
            api_url: api_url.clone(),
            download_url: api_url,
            supported_nips: if state.settings.file_api_enabled() && state.settings.support_nip98 {
                vec![98]
            } else {
                vec![]
            },
            content_types,
            plans,
        },
    )
}

async fn cors_preflight() -> axum::response::Response {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"))
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "Authorization, Content-Type, Content-Length, X-Content-Length, X-Content-Type, X-SHA-256",
            ),
        )
        .header(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET,HEAD,PUT,POST,DELETE,OPTIONS"),
        )
        .body(Body::empty())
        .unwrap()
}

async fn upload_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
    mut multipart: Multipart,
) -> axum::response::Response {
    if !state.settings.file_api_enabled() {
        return file_api_error(StatusCode::FORBIDDEN, "blocked: file API disabled");
    }
    if headers.contains_key(header::AUTHORIZATION) && !state.settings.support_nip98 {
        return file_api_error(
            StatusCode::FORBIDDEN,
            "blocked: NIP-98 auth support disabled",
        );
    }
    let file_store = state.store.files();
    if let Err(error) = file_store.init() {
        return internal_file_api_error(error);
    }

    let temp_path = temp_upload_path(&state);
    let mut file_seen = false;
    let mut file_name = None;
    let mut detected_mime = None;
    let mut bytes_written = 0usize;
    let mut text_fields = vec![];

    loop {
        let next = match multipart.next_field().await {
            Ok(next) => next,
            Err(error) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return file_api_error(
                    StatusCode::BAD_REQUEST,
                    &format!("invalid: multipart parse failed: {error}"),
                );
            }
        };
        let Some(mut field) = next else {
            break;
        };
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            if file_seen {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return file_api_error(StatusCode::BAD_REQUEST, "invalid: multiple file fields");
            }
            file_seen = true;
            file_name = field.file_name().map(ToString::to_string);
            detected_mime = field.content_type().map(ToString::to_string);

            let file = match tokio::fs::File::create(&temp_path).await {
                Ok(file) => file,
                Err(error) => return internal_file_api_error(error.into()),
            };
            let mut writer = tokio::io::BufWriter::new(file);
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        bytes_written += chunk.len();
                        if bytes_written > state.settings.file_max_bytes {
                            let _ = tokio::fs::remove_file(&temp_path).await;
                            return file_api_error(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                "blocked: file exceeds max size",
                            );
                        }
                        if let Err(error) = writer.write_all(&chunk).await {
                            let _ = tokio::fs::remove_file(&temp_path).await;
                            return internal_file_api_error(error.into());
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        return file_api_error(
                            StatusCode::BAD_REQUEST,
                            &format!("invalid: multipart read failed: {error}"),
                        );
                    }
                }
            }
            if let Err(error) = writer.flush().await {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return internal_file_api_error(error.into());
            }
        } else {
            match field.text().await {
                Ok(value) => text_fields.push((name, value)),
                Err(error) => {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return file_api_error(
                        StatusCode::BAD_REQUEST,
                        &format!("invalid: failed to read form field: {error}"),
                    );
                }
            }
        }
    }

    if !file_seen {
        return file_api_error(StatusCode::BAD_REQUEST, "invalid: missing file field");
    }

    let fields = files::parse_text_fields(&text_fields);
    let expires_at = match optional_u64_field(&fields, "expiration") {
        Ok(value) => value,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return file_api_error(StatusCode::BAD_REQUEST, &error.to_string());
        }
    };
    let mime = fields
        .get("content_type")
        .cloned()
        .or(detected_mime)
        .unwrap_or_else(|| "application/octet-stream".into());

    let (payload_hash, _) = match files::hash_file(&temp_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return internal_file_api_error(error);
        }
    };
    let request_url = request_absolute_url(&headers, &uri);
    let owner = match optional_auth_pubkey(
        state.settings.nip98_auth_required(),
        &headers,
        "POST",
        &request_url,
        Some(&payload_hash),
    ) {
        Ok(owner) => owner,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return file_api_error(StatusCode::UNAUTHORIZED, &error.to_string());
        }
    };
    let existed = match file_store.load_meta(&payload_hash) {
        Ok(meta) => meta.is_some(),
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return internal_file_api_error(error);
        }
    };
    let advertised_url = advertised_file_api_url(&headers, &state.settings);
    let info = match file_store.store_upload(
        UploadCandidate {
            temp_path: temp_path.clone(),
            filename: file_name.or_else(|| fields.get("filename").cloned()),
            mime,
            owner,
            expires_at,
        },
        &state.settings,
        &advertised_url,
    ) {
        Ok(info) => info,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return file_api_error(status_for_file_error(&error), &error.to_string());
        }
    };
    let meta = match file_store.load_meta(&info.sha256) {
        Ok(Some(meta)) => meta,
        Ok(None) => {
            return internal_file_api_error(anyhow::anyhow!("missing blob metadata after upload"))
        }
        Err(error) => return internal_file_api_error(error),
    };
    json_response(
        if existed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        FileApiResponse {
            status: "success".into(),
            message: if existed {
                "file already present".into()
            } else {
                "upload accepted".into()
            },
            nip94_event: Some(blob_to_nip94_event(&info, &meta, &fields)),
            processing_url: None,
        },
    )
}

async fn list_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
    RawQuery(raw): RawQuery,
) -> axum::response::Response {
    if !state.settings.file_api_enabled() {
        return file_api_error(StatusCode::FORBIDDEN, "blocked: file API disabled");
    }
    if !state.settings.support_nip98 {
        return file_api_error(
            StatusCode::FORBIDDEN,
            "blocked: NIP-98 auth support disabled",
        );
    }
    let request_url = request_absolute_url(&headers, &uri);
    let owner = match optional_auth_pubkey(true, &headers, "GET", &request_url, None) {
        Ok(Some(owner)) => owner,
        Ok(None) => unreachable!(),
        Err(error) => return file_api_error(StatusCode::UNAUTHORIZED, &error.to_string()),
    };
    let page = params_to_pagination(raw.as_deref(), &state.settings);
    let base_url = advertised_file_api_url(&headers, &state.settings);
    let mut files = vec![];
    let listed = match state.store.files().list(&base_url) {
        Ok(files) => files,
        Err(error) => return internal_file_api_error(error),
    };
    for info in listed {
        let Ok(Some(meta)) = state.store.files().load_meta(&info.sha256) else {
            continue;
        };
        if !meta.owners.contains(&owner) {
            continue;
        }
        files.push(blob_to_nip94_event(&info, &meta, &Default::default()));
    }
    files.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    let total = files.len();
    let start = page.page.saturating_mul(page.count).min(total);
    let end = start.saturating_add(page.count).min(total);
    json_response(
        StatusCode::OK,
        FileListResponse {
            count: end.saturating_sub(start),
            total,
            page: page.page,
            files: files[start..end].to_vec(),
        },
    )
}

async fn download_file(
    State(state): State<Arc<AppState>>,
    Path(blob_id): Path<String>,
) -> axum::response::Response {
    if !state.settings.file_api_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let hash = match normalize_blob_id(&blob_id) {
        Ok(hash) => hash,
        Err(error) => return file_api_error(StatusCode::BAD_REQUEST, &error.to_string()),
    };
    let meta = match state.store.files().load_meta(&hash) {
        Ok(Some(meta)) => meta,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_file_api_error(error),
    };
    if meta.expires_at.is_some_and(|ts| ts <= unix_now()) {
        return StatusCode::GONE.into_response();
    }
    if !state.settings.file_hash_allowed(&hash) || !state.settings.file_mime_allowed(&meta.mime) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let data = match tokio::fs::read(state.store.files().blob_path(&hash)).await {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response()
        }
        Err(error) => return internal_file_api_error(error.into()),
    };
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Authorization, Content-Type"),
        )
        .header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, immutable"),
        )
        .header(header::CONTENT_TYPE, meta.mime.as_str());
    if let Some(name) = meta.original_name.as_deref() {
        builder = builder.header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", sanitized_filename(name)),
        );
    }
    builder.body(Body::from(data)).unwrap()
}

async fn delete_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
    Path(blob_id): Path<String>,
) -> axum::response::Response {
    if !state.settings.file_api_enabled() {
        return file_api_error(StatusCode::FORBIDDEN, "blocked: file API disabled");
    }
    if !state.settings.support_nip98 {
        return file_api_error(
            StatusCode::FORBIDDEN,
            "blocked: NIP-98 auth support disabled",
        );
    }
    let hash = match normalize_blob_id(&blob_id) {
        Ok(hash) => hash,
        Err(error) => return file_api_error(StatusCode::BAD_REQUEST, &error.to_string()),
    };
    let request_url = request_absolute_url(&headers, &uri);
    let owner = match optional_auth_pubkey(true, &headers, "DELETE", &request_url, None) {
        Ok(Some(owner)) => owner,
        Ok(None) => unreachable!(),
        Err(error) => return file_api_error(StatusCode::UNAUTHORIZED, &error.to_string()),
    };
    match state
        .store
        .files()
        .delete_for_owner(&hash, &owner, &state.settings)
    {
        Ok(true) => json_response(
            StatusCode::OK,
            FileApiResponse {
                status: "success".into(),
                message: "file deleted".into(),
                nip94_event: None,
                processing_url: None,
            },
        ),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => file_api_error(status_for_file_error(&error), &error.to_string()),
    }
}

async fn blossom_check_upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> axum::response::Response {
    if !state.settings.blossom_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let host_domain = request_host_domain(&headers);
    let auth = match blossom_optional_auth(
        state.settings.blossom_write_auth_required(),
        &headers,
        BlossomAction::Upload,
        &host_domain,
    ) {
        Ok(auth) => auth,
        Err(error) => return blossom_error(StatusCode::UNAUTHORIZED, &error.to_string()),
    };
    let Some(hash) = header_string(&headers, "x-sha-256") else {
        return blossom_error(StatusCode::BAD_REQUEST, "invalid: missing X-SHA-256 header");
    };
    let Some(mime) = header_string(&headers, "x-content-type") else {
        return blossom_error(
            StatusCode::BAD_REQUEST,
            "invalid: missing X-Content-Type header",
        );
    };
    let Some(length) = header_string(&headers, "x-content-length") else {
        return blossom_error(
            StatusCode::BAD_REQUEST,
            "invalid: missing X-Content-Length header",
        );
    };
    let length = match length.parse::<u64>() {
        Ok(length) => length,
        Err(_) => {
            return blossom_error(
                StatusCode::BAD_REQUEST,
                "invalid: X-Content-Length must be an integer",
            )
        }
    };
    if let Some(auth) = &auth {
        if let Err(error) = auth.require_hash(&hash) {
            return blossom_error(StatusCode::FORBIDDEN, &error.to_string());
        }
    }
    if let Err(error) = validate_upload_requirements(&state.settings, &hash, &mime, length) {
        return blossom_error(status_for_file_error(&error), &error.to_string());
    }
    blossom_head_ok("upload permitted")
}

async fn blossom_upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: Request,
) -> axum::response::Response {
    if !state.settings.blossom_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let file_store = state.store.files();
    if let Err(error) = file_store.init() {
        return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
    }
    let Some(expected_hash) = header_string(&headers, "x-sha-256") else {
        return blossom_error(StatusCode::BAD_REQUEST, "invalid: missing X-SHA-256 header");
    };
    let mime = header_string(&headers, header::CONTENT_TYPE.as_str())
        .unwrap_or_else(|| "application/octet-stream".into());
    let declared_len = header_string(&headers, header::CONTENT_LENGTH.as_str())
        .and_then(|value| value.parse::<u64>().ok());
    if let Some(length) = declared_len {
        if let Err(error) =
            validate_upload_requirements(&state.settings, &expected_hash, &mime, length)
        {
            return blossom_error(status_for_file_error(&error), &error.to_string());
        }
    } else if !is_valid_hash(&expected_hash) {
        return blossom_error(
            StatusCode::BAD_REQUEST,
            "invalid: X-SHA-256 must be 64 hex chars",
        );
    }
    let host_domain = request_host_domain(&headers);
    let auth = match blossom_optional_auth(
        state.settings.blossom_write_auth_required(),
        &headers,
        BlossomAction::Upload,
        &host_domain,
    ) {
        Ok(auth) => auth,
        Err(error) => return blossom_error(StatusCode::UNAUTHORIZED, &error.to_string()),
    };
    if let Some(auth) = &auth {
        if let Err(error) = auth.require_hash(&expected_hash) {
            return blossom_error(StatusCode::FORBIDDEN, &error.to_string());
        }
    }
    let temp_path = temp_upload_path(&state);
    match write_request_body_to_temp(request, &temp_path, state.settings.file_max_bytes).await {
        Ok(_) => {}
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return blossom_error(status_for_file_error(&error), &error.to_string());
        }
    }
    let (actual_hash, _) = match files::hash_file(&temp_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
        }
    };
    if actual_hash != expected_hash.to_ascii_lowercase() {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return blossom_error(
            StatusCode::BAD_REQUEST,
            "invalid: uploaded bytes do not match X-SHA-256",
        );
    }
    let existed = match file_store.load_meta(&actual_hash) {
        Ok(meta) => meta.is_some(),
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
        }
    };
    let origin = advertised_blossom_origin(&headers, &state.settings);
    if let Err(error) = file_store.store_upload(
        UploadCandidate {
            temp_path: temp_path.clone(),
            filename: None,
            mime,
            owner: auth.map(|auth| auth.pubkey),
            expires_at: None,
        },
        &state.settings,
        &origin,
    ) {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return blossom_error(status_for_file_error(&error), &error.to_string());
    }
    let meta = match file_store.load_meta(&actual_hash) {
        Ok(Some(meta)) => meta,
        Ok(None) => {
            return blossom_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "missing blob metadata after upload",
            )
        }
        Err(error) => return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    blossom_json_response(
        if existed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        blossom::descriptor(&meta, &origin),
    )
}

async fn blossom_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(pubkey): Path<String>,
    RawQuery(raw): RawQuery,
) -> axum::response::Response {
    if !state.settings.blossom_list_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let host_domain = request_host_domain(&headers);
    let auth = match blossom_optional_auth(true, &headers, BlossomAction::List, &host_domain) {
        Ok(auth) => auth,
        Err(error) => return blossom_error(StatusCode::UNAUTHORIZED, &error.to_string()),
    };
    if let Some(auth) = &auth {
        if auth.pubkey != pubkey {
            return blossom_error(StatusCode::FORBIDDEN, "blocked: auth pubkey mismatch");
        }
    }
    let page = params_to_pagination(raw.as_deref(), &state.settings);
    let origin = advertised_blossom_origin(&headers, &state.settings);
    let mut descriptors = match state.store.files().all_meta() {
        Ok(metas) => metas
            .into_iter()
            .filter(|meta| meta.owners.contains(&pubkey))
            .map(|meta| blossom::descriptor(&meta, &origin))
            .collect::<Vec<_>>(),
        Err(error) => return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    descriptors.sort_by(|left, right| {
        right
            .uploaded
            .cmp(&left.uploaded)
            .then_with(|| right.sha256.cmp(&left.sha256))
    });
    let start = page.page.saturating_mul(page.count).min(descriptors.len());
    let end = start.saturating_add(page.count).min(descriptors.len());
    blossom_json_response(StatusCode::OK, descriptors[start..end].to_vec())
}

async fn blossom_mirror(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<MirrorRequest>,
) -> axum::response::Response {
    if !state.settings.blossom_mirror_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let file_store = state.store.files();
    if let Err(error) = file_store.init() {
        return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
    }
    let host_domain = request_host_domain(&headers);
    let auth = match blossom_optional_auth(true, &headers, BlossomAction::Upload, &host_domain) {
        Ok(auth) => auth,
        Err(error) => return blossom_error(StatusCode::UNAUTHORIZED, &error.to_string()),
    };
    let source = match Url::parse(&body.url) {
        Ok(url) if matches!(url.scheme(), "http" | "https") => url,
        _ => {
            return blossom_error(
                StatusCode::BAD_REQUEST,
                "invalid: mirror url must be http or https",
            )
        }
    };
    let response = match reqwest::Client::new().get(source.clone()).send().await {
        Ok(response) => response,
        Err(error) => {
            return blossom_error(
                StatusCode::BAD_GATEWAY,
                &format!("invalid: mirror fetch failed: {error}"),
            )
        }
    };
    if !response.status().is_success() {
        return blossom_error(
            StatusCode::BAD_GATEWAY,
            &format!("invalid: mirror source returned {}", response.status()),
        );
    }
    if let Some(length) = response.content_length() {
        if length > state.settings.file_max_bytes as u64 {
            return blossom_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "blocked: file exceeds max size",
            );
        }
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| "application/octet-stream".into());
    if !state.settings.file_mime_allowed(&mime) {
        return blossom_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "blocked: MIME type not allowed",
        );
    }
    let temp_path = temp_upload_path(&state);
    let mut writer = match tokio::fs::File::create(&temp_path).await {
        Ok(file) => tokio::io::BufWriter::new(file),
        Err(error) => return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    let mut written = 0usize;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return blossom_error(
                    StatusCode::BAD_GATEWAY,
                    &format!("invalid: mirror stream failed: {error}"),
                );
            }
        };
        written += chunk.len();
        if written > state.settings.file_max_bytes {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return blossom_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "blocked: file exceeds max size",
            );
        }
        if let Err(error) = writer.write_all(&chunk).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
        }
    }
    if let Err(error) = writer.flush().await {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
    }
    let (hash, _) = match files::hash_file(&temp_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
        }
    };
    if let Some(auth) = &auth {
        if let Err(error) = auth.require_hash(&hash) {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return blossom_error(StatusCode::FORBIDDEN, &error.to_string());
        }
    }
    let existed = match file_store.load_meta(&hash) {
        Ok(meta) => meta.is_some(),
        Err(error) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
        }
    };
    let origin = advertised_blossom_origin(&headers, &state.settings);
    if let Err(error) = file_store.store_upload(
        UploadCandidate {
            temp_path: temp_path.clone(),
            filename: filename_from_url(&source),
            mime,
            owner: auth.map(|auth| auth.pubkey),
            expires_at: None,
        },
        &state.settings,
        &origin,
    ) {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return blossom_error(status_for_file_error(&error), &error.to_string());
    }
    let meta = match file_store.load_meta(&hash) {
        Ok(Some(meta)) => meta,
        Ok(None) => {
            return blossom_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "missing blob metadata after mirror",
            )
        }
        Err(error) => return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    blossom_json_response(
        if existed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        blossom::descriptor(&meta, &origin),
    )
}

async fn blossom_get_blob(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(blob_id): Path<String>,
) -> axum::response::Response {
    blossom_blob_response(state, headers, blob_id, false).await
}

async fn blossom_head_blob(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(blob_id): Path<String>,
) -> axum::response::Response {
    blossom_blob_response(state, headers, blob_id, true).await
}

async fn blossom_delete_blob(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(blob_id): Path<String>,
) -> axum::response::Response {
    if !state.settings.blossom_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let hash = match normalize_blob_id(&blob_id) {
        Ok(hash) => hash,
        Err(error) => return blossom_error(StatusCode::BAD_REQUEST, &error.to_string()),
    };
    let meta = match state.store.files().load_meta(&hash) {
        Ok(Some(meta)) => meta,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    let host_domain = request_host_domain(&headers);
    let auth = match blossom::verify_auth(&headers, BlossomAction::Delete, &host_domain) {
        Ok(auth) => auth,
        Err(error) => return blossom_error(StatusCode::UNAUTHORIZED, &error.to_string()),
    };
    if let Err(error) = auth.require_hash(&hash) {
        return blossom_error(StatusCode::FORBIDDEN, &error.to_string());
    }
    match state
        .store
        .files()
        .delete_for_owner(&hash, &auth.pubkey, &state.settings)
    {
        Ok(true) => blossom_json_response(
            StatusCode::OK,
            blossom::descriptor(&meta, &advertised_blossom_origin(&headers, &state.settings)),
        ),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => blossom_error(status_for_file_error(&error), &error.to_string()),
    }
}

async fn blossom_blob_response(
    state: Arc<AppState>,
    headers: HeaderMap,
    blob_id: String,
    head_only: bool,
) -> axum::response::Response {
    if !state.settings.blossom_enabled() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let hash = match normalize_blob_id(&blob_id) {
        Ok(hash) => hash,
        Err(error) => return blossom_error(StatusCode::BAD_REQUEST, &error.to_string()),
    };
    let meta = match state.store.files().load_meta(&hash) {
        Ok(Some(meta)) => meta,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    if meta.expires_at.is_some_and(|ts| ts <= unix_now()) {
        return StatusCode::GONE.into_response();
    }
    if !state.settings.file_hash_allowed(&hash) || !state.settings.file_mime_allowed(&meta.mime) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if state.settings.blossom_get_auth_required() || headers.contains_key(header::AUTHORIZATION) {
        let host_domain = request_host_domain(&headers);
        let auth = match blossom::verify_auth(&headers, BlossomAction::Get, &host_domain) {
            Ok(auth) => auth,
            Err(error) => return blossom_error(StatusCode::UNAUTHORIZED, &error.to_string()),
        };
        if !auth.hashes.is_empty() {
            if let Err(error) = auth.require_hash(&hash) {
                return blossom_error(StatusCode::FORBIDDEN, &error.to_string());
            }
        }
    }
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"))
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "Authorization, Content-Type, Content-Length, X-Content-Length, X-Content-Type, X-SHA-256",
            ),
        )
        .header(header::CONTENT_TYPE, meta.mime.as_str())
        .header(header::CONTENT_LENGTH, meta.size.to_string())
        .header("X-Content-Type-Options", HeaderValue::from_static("nosniff"))
        .header(header::CACHE_CONTROL, HeaderValue::from_static("public, immutable"));
    if let Some(name) = meta.original_name.as_deref() {
        builder = builder.header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", sanitized_filename(name)),
        );
    }
    if head_only {
        return builder.body(Body::empty()).unwrap();
    }
    let data = match tokio::fs::read(state.store.files().blob_path(&hash)).await {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response()
        }
        Err(error) => return blossom_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    builder.body(Body::from(data)).unwrap()
}

fn params_to_pagination(raw: Option<&str>, settings: &Settings) -> Pagination {
    let max = settings.max_limit.unwrap_or(200).max(1);
    let mut page = 0usize;
    let mut count = 50usize.min(max);
    if let Some(raw) = raw {
        for (key, value) in form_urlencoded::parse(raw.as_bytes()) {
            match key.as_ref() {
                "page" => page = value.parse::<usize>().unwrap_or(0),
                "count" => count = value.parse::<usize>().unwrap_or(count).clamp(1, max),
                _ => {}
            }
        }
    }
    Pagination { count, page }
}

fn optional_auth_pubkey(
    required: bool,
    headers: &HeaderMap,
    method: &str,
    url: &str,
    payload_hash_hex: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let has_auth = headers.contains_key(header::AUTHORIZATION);
    if !required && !has_auth {
        return Ok(None);
    }
    Ok(Some(nip98::verify_http_auth(
        headers,
        method,
        url,
        payload_hash_hex,
    )?))
}

fn blossom_optional_auth(
    required: bool,
    headers: &HeaderMap,
    action: BlossomAction,
    host_domain: &str,
) -> anyhow::Result<Option<VerifiedAuth>> {
    let has_auth = headers.contains_key(header::AUTHORIZATION);
    if !required && !has_auth {
        return Ok(None);
    }
    Ok(Some(blossom::verify_auth(headers, action, host_domain)?))
}

fn validate_upload_requirements(
    settings: &Settings,
    hash: &str,
    mime: &str,
    length: u64,
) -> anyhow::Result<()> {
    if !is_valid_hash(hash) {
        return Err(anyhow::anyhow!("invalid: X-SHA-256 must be 64 hex chars"));
    }
    if length > settings.file_max_bytes as u64 {
        return Err(anyhow::anyhow!("blocked: file exceeds max size"));
    }
    if !settings.file_mime_allowed(mime) {
        return Err(anyhow::anyhow!("blocked: MIME type not allowed"));
    }
    if !settings.file_hash_allowed(hash) {
        return Err(anyhow::anyhow!("blocked: file hash denylisted"));
    }
    Ok(())
}

fn optional_u64_field(
    fields: &std::collections::HashMap<String, String>,
    name: &str,
) -> anyhow::Result<Option<u64>> {
    match fields.get(name) {
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => value
            .parse::<u64>()
            .map(Some)
            .map_err(|_| anyhow::anyhow!("invalid: field {name} must be a unix timestamp")),
        None => Ok(None),
    }
}

fn blob_to_nip94_event(
    info: &BlobInfo,
    meta: &BlobMeta,
    fields: &std::collections::HashMap<String, String>,
) -> Nip94EventDoc {
    let mut tags = files::nip94_tags(info);
    if let Some(expires_at) = meta.expires_at {
        tags.push(vec!["expiration".into(), expires_at.to_string()]);
    }
    if let Some(alt) = fields.get("alt").filter(|value| !value.trim().is_empty()) {
        tags.push(vec!["alt".into(), alt.clone()]);
    }
    tags.push(vec!["service".into(), "NIP-96".into()]);
    Nip94EventDoc {
        tags,
        content: fields.get("caption").cloned().unwrap_or_default(),
        created_at: meta.uploaded_at,
    }
}

fn request_absolute_url(headers: &HeaderMap, uri: &Uri) -> String {
    let scheme = forwarded_value(headers, "x-forwarded-proto").unwrap_or("http");
    let host = forwarded_value(headers, "x-forwarded-host")
        .or_else(|| forwarded_value(headers, header::HOST.as_str()))
        .unwrap_or("localhost");
    let path = uri
        .path_and_query()
        .map(axum::http::uri::PathAndQuery::as_str)
        .unwrap_or("/");
    format!("{scheme}://{host}{path}")
}

fn advertised_file_api_url(headers: &HeaderMap, settings: &Settings) -> String {
    settings
        .file_api_url
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}/files", request_origin(headers)))
}

fn advertised_blossom_origin(headers: &HeaderMap, settings: &Settings) -> String {
    settings
        .blossom_public_url
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| request_origin(headers))
}

fn request_origin(headers: &HeaderMap) -> String {
    let scheme = forwarded_value(headers, "x-forwarded-proto").unwrap_or("http");
    let host = forwarded_value(headers, "x-forwarded-host")
        .or_else(|| forwarded_value(headers, header::HOST.as_str()))
        .unwrap_or("localhost");
    format!("{scheme}://{host}")
}

fn request_host_domain(headers: &HeaderMap) -> String {
    let raw = forwarded_value(headers, "x-forwarded-host")
        .or_else(|| forwarded_value(headers, header::HOST.as_str()))
        .unwrap_or("localhost");
    Url::parse(&format!("http://{raw}"))
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| raw.split(':').next().unwrap_or("localhost").to_string())
        .to_ascii_lowercase()
}

fn forwarded_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalize_blob_id(blob_id: &str) -> anyhow::Result<String> {
    let hash = blob_id
        .split_once('.')
        .map(|(hash, _)| hash)
        .unwrap_or(blob_id)
        .trim();
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(anyhow::anyhow!(
            "invalid: blob id must be a 64-byte hex sha256"
        ));
    }
    Ok(hash.to_ascii_lowercase())
}

fn is_valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn temp_upload_path(state: &AppState) -> PathBuf {
    state
        .store
        .files()
        .temp_dir()
        .join(format!("upload-{}-{}", std::process::id(), now_nanos()))
}

async fn write_request_body_to_temp(
    request: Request,
    temp_path: &std::path::Path,
    max_bytes: usize,
) -> anyhow::Result<usize> {
    let file = tokio::fs::File::create(temp_path).await?;
    let mut writer = tokio::io::BufWriter::new(file);
    let mut stream = request.into_body().into_data_stream();
    let mut written = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        written += chunk.len();
        if written > max_bytes {
            return Err(anyhow::anyhow!("blocked: file exceeds max size"));
        }
        writer.write_all(&chunk).await?;
    }
    writer.flush().await?;
    Ok(written)
}

fn header_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn filename_from_url(url: &Url) -> Option<String> {
    url.path_segments()
        .and_then(|mut segments| segments.next_back())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn sanitized_filename(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            '"' | '\\' | '\n' | '\r' => '_',
            _ => ch,
        })
        .collect()
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn status_for_file_error(error: &anyhow::Error) -> StatusCode {
    let message = error.to_string();
    if message.contains("max size") {
        StatusCode::PAYLOAD_TOO_LARGE
    } else if message.contains("missing Authorization")
        || message.contains("unsupported Authorization")
        || message.contains("auth ")
    {
        StatusCode::UNAUTHORIZED
    } else if message.starts_with("invalid:") {
        StatusCode::BAD_REQUEST
    } else if message.starts_with("blocked:") {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

fn json_response<T: Serialize>(status: StatusCode, payload: T) -> axum::response::Response {
    let body = serde_json::to_vec(&payload).unwrap();
    Response::builder()
        .status(status)
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Authorization, Content-Type"),
        )
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )
        .body(Body::from(body))
        .unwrap()
}

fn blossom_json_response<T: Serialize>(status: StatusCode, payload: T) -> axum::response::Response {
    let body = serde_json::to_vec(&payload).unwrap();
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"))
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "Authorization, Content-Type, Content-Length, X-Content-Length, X-Content-Type, X-SHA-256",
            ),
        )
        .header(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))
        .body(Body::from(body))
        .unwrap()
}

fn blossom_head_ok(reason: &str) -> axum::response::Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"))
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "Authorization, Content-Type, Content-Length, X-Content-Length, X-Content-Type, X-SHA-256",
            ),
        )
        .header("X-Reason", reason)
        .body(Body::empty())
        .unwrap()
}

fn file_api_error(status: StatusCode, message: &str) -> axum::response::Response {
    json_response(
        status,
        FileApiResponse {
            status: "error".into(),
            message: message.into(),
            nip94_event: None,
            processing_url: None,
        },
    )
}

fn internal_file_api_error(error: anyhow::Error) -> axum::response::Response {
    file_api_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string())
}

fn blossom_error(status: StatusCode, message: &str) -> axum::response::Response {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"))
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(
                "Authorization, Content-Type, Content-Length, X-Content-Length, X-Content-Type, X-SHA-256",
            ),
        )
        .header("X-Reason", message)
        .body(Body::from(message.to_string()))
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
    use crate::{
        event::Event,
        mirror::{write_status, MirrorStatus},
    };
    use reqwest::{
        self,
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        multipart::{Form, Part},
    };
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
            file_keep_mode: crate::config::FileKeepMode::Referenced,
            max_blob_bytes_per_pubkey: None,
            owner_pubkeys: None,
            follow_pubkeys: None,
            pinned_event_ids: None,
            protect_pinned_from_deletes: true,
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
    async fn ready_endpoint_reports_ready_for_initialized_store() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/readyz", get(super::readyz))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/readyz", addr);
        let resp = reqwest::get(&url).await.unwrap();
        let status = resp.status();
        let body: super::Readiness = resp.json().await.unwrap();
        assert_eq!(status, reqwest::StatusCode::OK);
        assert_eq!(body.status, "ready");
        assert!(body.issues.is_empty());
        handle.abort();
    }

    #[tokio::test]
    async fn ready_endpoint_reports_missing_store_paths() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/readyz", get(super::readyz))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/readyz", addr);
        let resp = reqwest::get(&url).await.unwrap();
        let status = resp.status();
        let body: super::Readiness = resp.json().await.unwrap();
        assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body.status, "not-ready");
        assert!(body
            .issues
            .iter()
            .any(|issue| issue.contains("missing store path: events")));
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
    async fn retention_health_endpoint_reports_runtime_status() {
        let dir = TempDir::new().unwrap();
        let store = Store::with_limits(dir.path().to_path_buf(), false, Some(5), Some(1024));
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/retention-health", get(super::retention_health))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/retention-health", addr);
        let body: RetentionStatus = reqwest::get(&url).await.unwrap().json().await.unwrap();
        assert_eq!(body.state, "ok");
        assert_eq!(body.current_events, 0);
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
        assert_eq!(info.supported_nips, vec![9, 11, 12, 40, 45, 50, 94, 96, 98]);
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
    async fn nip96_info_endpoint_reports_file_policy() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let mut settings = test_settings(dir.path());
        settings.require_nip98_auth = true;
        settings.file_allowed_mime = Some(vec!["image/*".into(), "application/pdf".into()]);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/.well-known/nostr/nip96.json", get(super::nip96_info))
            .with_state(Arc::new(AppState { store, settings }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/.well-known/nostr/nip96.json", addr);
        let resp = reqwest::get(&url).await.unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let body: super::Nip96Info = resp.json().await.unwrap();
        assert_eq!(body.api_url, format!("http://{addr}/files"));
        assert_eq!(body.supported_nips, vec![98]);
        assert!(body.plans["free"].is_nip98_required);
        assert_eq!(
            body.content_types.unwrap(),
            vec!["application/pdf".to_string(), "image/*".to_string()]
        );
        handle.abort();
    }

    #[tokio::test]
    async fn file_api_upload_and_download_round_trip() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route(
                "/files",
                options(super::cors_preflight).post(super::upload_file),
            )
            .route("/files/:blob_id", get(super::download_file))
            .with_state(test_state(store, dir.path()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let client = reqwest::Client::new();
        let upload_url = format!("http://{addr}/files");
        let form = Form::new().part(
            "file",
            Part::bytes(b"hello world".to_vec())
                .file_name("hello.txt")
                .mime_str("text/plain")
                .unwrap(),
        );
        let response = client
            .post(&upload_url)
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::CREATED);
        let body: super::FileApiResponse = response.json().await.unwrap();
        let hash = body
            .nip94_event
            .unwrap()
            .tags
            .into_iter()
            .find(|tag| tag.first().map(String::as_str) == Some("x"))
            .and_then(|tag| tag.get(1).cloned())
            .unwrap();

        let download_url = format!("http://{addr}/files/{hash}");
        let response = client.get(&download_url).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .unwrap(),
            "text/plain"
        );
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"hello world");
        handle.abort();
    }

    #[tokio::test]
    async fn file_owner_routes_require_nip98_support() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let mut settings = test_settings(dir.path());
        settings.support_nip98 = false;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/files", get(super::list_files))
            .with_state(Arc::new(AppState { store, settings }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let response = reqwest::get(&format!("http://{addr}/files")).await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
        assert!(response
            .text()
            .await
            .unwrap()
            .contains("NIP-98 auth support disabled"));
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
    async fn query_endpoint_applies_max_limit_cap() {
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
        ] {
            store.ingest(&event).unwrap();
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.max_limit = Some(1);
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(Arc::new(AppState { store, settings }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/query?authors=p1", addr);
        let body = reqwest::get(&url).await.unwrap().text().await.unwrap();
        let lines: Vec<_> = body.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("bb22"));
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
    async fn query_endpoint_rate_limits_reads() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        store
            .ingest(&Event {
                id: "aa11".into(),
                pubkey: "p1".into(),
                kind: 1,
                created_at: 1,
                tags: vec![],
                content: String::new(),
                sig: String::new(),
            })
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.rate_limit_window_secs = Some(60);
        settings.max_queries_per_window = Some(1);
        let app = Router::new()
            .route("/query", get(super::query))
            .with_state(Arc::new(AppState { store, settings }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = task::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("http://{}/query?authors=p1", addr);
        let first = reqwest::get(&url).await.unwrap();
        assert_eq!(first.status(), reqwest::StatusCode::OK);
        let second = reqwest::get(&url).await.unwrap();
        assert_eq!(second.status(), reqwest::StatusCode::TOO_MANY_REQUESTS);
        assert!(second.text().await.unwrap().contains("rate limit exceeded"));
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
        assert!(
            super::serve_http(addr, store, settings, std::future::pending())
                .await
                .is_err()
        );
    }
}
