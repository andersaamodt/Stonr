use std::{fs, process::Command};

use serde_json::Value;
use tempfile::TempDir;

fn backend_script() -> String {
    format!(
        "{}/apps/stonr-control/scripts/stonr-control-backend.sh",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn write_env(dir: &TempDir) -> String {
    let env_path = dir.path().join("relay.env");
    let store_root = dir.path().join("store");
    fs::create_dir_all(store_root.join("log")).unwrap();
    fs::create_dir_all(store_root.join("events")).unwrap();
    fs::write(
        &env_path,
        format!(
            "STORE_ROOT={}\nBIND_HTTP=127.0.0.1:7777\nBIND_WS=127.0.0.1:7778\nVERIFY_SIG=0\n",
            store_root.display()
        ),
    )
    .unwrap();
    env_path.display().to_string()
}

fn run_backend(args: &[&str]) -> String {
    let output = Command::new("sh")
        .arg(backend_script())
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "backend failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn count_events_uses_event_log() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);
    let store_root = dir.path().join("store");
    fs::write(
        store_root.join("log/events.ndjson"),
        concat!(
            "{\"id\":\"a\",\"kind\":1,\"created_at\":1,\"content\":\"one\"}\n",
            "{\"id\":\"b\",\"kind\":1,\"created_at\":2,\"content\":\"two\"}\n",
            "{\"id\":\"c\",\"kind\":1,\"created_at\":3,\"content\":\"three\"}\n"
        ),
    )
    .unwrap();

    let output = run_backend(&["count-events", &env_path]);
    assert_eq!(output.trim(), "3");
}

#[test]
fn query_events_reads_recent_summaries_from_log() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);
    let store_root = dir.path().join("store");
    fs::write(
        store_root.join("log/events.ndjson"),
        concat!(
            "{\"id\":\"a\",\"pubkey\":\"p1\",\"kind\":1,\"created_at\":10,\"tags\":[],\"content\":\"older text\"}\n",
            "{\"id\":\"b\",\"pubkey\":\"p2\",\"kind\":1059,\"created_at\":20,\"tags\":[],\"content\":\"ciphertext\"}\n",
            "{\"id\":\"c\",\"pubkey\":\"p3\",\"kind\":1,\"created_at\":30,\"tags\":[],\"content\":\"latest text\"}\n"
        ),
    )
    .unwrap();

    let output = run_backend(&["query-events", &env_path, "", "2"]);
    let events: Vec<Value> = serde_json::from_str(&output).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["id"], "c");
    assert_eq!(events[1]["id"], "b");
    assert_eq!(events[1]["content"], "Encrypted message payload");
}

#[test]
fn query_events_searches_recent_matches_from_log() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);
    let store_root = dir.path().join("store");
    fs::write(
        store_root.join("log/events.ndjson"),
        concat!(
            "{\"id\":\"a\",\"pubkey\":\"p1\",\"kind\":1,\"created_at\":10,\"tags\":[],\"content\":\"alpha keyword\"}\n",
            "{\"id\":\"b\",\"pubkey\":\"p2\",\"kind\":1,\"created_at\":20,\"tags\":[],\"content\":\"beta\"}\n",
            "{\"id\":\"c\",\"pubkey\":\"p3\",\"kind\":1,\"created_at\":30,\"tags\":[],\"content\":\"gamma keyword\"}\n"
        ),
    )
    .unwrap();

    let output = run_backend(&["query-events", &env_path, "keyword", "5"]);
    let events: Vec<Value> = serde_json::from_str(&output).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["id"], "c");
    assert_eq!(events[1]["id"], "a");
}

#[test]
fn apply_retention_prunes_oldest_events() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);
    let store_root = dir.path().join("store");
    fs::write(
        &env_path,
        format!(
            "STORE_ROOT={}\nBIND_HTTP=127.0.0.1:7777\nBIND_WS=127.0.0.1:7778\nVERIFY_SIG=0\nMAX_STORED_EVENTS=2\n",
            store_root.display()
        ),
    )
    .unwrap();

    fs::create_dir_all(store_root.join("events/aa/11")).unwrap();
    fs::create_dir_all(store_root.join("events/bb/22")).unwrap();
    fs::create_dir_all(store_root.join("events/cc/33")).unwrap();
    fs::create_dir_all(store_root.join("index/by-author")).unwrap();
    fs::create_dir_all(store_root.join("mirror/authors/p1")).unwrap();
    fs::create_dir_all(store_root.join("latest")).unwrap();
    fs::write(
        store_root.join("events/aa/11/aa11.json"),
        r#"{"id":"aa11","pubkey":"p1","kind":1,"created_at":10,"tags":[],"content":"","sig":""}"#,
    )
    .unwrap();
    fs::write(
        store_root.join("events/bb/22/bb22.json"),
        r#"{"id":"bb22","pubkey":"p1","kind":1,"created_at":20,"tags":[],"content":"","sig":""}"#,
    )
    .unwrap();
    fs::write(
        store_root.join("events/cc/33/cc33.json"),
        r#"{"id":"cc33","pubkey":"p1","kind":1,"created_at":30,"tags":[],"content":"","sig":""}"#,
    )
    .unwrap();

    run_backend(&["apply-retention", &env_path]);

    assert!(!store_root.join("events/aa/11/aa11.json").exists());
    assert!(store_root.join("events/bb/22/bb22.json").exists());
    assert!(store_root.join("events/cc/33/cc33.json").exists());
}
