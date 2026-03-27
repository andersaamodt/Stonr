use std::{fs, process::Command};
use std::{thread, time::Duration};

use serde_json::Value;
use tempfile::TempDir;

fn backend_script() -> String {
    format!(
        "{}/app/scripts/stonr-control-backend.sh",
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
            "STORE_ROOT={}\nBIND_HTTP=127.0.0.1:7777\nBIND_WS=127.0.0.1:7778\nVERIFY_SIG=0\nFILTER_PRIVATE_MESSAGES=1\n",
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
fn retention_status_returns_structured_json() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);

    let output = run_backend(&["retention-status", &env_path]);
    let body: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(body["state"], "disabled");
    assert_eq!(body["current_events"], 0);
}

#[test]
fn mirror_status_returns_json_array() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);

    let output = run_backend(&["mirror-status", &env_path]);
    let body: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(body, Value::Array(vec![]));
}

#[test]
fn tail_log_prefixes_timestamp_on_each_line() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);
    let store_root = dir.path().join("store");
    fs::create_dir_all(store_root.join("runtime")).unwrap();
    fs::write(
        store_root.join("runtime/relay.log"),
        concat!(
            "storage warning: skipping unreadable event file /tmp/a: bad UTF-8\n",
            "{\"ts\":1711111111,\"level\":\"warn\",\"component\":\"retention\",\"message\":\"failed to enforce retention\"}\n"
        ),
    )
    .unwrap();

    let output = run_backend(&["tail-log", &env_path, "20"]);
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with('['));
    assert!(
        lines[0].contains("] storage warning: skipping unreadable event file /tmp/a: bad UTF-8")
    );
    assert!(lines[1].starts_with('['));
    assert!(lines[1].contains("] {\"ts\":1711111111,"));
}

#[test]
fn count_events_prefers_runtime_cache() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);
    let store_root = dir.path().join("store");
    fs::create_dir_all(store_root.join("runtime")).unwrap();
    fs::write(store_root.join("runtime/events-count.cache"), "224180\n").unwrap();

    let output = run_backend(&["count-events", &env_path]);
    assert_eq!(output.trim(), "224180");
}

#[test]
fn size_events_prefers_runtime_cache() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);
    let store_root = dir.path().join("store");
    fs::create_dir_all(store_root.join("runtime")).unwrap();
    fs::write(
        store_root.join("runtime/events-bytes.cache"),
        "1258291200\n",
    )
    .unwrap();

    let output = run_backend(&["size-events", &env_path]);
    assert_eq!(output.trim(), "1258291200");
}

#[test]
fn query_events_hides_private_messages_when_filter_enabled() {
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
    assert_eq!(events[1]["id"], "a");
}

#[test]
fn query_events_can_show_private_messages_when_filter_disabled() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);
    let store_root = dir.path().join("store");
    fs::write(
        &env_path,
        format!(
            "STORE_ROOT={}\nBIND_HTTP=127.0.0.1:7777\nBIND_WS=127.0.0.1:7778\nVERIFY_SIG=0\nFILTER_PRIVATE_MESSAGES=0\n",
            store_root.display()
        ),
    )
    .unwrap();
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
fn query_events_prefers_event_log_when_running_pid_exists() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);
    let store_root = dir.path().join("store");
    fs::create_dir_all(store_root.join("runtime")).unwrap();
    fs::write(
        store_root.join("runtime/relay.pid"),
        format!("{}\n", std::process::id()),
    )
    .unwrap();
    fs::write(
        store_root.join("log/events.ndjson"),
        concat!(
            "{\"id\":\"a\",\"pubkey\":\"p1\",\"kind\":1,\"created_at\":10,\"tags\":[],\"content\":\"older\"}\n",
            "{\"id\":\"b\",\"pubkey\":\"p2\",\"kind\":1,\"created_at\":20,\"tags\":[],\"content\":\"newer\"}\n"
        ),
    )
    .unwrap();

    let output = run_backend(&["query-events", &env_path, "", "5"]);
    let events: Vec<Value> = serde_json::from_str(&output).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["id"], "b");
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
    for _ in 0..40 {
        if !store_root.join("events/aa/11/aa11.json").exists() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    assert!(!store_root.join("events/aa/11/aa11.json").exists());
    assert!(store_root.join("events/bb/22/bb22.json").exists());
    assert!(store_root.join("events/cc/33/cc33.json").exists());
}

#[test]
fn apply_preset_nostr_blog_sets_site_mirror_defaults() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);
    let store_root = dir.path().join("store");
    fs::write(
        &env_path,
        format!(
            "STORE_ROOT={}\nBIND_HTTP=127.0.0.1:7777\nBIND_WS=127.0.0.1:7778\nVERIFY_SIG=0\nENABLE_PUBLISH=1\nENABLE_MIRRORING=0\nMIRROR_MODE=broad\nRELAYS_UPSTREAM=\nFILTER_AUTHORS=abc\nFILTER_KINDS=1,7\nFILTER_TAG_A=30023:old:post\nFILTER_TAG_T=foo\n",
            store_root.display()
        ),
    )
    .unwrap();

    run_backend(&["apply-preset", &env_path, "nostr-blog"]);
    let env_text = fs::read_to_string(&env_path).unwrap();

    assert!(env_text.contains("ENABLE_MIRRORING=1\n"));
    assert!(env_text.contains("MIRROR_MODE=site\n"));
    assert!(env_text.contains("MIRROR_SITE_INCLUDE_COMMENTS=1\n"));
    assert!(env_text.contains("ENABLE_PUBLISH=0\n"));
    assert!(env_text.contains("FILTER_PRIVATE_MESSAGES=1\n"));
    assert!(env_text.contains("FILTER_AUTHORS=\n"));
    assert!(env_text.contains("FILTER_KINDS=\n"));
    assert!(env_text.contains("FILTER_TAG_A=\n"));
    assert!(env_text.contains("FILTER_TAG_T=\n"));
    assert!(env_text.contains("RELAYS_UPSTREAM=wss://relay.damus.io,wss://nos.lol"));
}
