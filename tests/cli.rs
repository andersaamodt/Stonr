use assert_cmd::prelude::*;
use secp256k1::{Keypair, Message, Secp256k1};
use sha2::{Digest, Sha256};
use std::{fs, process::Command};
use tempfile::TempDir;

fn write_env(dir: &TempDir) -> String {
    let env_path = dir.path().join("env");
    let content = format!(
        "STORE_ROOT={}\nBIND_HTTP=127.0.0.1:0\nBIND_WS=127.0.0.1:0\nVERIFY_SIG=0\n",
        dir.path().display()
    );
    fs::write(&env_path, content).unwrap();
    env_path.to_str().unwrap().to_string()
}

fn signed_event_json() -> serde_json::Value {
    let secp = Secp256k1::new();
    let sk = [1u8; 32];
    let kp = Keypair::from_seckey_slice(&secp, &sk).unwrap();
    let pubkey = hex::encode(kp.x_only_public_key().0.serialize());
    let created_at = 1u64;
    let kind = 1u32;
    let tags: Vec<Vec<String>> = vec![];
    let arr = serde_json::json!([0, pubkey, created_at, kind, tags, ""]);
    let data = serde_json::to_vec(&arr).unwrap();
    let hash = Sha256::digest(&data);
    let id = hex::encode(hash);
    let msg = Message::from_digest_slice(&hash).unwrap();
    let sig = secp.sign_schnorr_no_aux_rand(&msg, &kp);
    serde_json::json!({
        "id": id,
        "pubkey": pubkey,
        "kind": kind,
        "created_at": created_at,
        "tags": tags,
        "content": "",
        "sig": hex::encode(sig.as_ref()),
    })
}

#[test]
fn reindex_cli_rebuilds_indexes() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);

    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "init"])
        .assert()
        .success();

    let ev = signed_event_json();
    let ev_path = dir.path().join("ev.json");
    fs::write(&ev_path, serde_json::to_string(&ev).unwrap()).unwrap();

    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "ingest", ev_path.to_str().unwrap()])
        .assert()
        .success();

    fs::remove_dir_all(dir.path().join("index")).unwrap();
    fs::remove_dir_all(dir.path().join("latest")).unwrap();

    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "reindex"])
        .assert()
        .success();

    assert!(dir
        .path()
        .join("index/by-author")
        .read_dir()
        .unwrap()
        .next()
        .is_some());
}

#[test]
fn verify_cli_success_and_failure() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);

    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "init"])
        .assert()
        .success();

    // valid event
    let good = signed_event_json();
    let good_path = dir.path().join("good.json");
    fs::write(&good_path, serde_json::to_string(&good).unwrap()).unwrap();
    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "ingest", good_path.to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "verify", "--sample", "10"])
        .assert()
        .success();

    // ingest event with mismatched id
    let mut bad = signed_event_json();
    bad["id"] = serde_json::Value::String("ff".repeat(32));
    let bad_path = dir.path().join("bad.json");
    fs::write(&bad_path, serde_json::to_string(&bad).unwrap()).unwrap();
    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "ingest", bad_path.to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "verify", "--sample", "10"])
        .assert()
        .failure();
}

#[test]
fn init_and_ingest_cli_store_event() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);

    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "init"])
        .assert()
        .success();

    let ev = signed_event_json();
    let ev_path = dir.path().join("ev.json");
    fs::write(&ev_path, serde_json::to_string(&ev).unwrap()).unwrap();

    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "ingest", ev_path.to_str().unwrap()])
        .assert()
        .success();

    let id = ev["id"].as_str().unwrap();
    let stored = dir
        .path()
        .join("events")
        .join(&id[0..2])
        .join(&id[2..4])
        .join(format!("{}.json", id));
    assert!(stored.exists());
}

#[test]
fn cli_help_lists_commands() {
    let output = Command::cargo_bin("stonr")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    for cmd in [
        "init",
        "ingest",
        "serve",
        "reindex",
        "query",
        "mirror-status",
        "mirror-cursor",
        "prune-retention",
        "verify",
    ] {
        assert!(text.contains(cmd));
    }
}

#[test]
fn mirror_cursor_cli_can_get_set_and_clear() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);

    let relay = "wss://relay.example";
    let output = Command::cargo_bin("stonr")
        .unwrap()
        .args([
            "--env",
            &env_path,
            "mirror-cursor",
            "get",
            "--relay",
            relay,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let body: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(body["relay"], relay);
    assert_eq!(body["scope"], "broad");
    assert!(body["since"].is_null());

    let output = Command::cargo_bin("stonr")
        .unwrap()
        .args([
            "--env",
            &env_path,
            "mirror-cursor",
            "set",
            "--relay",
            relay,
            "--scope",
            "site-posts",
            "--since",
            "123",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let body: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(body["scope"], "site-posts");
    assert_eq!(body["since"], 123);

    let output = Command::cargo_bin("stonr")
        .unwrap()
        .args([
            "--env",
            &env_path,
            "mirror-cursor",
            "clear",
            "--relay",
            relay,
            "--scope",
            "site-posts",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let body: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(body["since"].is_null());
}

#[test]
fn query_cli_filters_and_counts() {
    let dir = TempDir::new().unwrap();
    let env_path = write_env(&dir);

    Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "init"])
        .assert()
        .success();

    for (path_name, mut event, kind, created_at) in [
        ("a.json", signed_event_json(), 1u64, 1u64),
        ("b.json", signed_event_json(), 1u64, 2u64),
        ("c.json", signed_event_json(), 2u64, 3u64),
    ] {
        event["kind"] = serde_json::Value::Number(kind.into());
        event["created_at"] = serde_json::Value::Number(created_at.into());
        let arr = serde_json::json!([
            0,
            event["pubkey"].as_str().unwrap(),
            created_at,
            kind,
            [],
            ""
        ]);
        let data = serde_json::to_vec(&arr).unwrap();
        let hash = Sha256::digest(&data);
        event["id"] = serde_json::Value::String(hex::encode(hash));
        let path = dir.path().join(path_name);
        fs::write(&path, serde_json::to_string(&event).unwrap()).unwrap();
        Command::cargo_bin("stonr")
            .unwrap()
            .args(["--env", &env_path, "ingest", path.to_str().unwrap()])
            .assert()
            .success();
    }

    let output = Command::cargo_bin("stonr")
        .unwrap()
        .args(["--env", &env_path, "query", "--kinds", "1", "--count"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(String::from_utf8(output).unwrap().trim(), "2");
}
