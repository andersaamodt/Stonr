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
    event::{Event, Tag},
    storage::{event_hash, Query, Store},
};

#[derive(Clone)]
struct AppState {
    store: Store,
    events_tx: broadcast::Sender<Event>,
}

/// Start a WebSocket server speaking a practical NIP-01 subset.
pub async fn serve_ws(
    addr: SocketAddr,
    store: Store,
    events_tx: broadcast::Sender<Event>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let app = Router::new()
        .route("/", get(handler))
        .with_state(Arc::new(AppState { store, events_tx }));
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

/// Handle the HTTP upgrade and spawn the connection processor.
async fn handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| async move { process(socket, state).await })
}

async fn process(socket: WebSocket, state: Arc<AppState>) {
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

    loop {
        tokio::select! {
            incoming = reader.next() => {
                match incoming {
                    Some(Ok(Message::Text(txt))) => handle_text(&txt, &state, &out_tx, &mut subs).await,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
            live = live_rx.recv() => {
                match live {
                    Ok(event) => fan_out_live_event(&event, &subs, &out_tx),
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
            let filters = arr.iter().skip(2).map(Query::from_value).collect::<Vec<_>>();
            let snapshot = snapshot_events(&state.store, &filters).unwrap_or_default();
            for event in snapshot {
                send_json(out_tx, serde_json::json!(["EVENT", sub, event]));
            }
            send_json(out_tx, serde_json::json!(["EOSE", sub]));
            subs.insert(sub, filters);
        }
        Some("COUNT") if arr.len() >= 3 => {
            let sub = arr.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let filters = arr.iter().skip(2).map(Query::from_value).collect::<Vec<_>>();
            let count = snapshot_events(&state.store, &filters).map(|events| events.len()).unwrap_or(0);
            send_json(out_tx, serde_json::json!(["COUNT", sub, {"count": count}]));
        }
        Some("CLOSE") if arr.len() >= 2 => {
            let sub = arr.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            subs.remove(&sub);
            send_json(out_tx, serde_json::json!(["CLOSED", sub, "closed"]));
        }
        Some("EVENT") if arr.len() >= 2 => {
            match serde_json::from_value::<Event>(arr[1].clone()) {
                Ok(event) => {
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
        _ => {}
    }
}

fn publish_event(state: &AppState, event: &Event) -> Result<()> {
    let calc_id = hex::encode(event_hash(event)?);
    if calc_id != event.id {
        anyhow::bail!("id mismatch");
    }
    state.store.ingest(event)?;
    let _ = state.events_tx.send(event.clone());
    Ok(())
}

fn fan_out_live_event(
    event: &Event,
    subs: &HashMap<String, Vec<Query>>,
    out_tx: &mpsc::UnboundedSender<String>,
) {
    for (sub, filters) in subs {
        if filters.iter().any(|filter| event_matches_query(event, filter)) {
            send_json(out_tx, serde_json::json!(["EVENT", sub, event]));
        }
    }
}

fn snapshot_events(store: &Store, filters: &[Query]) -> Result<Vec<Event>> {
    let mut seen = HashSet::new();
    let mut events = Vec::new();
    for filter in filters {
        for event in store.query(filter.clone())? {
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

    fn test_state(store: Store) -> Arc<AppState> {
        let (events_tx, _) = broadcast::channel(256);
        Arc::new(AppState { store, events_tx })
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

    #[test]
    fn from_value_fields() {
        let val = serde_json::json!({
            "authors": ["a1", "a2"],
            "kinds": [1, 2],
            "#d": ["slug"],
            "#t": ["tag"],
            "since": 1,
            "until": 2,
            "limit": 3
        });
        let q = Query::from_value(&val);
        assert_eq!(q.authors.unwrap(), vec!["a1".to_string(), "a2".to_string()]);
        assert_eq!(q.kinds.unwrap(), vec![1, 2]);
        assert_eq!(q.d.unwrap(), "slug");
        assert_eq!(q.t.unwrap(), "tag");
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
        let (events_tx, _) = broadcast::channel(256);
        let shutdown = tokio::time::sleep(std::time::Duration::from_millis(100));
        let handle = tokio::spawn(async move {
            super::serve_ws(addr, store_clone, events_tx, shutdown)
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
        let (events_tx, _) = broadcast::channel(256);
        assert!(super::serve_ws(addr, store, events_tx, std::future::pending())
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
}
