//! WebSocket relay server implementing a practical NIP-01 subset.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    net::SocketAddr,
    sync::Arc,
};

use anyhow::Result;
use axum::{
    extract::{
        ConnectInfo,
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};

use crate::{
    auth::{verify_auth_event, SessionAuth},
    config::Settings,
    event::{Event, Tag},
    policy::{
        apply_query_policy, client_actor_label, current_unix_ts, enforce_rate_limit,
        validate_event_with_files, RateLimitAction,
    },
    storage::{event_hash, Query, Store},
};

#[derive(Clone)]
struct AppState {
    store: Store,
    settings: Settings,
    events_tx: broadcast::Sender<Event>,
}

/// Start a WebSocket server speaking a practical NIP-01 subset.
pub async fn serve_ws(
    addr: SocketAddr,
    store: Store,
    settings: Settings,
    events_tx: broadcast::Sender<Event>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let app = Router::new()
        .route("/", get(handler))
        .with_state(Arc::new(AppState {
            store,
            settings,
            events_tx,
        }));
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

/// Handle the HTTP upgrade and spawn the connection processor.
async fn handler(
    ws: WebSocketUpgrade,
    connect: Option<ConnectInfo<SocketAddr>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let peer_addr = connect.map(|info| info.0);
    ws.on_upgrade(move |socket| async move { process(socket, state, peer_addr).await })
}

async fn process(socket: WebSocket, state: Arc<AppState>, peer_addr: Option<SocketAddr>) {
    let (mut writer, mut reader) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let writer_task = tokio::spawn(async move {
        while let Some(text) = out_rx.recv().await {
            if writer.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    });

    let mut subs: HashMap<String, Vec<Query>> = HashMap::new();
    let mut live_rx = state.events_tx.subscribe();
    let mut auth = SessionAuth::new();

    if state.settings.nip42_enabled() {
        send_json(&out_tx, serde_json::json!(["AUTH", auth.challenge()]));
    }

    loop {
        tokio::select! {
            incoming = reader.next() => {
                match incoming {
                    Some(Ok(Message::Text(txt))) => handle_text(&txt, &state, &out_tx, &mut subs, &mut auth, peer_addr).await,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
            live = live_rx.recv() => {
                match live {
                    Ok(event) => fan_out_live_event(
                        &state.store,
                        &state.settings,
                        &event,
                        &subs,
                        &out_tx,
                    ),
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    drop(out_tx);
    let _ = writer_task.await;
}

async fn handle_text(
    txt: &str,
    state: &Arc<AppState>,
    out_tx: &mpsc::UnboundedSender<String>,
    subs: &mut HashMap<String, Vec<Query>>,
    auth: &mut SessionAuth,
    peer_addr: Option<SocketAddr>,
) {
    let val = match serde_json::from_str::<Value>(txt) {
        Ok(val) => val,
        Err(_) => return,
    };
    let arr = match val.as_array() {
        Some(arr) => arr,
        None => return,
    };
    match arr.first().and_then(|v| v.as_str()) {
        Some("REQ") if arr.len() >= 3 => {
            let sub = arr.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let filters = arr
                .iter()
                .skip(2)
                .map(Query::from_value)
                .map(|query| apply_query_policy(&state.settings, query))
                .collect::<Vec<_>>();
            if !state.settings.query_enabled() {
                send_json(out_tx, serde_json::json!(["CLOSED", sub, "read access disabled"]));
                return;
            }
            if state.settings.query_auth_required() && !auth.is_authenticated() {
                if state.settings.nip42_enabled() {
                    send_json(out_tx, serde_json::json!(["AUTH", auth.challenge()]));
                }
                send_json(out_tx, serde_json::json!(["CLOSED", sub, "auth-required: relay login required"]));
                return;
            }
            if filters.iter().any(Query::has_tag_filters) && !state.settings.tag_queries_enabled() {
                send_json(out_tx, serde_json::json!(["CLOSED", sub, "tag queries disabled"]));
                return;
            }
            if filters.iter().any(|query| query.search.is_some()) && !state.settings.search_enabled() {
                send_json(out_tx, serde_json::json!(["CLOSED", sub, "text search disabled"]));
                return;
            }
            let actor = client_actor_label(peer_addr, auth.actor_pubkey());
            if let Err(error) = enforce_rate_limit(
                &state.settings,
                RateLimitAction::Query,
                &actor,
                current_unix_ts(),
            ) {
                send_json(out_tx, serde_json::json!(["CLOSED", sub, format!("rate-limited: {error}")]));
                return;
            }
            let snapshot = snapshot_events(&state.store, &state.settings, &filters).unwrap_or_default();
            for event in snapshot {
                send_json(out_tx, serde_json::json!(["EVENT", sub, event]));
            }
            send_json(out_tx, serde_json::json!(["EOSE", sub]));
            if state.settings.live_subscriptions_enabled() {
                subs.insert(sub, filters);
            }
        }
        Some("COUNT") if arr.len() >= 3 => {
            let sub = arr.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let filters = arr
                .iter()
                .skip(2)
                .map(Query::from_value)
                .map(|query| apply_query_policy(&state.settings, query))
                .collect::<Vec<_>>();
            if !state.settings.query_enabled() || !state.settings.count_enabled() {
                send_json(out_tx, serde_json::json!(["CLOSED", sub, "count queries disabled"]));
                return;
            }
            if state.settings.count_auth_required() && !auth.is_authenticated() {
                if state.settings.nip42_enabled() {
                    send_json(out_tx, serde_json::json!(["AUTH", auth.challenge()]));
                }
                send_json(out_tx, serde_json::json!(["CLOSED", sub, "auth-required: relay login required"]));
                return;
            }
            if filters.iter().any(Query::has_tag_filters) && !state.settings.tag_queries_enabled() {
                send_json(out_tx, serde_json::json!(["CLOSED", sub, "tag queries disabled"]));
                return;
            }
            if filters.iter().any(|query| query.search.is_some()) && !state.settings.search_enabled() {
                send_json(out_tx, serde_json::json!(["CLOSED", sub, "text search disabled"]));
                return;
            }
            let actor = client_actor_label(peer_addr, auth.actor_pubkey());
            if let Err(error) = enforce_rate_limit(
                &state.settings,
                RateLimitAction::Count,
                &actor,
                current_unix_ts(),
            ) {
                send_json(out_tx, serde_json::json!(["CLOSED", sub, format!("rate-limited: {error}")]));
                return;
            }
            let count = snapshot_events(&state.store, &state.settings, &filters)
                .map(|events| events.len())
                .unwrap_or(0);
            send_json(out_tx, serde_json::json!(["COUNT", sub, {"count": count}]));
        }
        Some("CLOSE") if arr.len() >= 2 => {
            let sub = arr.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            subs.remove(&sub);
            send_json(out_tx, serde_json::json!(["CLOSED", sub, "closed"]));
        }
        Some("EVENT") if arr.len() >= 2 => {
            if !state.settings.publish_enabled() {
                let event_id = arr
                    .get(1)
                    .and_then(|value| value.get("id"))
                    .and_then(|value| value.as_str())
                    .unwrap_or_default();
                send_json(out_tx, serde_json::json!(["OK", event_id, false, "publish disabled"]));
                return;
            }
            match serde_json::from_value::<Event>(arr[1].clone()) {
                Ok(event) => {
                    let actor = client_actor_label(peer_addr, auth.actor_pubkey());
                    if state.settings.publish_auth_required() && !auth.is_authenticated() {
                        if state.settings.nip42_enabled() {
                            send_json(out_tx, serde_json::json!(["AUTH", auth.challenge()]));
                        }
                        send_json(out_tx, serde_json::json!(["OK", event.id, false, "auth-required: relay login required"]));
                        return;
                    }
                    if state.settings.nip42_enabled()
                        && state.settings.auth_must_match_event_pubkey
                        && !auth.contains_pubkey(&event.pubkey)
                    {
                        let msg = if auth.is_authenticated() {
                            "restricted: authenticated pubkey does not match event pubkey"
                        } else {
                            "auth-required: relay login required"
                        };
                        if !auth.is_authenticated() {
                            send_json(out_tx, serde_json::json!(["AUTH", auth.challenge()]));
                        }
                        send_json(out_tx, serde_json::json!(["OK", event.id, false, msg]));
                        return;
                    }
                    if let Err(error) = enforce_rate_limit(
                        &state.settings,
                        RateLimitAction::Publish,
                        &actor,
                        current_unix_ts(),
                    ) {
                        send_json(out_tx, serde_json::json!(["OK", event.id, false, format!("rate-limited: {error}")]));
                        return;
                    }
                    let result = publish_event(state, &event);
                    match result {
                        Ok(()) => send_json(out_tx, serde_json::json!(["OK", event.id, true, "stored"])),
                        Err(error) => send_json(out_tx, serde_json::json!(["OK", event.id, false, format!("error: {error}")])),
                    }
                }
                Err(error) => {
                    send_json(out_tx, serde_json::json!(["NOTICE", format!("invalid EVENT payload: {error}")]));
                }
            }
        }
        Some("AUTH") if arr.len() >= 2 => {
            match serde_json::from_value::<Event>(arr[1].clone()) {
                Ok(event) => {
                    if !state.settings.nip42_enabled() {
                        send_json(
                            out_tx,
                            serde_json::json!(["OK", event.id, false, "restricted: relay authentication disabled"]),
                        );
                        return;
                    }
                    match verify_auth_event(
                        &event,
                        auth.challenge(),
                        &state.settings.bind_ws,
                        state.settings.auth_max_age_secs,
                        current_unix_ts(),
                    ) {
                        Ok(()) => {
                            auth.authenticate(event.pubkey.clone());
                            send_json(out_tx, serde_json::json!(["OK", event.id, true, "authenticated"]));
                        }
                        Err(error) => {
                            send_json(out_tx, serde_json::json!(["OK", event.id, false, format!("invalid: {error}")]));
                        }
                    }
                }
                Err(error) => {
                    send_json(out_tx, serde_json::json!(["NOTICE", format!("invalid AUTH payload: {error}")]));
                }
            }
        }
        _ => {}
    }
}

fn publish_event(state: &AppState, event: &Event) -> Result<()> {
    let calc_id = hex::encode(event_hash(event)?);
    if calc_id != event.id {
        anyhow::bail!("id mismatch");
    }
    validate_event_with_files(&state.settings, &state.store.files(), event, current_unix_ts())?;
    if state.store.ingest_with_policy(
        event,
        state.settings.delete_enabled(),
        state.settings.expiration_enabled(),
    )? {
        state.store.files().add_event_references(event)?;
        let _ = state.events_tx.send(event.clone());
    }
    Ok(())
}

fn fan_out_live_event(
    store: &Store,
    settings: &Settings,
    event: &Event,
    subs: &HashMap<String, Vec<Query>>,
    out_tx: &mpsc::UnboundedSender<String>,
) {
    let visible = store
        .event_visible_with_policy(
            event,
            settings.delete_enabled(),
            settings.expiration_enabled(),
        )
        .unwrap_or(false);
    if !visible {
        return;
    }
    for (sub, filters) in subs {
        if filters.iter().any(|filter| event_matches_query(event, filter)) {
            send_json(out_tx, serde_json::json!(["EVENT", sub, event]));
        }
    }
}

fn snapshot_events(store: &Store, settings: &Settings, filters: &[Query]) -> Result<Vec<Event>> {
    let mut seen = HashSet::new();
    let mut events = Vec::new();
    for filter in filters {
        for event in store.query_with_policy(
            filter.clone(),
            settings.delete_enabled(),
            settings.expiration_enabled(),
        )? {
            if seen.insert(event.id.clone()) {
                events.push(event);
            }
        }
    }
    events.sort_by_key(|event| std::cmp::Reverse(event.created_at));
    Ok(events)
}

fn event_matches_query(event: &Event, query: &Query) -> bool {
    if let Some(authors) = &query.authors {
        if !authors.iter().any(|author| author == &event.pubkey) {
            return false;
        }
    }
    if let Some(kinds) = &query.kinds {
        if !kinds.contains(&event.kind) {
            return false;
        }
    }
    if let Some(since) = query.since {
        if event.created_at < since {
            return false;
        }
    }
    if let Some(until) = query.until {
        if event.created_at > until {
            return false;
        }
    }
    if let Some(search) = &query.search {
        if !event.content.to_lowercase().contains(&search.to_lowercase()) {
            return false;
        }
    }
    if let Some(d) = &query.d {
        if !event.tags.iter().any(|Tag(fields)| matches!(fields.as_slice(), [tag, value, ..] if tag == "d" && value == d)) {
            return false;
        }
    }
    if let Some(t) = &query.t {
        if !event.tags.iter().any(|Tag(fields)| matches!(fields.as_slice(), [tag, value, ..] if tag == "t" && value == t)) {
            return false;
        }
    }
    for (tag, values) in &query.tags {
        let matched = event.tags.iter().any(|Tag(fields)| {
            matches!(fields.as_slice(), [event_tag, value, ..] if event_tag == tag && values.iter().any(|candidate| candidate == value))
        });
        if !matched {
            return false;
        }
    }
    true
}

fn send_json(out_tx: &mpsc::UnboundedSender<String>, value: Value) {
    let _ = out_tx.send(value.to_string());
}

#[cfg(test)]
#[allow(clippy::single_match)]
mod tests {
    use super::*;
    use crate::event::{Event, Tag};
    use futures_util::{SinkExt, StreamExt};
    use tempfile::TempDir;
    use tokio_tungstenite::tungstenite::protocol::Message as TungMessage;

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
        }
    }

    fn test_state(store: Store) -> Arc<AppState> {
        let (events_tx, _) = broadcast::channel(256);
        let settings = test_settings(std::path::Path::new("/tmp"));
        Arc::new(AppState {
            store,
            settings,
            events_tx,
        })
    }

    fn hashed_event(
        pubkey: &str,
        kind: u32,
        created_at: u64,
        tags: Vec<Tag>,
        content: &str,
    ) -> Event {
        let mut event = Event {
            id: String::new(),
            pubkey: pubkey.into(),
            kind,
            created_at,
            tags,
            content: content.into(),
            sig: String::new(),
        };
        event.id = hex::encode(event_hash(&event).unwrap());
        event
    }

    fn signed_auth_event(bind_ws: &str, challenge: &str, pubkey_bytes: [u8; 32]) -> Event {
        use secp256k1::{Keypair, Message, Secp256k1};

        let secp = Secp256k1::new();
        let kp = Keypair::from_seckey_slice(&secp, &pubkey_bytes).unwrap();
        let pubkey = hex::encode(kp.x_only_public_key().0.serialize());
        let mut event = Event {
            id: String::new(),
            pubkey,
            kind: 22242,
            created_at: current_unix_ts(),
            tags: vec![
                Tag(vec!["relay".into(), format!("ws://{bind_ws}/")]),
                Tag(vec!["challenge".into(), challenge.into()]),
            ],
            content: String::new(),
            sig: String::new(),
        };
        event.id = hex::encode(event_hash(&event).unwrap());
        let hash = hex::decode(&event.id).unwrap();
        let msg = Message::from_digest_slice(&hash).unwrap();
        let sig = secp.sign_schnorr_no_aux_rand(&msg, &kp);
        event.sig = hex::encode(sig.as_ref());
        event
    }

    async fn next_text(ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>) -> String {
        loop {
            match ws.next().await.unwrap().unwrap() {
                TungMessage::Text(text) => return text,
                _ => {}
            }
        }
    }

    #[test]
    fn from_value_fields() {
        let val = serde_json::json!({
            "authors": ["a1", "a2"],
            "kinds": [1, 2],
            "#d": ["slug"],
            "#t": ["tag"],
            "search": "hello",
            "since": 1,
            "until": 2,
            "limit": 3
        });
        let q = Query::from_value(&val);
        assert_eq!(q.authors.unwrap(), vec!["a1".to_string(), "a2".to_string()]);
        assert_eq!(q.kinds.unwrap(), vec![1, 2]);
        assert_eq!(q.d.unwrap(), "slug");
        assert_eq!(q.t.unwrap(), "tag");
        assert_eq!(q.search.unwrap(), "hello");
        assert_eq!(q.since, Some(1));
        assert_eq!(q.until, Some(2));
        assert_eq!(q.limit, Some(3));
    }

    #[test]
    fn from_value_defaults() {
        let q = Query::from_value(&serde_json::json!({}));
        assert!(q.authors.is_none());
        assert!(q.kinds.is_none());
        assert!(q.d.is_none());
        assert!(q.t.is_none());
        assert!(q.search.is_none());
        assert!(q.since.is_none());
        assert!(q.until.is_none());
        assert!(q.limit.is_none());
    }

    #[tokio::test]
    async fn ws_round_trip() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev = Event {
            id: "aa11".into(),
            pubkey: "p1".into(),
            kind: 1,
            created_at: 1,
            tags: vec![Tag(vec!["d".into(), "slug".into()])],
            content: String::new(),
            sig: String::new(),
        };
        store.ingest(&ev).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/", get(handler))
            .with_state(test_state(store));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req_msg = serde_json::json!([
            "REQ",
            "sub",
            {
                "authors": ["p1"],
                "kinds": [1],
                "#d": ["slug"],
            }
        ]);
        ws_stream
            .send(TungMessage::Text(req_msg.to_string()))
            .await
            .unwrap();

        let mut got_event = false;
        while let Some(msg) = ws_stream.next().await {
            match msg.unwrap() {
                TungMessage::Text(t) => {
                    if t.contains("EVENT") {
                        got_event = true;
                    }
                    if t.contains("EOSE") {
                        break;
                    }
                }
                _ => {}
            }
        }
        assert!(got_event);
        handle.abort();
    }

    #[tokio::test]
    async fn ws_limit_and_since() {
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
                pubkey: "p1".into(),
                kind: 1,
                created_at: 3,
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
            .route("/", get(handler))
            .with_state(test_state(store));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req_msg = serde_json::json!([
            "REQ",
            "sub",
            {
                "authors": ["p1"],
                "kinds": [1],
                "since": 2,
                "limit": 1
            }
        ]);
        ws_stream
            .send(TungMessage::Text(req_msg.to_string()))
            .await
            .unwrap();

        let mut events = vec![];
        while let Some(msg) = ws_stream.next().await {
            match msg.unwrap() {
                TungMessage::Text(t) => {
                    if t.contains("EVENT") {
                        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
                        let ev_id = v[2]["id"].as_str().unwrap().to_string();
                        events.push(ev_id);
                    }
                    if t.contains("EOSE") {
                        break;
                    }
                }
                _ => {}
            }
        }
        assert_eq!(events, vec!["cc33".to_string()]);
        handle.abort();
    }

    #[tokio::test]
    async fn ws_tag_filter() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let ev1 = Event {
            id: "aa11".into(),
            pubkey: "p1".into(),
            kind: 1,
            created_at: 1,
            tags: vec![Tag(vec!["t".into(), "tag1".into()])],
            content: String::new(),
            sig: String::new(),
        };
        let ev2 = Event {
            id: "bb22".into(),
            pubkey: "p1".into(),
            kind: 1,
            created_at: 2,
            tags: vec![Tag(vec!["t".into(), "tag2".into()])],
            content: String::new(),
            sig: String::new(),
        };
        store.ingest(&ev1).unwrap();
        store.ingest(&ev2).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/", get(handler))
            .with_state(test_state(store));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });
        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req_msg = serde_json::json!([
            "REQ",
            "sub",
            {"#t": ["tag1"]}
        ]);
        ws_stream
            .send(TungMessage::Text(req_msg.to_string()))
            .await
            .unwrap();
        let mut events = vec![];
        while let Some(msg) = ws_stream.next().await {
            match msg.unwrap() {
                TungMessage::Text(t) => {
                    if t.contains("EVENT") {
                        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
                        events.push(v[2]["id"].as_str().unwrap().to_string());
                    }
                    if t.contains("EOSE") {
                        break;
                    }
                }
                _ => {}
            }
        }
        assert_eq!(events, vec!["aa11".to_string()]);
        handle.abort();
    }

    #[tokio::test]
    async fn ws_close_then_req() {
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
            .route("/", get(handler))
            .with_state(test_state(store));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });
        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        ws_stream
            .send(TungMessage::Text("[\"CLOSE\",\"s\"]".into()))
            .await
            .unwrap();
        let req_msg = serde_json::json!(["REQ", "s", {"authors": ["p1"], "kinds": [1]}]);
        ws_stream
            .send(TungMessage::Text(req_msg.to_string()))
            .await
            .unwrap();
        let mut got_event = false;
        while let Some(msg) = ws_stream.next().await {
            match msg.unwrap() {
                TungMessage::Text(t) => {
                    if t.contains("EVENT") {
                        got_event = true;
                    }
                    if t.contains("EOSE") {
                        break;
                    }
                }
                _ => {}
            }
        }
        assert!(got_event);
        handle.abort();
    }

    #[tokio::test]
    async fn ws_replaceable_returns_latest() {
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
            .route("/", get(handler))
            .with_state(test_state(store));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });
        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req = serde_json::json!([
            "REQ",
            "s",
            {"authors": ["p1"], "kinds": [30023], "#d": ["slug"]}
        ]);
        ws_stream
            .send(TungMessage::Text(req.to_string()))
            .await
            .unwrap();
        let mut events = vec![];
        while let Some(msg) = ws_stream.next().await {
            match msg.unwrap() {
                TungMessage::Text(t) => {
                    if t.contains("EVENT") {
                        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
                        events.push(v[2]["id"].as_str().unwrap().to_string());
                    }
                    if t.contains("EOSE") {
                        break;
                    }
                }
                _ => {}
            }
        }
        assert_eq!(events, vec!["bb22".to_string()]);
        handle.abort();
    }

    #[tokio::test]
    async fn ws_limit_zero_returns_eose() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/", get(handler))
            .with_state(test_state(store));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });
        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req = serde_json::json!(["REQ", "s", {"limit": 0}]);
        ws_stream
            .send(TungMessage::Text(req.to_string()))
            .await
            .unwrap();
        let mut saw_event = false;
        let mut saw_eose = false;
        while let Some(msg) = ws_stream.next().await {
            match msg.unwrap() {
                TungMessage::Text(t) => {
                    if t.contains("EVENT") {
                        saw_event = true;
                    }
                    if t.contains("EOSE") {
                        saw_eose = true;
                        break;
                    }
                }
                _ => {}
            }
        }
        assert!(!saw_event);
        assert!(saw_eose);
        handle.abort();
    }

    #[tokio::test]
    async fn ws_malformed_messages_are_ignored() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/", get(handler))
            .with_state(test_state(store));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });
        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        ws_stream
            .send(TungMessage::Text("not json".into()))
            .await
            .unwrap();
        ws_stream
            .send(TungMessage::Text("{}".into()))
            .await
            .unwrap();
        let req = serde_json::json!(["REQ", "s", {"authors": ["p1"], "kinds": [1]}]);
        ws_stream
            .send(TungMessage::Text(req.to_string()))
            .await
            .unwrap();
        let mut saw_eose = false;
        while let Some(msg) = ws_stream.next().await {
            match msg.unwrap() {
                TungMessage::Text(t) => {
                    if t.contains("EOSE") {
                        saw_eose = true;
                        break;
                    }
                }
                _ => {}
            }
        }
        assert!(saw_eose);
        ws_stream
            .send(TungMessage::Text("[\"CLOSE\",\"s\"]".into()))
            .await
            .unwrap();
        handle.abort();
    }

    #[tokio::test]
    async fn ws_req_no_matches_returns_only_eose() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/", get(handler))
            .with_state(test_state(store));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });
        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req = serde_json::json!(["REQ", "s", {"authors": ["p"], "kinds": [1]}]);
        ws_stream
            .send(TungMessage::Text(req.to_string()))
            .await
            .unwrap();
        let mut saw_event = false;
        let mut saw_eose = false;
        while let Some(msg) = ws_stream.next().await {
            match msg.unwrap() {
                TungMessage::Text(t) => {
                    if t.contains("EVENT") {
                        saw_event = true;
                    }
                    if t.contains("EOSE") {
                        saw_eose = true;
                        break;
                    }
                }
                _ => {}
            }
        }
        assert!(!saw_event);
        assert!(saw_eose);
        handle.abort();
    }

    #[tokio::test]
    async fn serve_ws_serves_connections() {
        use tokio_tungstenite::tungstenite::Message as TungMessage;
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let store_clone = store.clone();
        let settings = test_settings(dir.path());
        let (events_tx, _) = broadcast::channel(256);
        let shutdown = tokio::time::sleep(std::time::Duration::from_millis(100));
        let handle = tokio::spawn(async move {
            super::serve_ws(addr, store_clone, settings, events_tx, shutdown)
                .await
                .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req = serde_json::json!(["REQ", "s", {"limit": 0}]);
        ws_stream
            .send(TungMessage::Text(req.to_string()))
            .await
            .unwrap();
        let mut saw_eose = false;
        while let Some(msg) = ws_stream.next().await {
            if let TungMessage::Text(t) = msg.unwrap() {
                if t.contains("EOSE") {
                    saw_eose = true;
                    break;
                }
            }
        }
        assert!(saw_eose);
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn serve_ws_bind_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        let settings = test_settings(dir.path());
        let (events_tx, _) = broadcast::channel(256);
        assert!(super::serve_ws(addr, store, settings, events_tx, std::future::pending())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn ws_count_returns_match_count() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        store.ingest(&hashed_event("p1", 1, 1, vec![], "a")).unwrap();
        store.ingest(&hashed_event("p1", 1, 2, vec![], "b")).unwrap();
        store.ingest(&hashed_event("p2", 1, 3, vec![], "c")).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/", get(handler))
            .with_state(test_state(store));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req = serde_json::json!(["COUNT", "c", {"authors": ["p1"], "kinds": [1]}]);
        ws_stream
            .send(TungMessage::Text(req.to_string()))
            .await
            .unwrap();

        let mut got_count = None;
        while let Some(msg) = ws_stream.next().await {
            if let TungMessage::Text(text) = msg.unwrap() {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value[0] == "COUNT" {
                    got_count = value[2]["count"].as_u64();
                    break;
                }
            }
        }
        assert_eq!(got_count, Some(2));
        handle.abort();
    }

    #[tokio::test]
    async fn ws_event_publish_and_live_subscriptions_work() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/", get(handler))
            .with_state(test_state(store.clone()));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("ws://{}/", addr);
        let (mut subscriber, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        let (mut publisher, _) = tokio_tungstenite::connect_async(url).await.unwrap();

        let req = serde_json::json!(["REQ", "live", {"authors": ["p9"], "kinds": [1]}]);
        subscriber
            .send(TungMessage::Text(req.to_string()))
            .await
            .unwrap();

        while let Some(msg) = subscriber.next().await {
            if let TungMessage::Text(text) = msg.unwrap() {
                if text.contains("\"EOSE\"") {
                    break;
                }
            }
        }

        let event = hashed_event("p9", 1, 42, vec![], "hello world");
        let publish = serde_json::json!(["EVENT", event]);
        publisher
            .send(TungMessage::Text(publish.to_string()))
            .await
            .unwrap();

        let mut saw_ok = false;
        while let Some(msg) = publisher.next().await {
            if let TungMessage::Text(text) = msg.unwrap() {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value[0] == "OK" {
                    saw_ok = value[2].as_bool().unwrap_or(false);
                    break;
                }
            }
        }
        assert!(saw_ok);

        let mut saw_live_event = false;
        while let Some(msg) = subscriber.next().await {
            if let TungMessage::Text(text) = msg.unwrap() {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value[0] == "EVENT" {
                    assert_eq!(value[2]["id"], event.id);
                    saw_live_event = true;
                    break;
                }
            }
        }
        assert!(saw_live_event);
        assert!(dir.path().join(format!("events/{}/{}/{}.json", &event.id[0..2], &event.id[2..4], event.id)).exists());
        handle.abort();
    }

    #[tokio::test]
    async fn ws_req_respects_disabled_query() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.enable_query = false;
        let (events_tx, _) = broadcast::channel(256);
        let app = Router::new()
            .route("/", get(handler))
            .with_state(Arc::new(AppState {
                store,
                settings,
                events_tx,
            }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req = serde_json::json!(["REQ", "sub", {"authors": ["p1"]}]);
        ws_stream
            .send(TungMessage::Text(req.to_string()))
            .await
            .unwrap();
        let msg = ws_stream.next().await.unwrap().unwrap();
        let text = match msg {
            TungMessage::Text(text) => text,
            other => panic!("expected text message, got {other:?}"),
        };
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[0], "CLOSED");
        assert_eq!(value[2], "read access disabled");
        handle.abort();
    }

    #[tokio::test]
    async fn ws_count_respects_disabled_count() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.enable_count = false;
        let (events_tx, _) = broadcast::channel(256);
        let app = Router::new()
            .route("/", get(handler))
            .with_state(Arc::new(AppState {
                store,
                settings,
                events_tx,
            }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req = serde_json::json!(["COUNT", "sub", {"authors": ["p1"]}]);
        ws_stream
            .send(TungMessage::Text(req.to_string()))
            .await
            .unwrap();
        let msg = ws_stream.next().await.unwrap().unwrap();
        let text = match msg {
            TungMessage::Text(text) => text,
            other => panic!("expected text message, got {other:?}"),
        };
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[0], "CLOSED");
        assert_eq!(value[2], "count queries disabled");
        handle.abort();
    }

    #[tokio::test]
    async fn ws_event_respects_disabled_publish() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.enable_publish = false;
        let (events_tx, _) = broadcast::channel(256);
        let app = Router::new()
            .route("/", get(handler))
            .with_state(Arc::new(AppState {
                store,
                settings,
                events_tx,
            }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let event = hashed_event("p9", 1, 42, vec![], "hello world");
        let publish = serde_json::json!(["EVENT", event]);
        ws_stream
            .send(TungMessage::Text(publish.to_string()))
            .await
            .unwrap();
        let msg = ws_stream.next().await.unwrap().unwrap();
        let text = match msg {
            TungMessage::Text(text) => text,
            other => panic!("expected text message, got {other:?}"),
        };
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[0], "OK");
        assert_eq!(value[2], false);
        assert_eq!(value[3], "publish disabled");
        handle.abort();
    }

    #[tokio::test]
    async fn ws_event_rejects_blocked_author() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.blocked_pubkeys = Some(vec!["p9".into()]);
        let (events_tx, _) = broadcast::channel(256);
        let app = Router::new()
            .route("/", get(handler))
            .with_state(Arc::new(AppState {
                store,
                settings,
                events_tx,
            }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move {
            server.await.unwrap();
        });

        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let event = hashed_event("p9", 1, 42, vec![], "hello world");
        let publish = serde_json::json!(["EVENT", event]);
        ws_stream
            .send(TungMessage::Text(publish.to_string()))
            .await
            .unwrap();
        let text = next_text(&mut ws_stream).await;
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[0], "OK");
        assert_eq!(value[2], false);
        assert!(value[3].as_str().unwrap().contains("author is blocked"));
        handle.abort();
    }

    #[tokio::test]
    async fn ws_sends_auth_challenge_when_nip42_enabled() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.enable_nip42 = true;
        settings.bind_ws = addr.to_string();
        let (events_tx, _) = broadcast::channel(256);
        let app = Router::new()
            .route("/", get(handler))
            .with_state(Arc::new(AppState { store, settings, events_tx }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move { server.await.unwrap(); });

        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let text = next_text(&mut ws_stream).await;
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[0], "AUTH");
        assert!(value[1].as_str().unwrap().len() >= 16);
        handle.abort();
    }

    #[tokio::test]
    async fn ws_req_requires_auth_when_enabled() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.enable_nip42 = true;
        settings.require_auth_for_query = true;
        settings.bind_ws = addr.to_string();
        let (events_tx, _) = broadcast::channel(256);
        let app = Router::new()
            .route("/", get(handler))
            .with_state(Arc::new(AppState { store, settings, events_tx }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move { server.await.unwrap(); });

        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let _ = next_text(&mut ws_stream).await;
        let req = serde_json::json!(["REQ", "sub", {"limit": 1}]);
        ws_stream.send(TungMessage::Text(req.to_string())).await.unwrap();

        let mut saw_closed = None;
        for _ in 0..3 {
            let text = next_text(&mut ws_stream).await;
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if value[0] == "CLOSED" {
                saw_closed = Some(value);
                break;
            }
        }
        let closed = saw_closed.expect("expected CLOSED after unauthenticated REQ");
        assert_eq!(closed[2], "auth-required: relay login required");
        handle.abort();
    }

    #[tokio::test]
    async fn ws_req_rate_limits_reads() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        store
            .ingest(&hashed_event("p1", 1, 1, vec![], "hello"))
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.rate_limit_window_secs = Some(60);
        settings.max_queries_per_window = Some(1);
        let (events_tx, _) = broadcast::channel(256);
        let app = Router::new()
            .route("/", get(handler))
            .with_state(Arc::new(AppState { store, settings, events_tx }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move { server.await.unwrap(); });

        let url = format!("ws://{}/", addr);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let req = serde_json::json!(["REQ", "sub1", {"authors": ["p1"], "kinds": [1]}]);
        ws_stream.send(TungMessage::Text(req.to_string())).await.unwrap();
        let _ = next_text(&mut ws_stream).await;
        let _ = next_text(&mut ws_stream).await;

        let req = serde_json::json!(["REQ", "sub2", {"authors": ["p1"], "kinds": [1]}]);
        ws_stream.send(TungMessage::Text(req.to_string())).await.unwrap();
        let text = next_text(&mut ws_stream).await;
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[0], "CLOSED");
        assert!(value[2].as_str().unwrap().contains("rate-limited"));
        handle.abort();
    }

    #[tokio::test]
    async fn ws_auth_then_req_succeeds() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        store
            .ingest(&hashed_event("p1", 1, 1, vec![], "hello"))
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.enable_nip42 = true;
        settings.require_auth_for_query = true;
        settings.bind_ws = addr.to_string();
        let (events_tx, _) = broadcast::channel(256);
        let app = Router::new()
            .route("/", get(handler))
            .with_state(Arc::new(AppState { store, settings, events_tx }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move { server.await.unwrap(); });

        let bind_ws = addr.to_string();
        let url = format!("ws://{}/", bind_ws);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let auth_prompt = next_text(&mut ws_stream).await;
        let auth_prompt_value: serde_json::Value = serde_json::from_str(&auth_prompt).unwrap();
        let challenge = auth_prompt_value[1].as_str().unwrap().to_string();
        let auth_event = signed_auth_event(&bind_ws, &challenge, [9u8; 32]);
        let auth_msg = serde_json::json!(["AUTH", auth_event]);
        ws_stream.send(TungMessage::Text(auth_msg.to_string())).await.unwrap();
        let ok_text = next_text(&mut ws_stream).await;
        let ok_value: serde_json::Value = serde_json::from_str(&ok_text).unwrap();
        assert_eq!(ok_value[0], "OK");
        assert_eq!(ok_value[2], true);

        let req = serde_json::json!(["REQ", "sub", {"authors": ["p1"], "kinds": [1]}]);
        ws_stream.send(TungMessage::Text(req.to_string())).await.unwrap();

        let mut saw_event = false;
        let mut saw_eose = false;
        for _ in 0..4 {
            let text = next_text(&mut ws_stream).await;
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if value[0] == "EVENT" {
                saw_event = true;
            }
            if value[0] == "EOSE" {
                saw_eose = true;
                break;
            }
        }
        assert!(saw_event);
        assert!(saw_eose);
        handle.abort();
    }

    #[tokio::test]
    async fn ws_publish_can_require_matching_authenticated_pubkey() {
        let dir = TempDir::new().unwrap();
        let store = Store::new(dir.path().to_path_buf(), false);
        store.init().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut settings = test_settings(dir.path());
        settings.enable_nip42 = true;
        settings.auth_must_match_event_pubkey = true;
        settings.bind_ws = addr.to_string();
        let (events_tx, _) = broadcast::channel(256);
        let app = Router::new()
            .route("/", get(handler))
            .with_state(Arc::new(AppState { store, settings, events_tx }));
        let server = axum::serve(listener, app.into_make_service());
        let handle = tokio::spawn(async move { server.await.unwrap(); });

        let bind_ws = addr.to_string();
        let url = format!("ws://{}/", bind_ws);
        let (mut ws_stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let auth_prompt = next_text(&mut ws_stream).await;
        let auth_prompt_value: serde_json::Value = serde_json::from_str(&auth_prompt).unwrap();
        let challenge = auth_prompt_value[1].as_str().unwrap().to_string();
        let auth_event = signed_auth_event(&bind_ws, &challenge, [9u8; 32]);
        let auth_msg = serde_json::json!(["AUTH", auth_event]);
        ws_stream.send(TungMessage::Text(auth_msg.to_string())).await.unwrap();
        let _ = next_text(&mut ws_stream).await;

        let event = hashed_event("other-pubkey", 1, 42, vec![], "hello world");
        let publish = serde_json::json!(["EVENT", event]);
        ws_stream.send(TungMessage::Text(publish.to_string())).await.unwrap();
        let text = next_text(&mut ws_stream).await;
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[0], "OK");
        assert_eq!(value[2], false);
        assert_eq!(value[3], "restricted: authenticated pubkey does not match event pubkey");
        handle.abort();
    }
}
