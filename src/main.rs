//! Command line interface for operating the relay. Supports initialization,
//! ingesting events, serving HTTP/WebSocket endpoints, mirroring from upstream
//! relays, and signature verification.

mod config;
mod deploy;
mod auth;
mod blossom;
mod event;
mod files;
mod log;
mod mirror;
mod nip98;
mod policy;
mod server;
mod storage;
mod ws;

use std::{net::SocketAddr, time::Duration};

use clap::{Parser, Subcommand};
use config::Settings;
use deploy::{ProxyManager, ServiceManager};
use policy::{apply_query_policy, current_unix_ts, validate_event_with_files};
use serde_json::Value;
use storage::Store;
use tokio::sync::broadcast;

/// Command line interface entry point.
#[derive(Parser)]
#[command(name = "stonr", author, version, about = "File-backed Nostr relay")]
struct Cli {
    /// Path to the `.env` configuration file.
    #[arg(long, default_value = ".env")]
    env: String,
    /// Subcommand to execute.
    #[command(subcommand)]
    command: Commands,
}

/// Supported CLI subcommands.
#[derive(Subcommand)]
enum Commands {
    /// Initialize the directory tree at `STORE_ROOT`.
    Init,
    /// Ingest one or more event files.
    Ingest {
        /// Paths to JSON event files to ingest.
        #[arg(required = true)]
        files: Vec<String>,
    },
    /// Rebuild indexes and latest pointers from existing events.
    Reindex,
    /// Query stored events from the local file-backed store.
    Query {
        /// Comma-separated author pubkeys.
        #[arg(long)]
        authors: Option<String>,
        /// Comma-separated kind numbers.
        #[arg(long)]
        kinds: Option<String>,
        /// Exact `#d` value to match.
        #[arg(long)]
        d: Option<String>,
        /// Exact `#t` value to match.
        #[arg(long)]
        t: Option<String>,
        /// Relay-side text search term.
        #[arg(long)]
        search: Option<String>,
        /// Minimum `created_at` timestamp.
        #[arg(long)]
        since: Option<u64>,
        /// Maximum `created_at` timestamp.
        #[arg(long)]
        until: Option<u64>,
        /// Maximum number of events to return.
        #[arg(long)]
        limit: Option<usize>,
        /// Print only the match count.
        #[arg(long)]
        count: bool,
    },
    /// Print the effective parsed configuration as JSON.
    PrintConfig,
    /// Print per-upstream mirror health/status as JSON.
    MirrorStatus,
    /// Print structured retention/cap status as JSON.
    RetentionStatus,
    /// Inspect or repair a stored mirror cursor.
    MirrorCursor {
        #[command(subcommand)]
        command: MirrorCursorCommand,
    },
    /// Create an authoritative snapshot of the relay store at DESTINATION.
    Backup {
        #[arg(long)]
        destination: String,
    },
    /// Restore a previously created store snapshot from SOURCE.
    Restore {
        #[arg(long)]
        source: String,
    },
    /// Print a service definition for a process supervisor.
    PrintService {
        #[arg(long, value_enum)]
        manager: ServiceManager,
        #[arg(long, default_value = "stonr")]
        label: String,
    },
    /// Print a reverse-proxy config that fronts the HTTP and WS ports together.
    PrintProxy {
        #[arg(long, value_enum)]
        manager: ProxyManager,
        #[arg(long)]
        domain: String,
        #[arg(long)]
        tls_cert: Option<String>,
        #[arg(long)]
        tls_key: Option<String>,
    },
    /// Apply store retention limits immediately.
    PruneRetention,
    /// Recompute exact event count and byte stats from the store.
    RefreshStats,
    /// Launch HTTP and WebSocket services (and mirror if configured).
    Serve,
    /// Verify a random sample of stored events.
    Verify {
        #[arg(long, default_value_t = 1000)]
        sample: usize,
    },
}

#[derive(Subcommand)]
enum MirrorCursorCommand {
    /// Show the current stored cursor for one relay/scope.
    Get {
        #[arg(long)]
        relay: String,
        #[arg(long, default_value = "broad")]
        scope: String,
    },
    /// Set the stored cursor to an explicit Unix timestamp.
    Set {
        #[arg(long)]
        relay: String,
        #[arg(long, default_value = "broad")]
        scope: String,
        #[arg(long)]
        since: u64,
    },
    /// Clear the stored cursor so the next mirror run starts fresh.
    Clear {
        #[arg(long)]
        relay: String,
        #[arg(long, default_value = "broad")]
        scope: String,
    },
}

struct QueryArgs {
    authors: Option<String>,
    kinds: Option<String>,
    d: Option<String>,
    t: Option<String>,
    search: Option<String>,
    since: Option<u64>,
    until: Option<u64>,
    limit: Option<usize>,
}

fn cli_query(args: QueryArgs) -> storage::Query {
    let mut obj = serde_json::Map::new();
    if let Some(authors) = args.authors {
        obj.insert(
            "authors".into(),
            Value::Array(
                authors
                    .split(',')
                    .filter(|value| !value.is_empty())
                    .map(|value| Value::String(value.to_string()))
                    .collect(),
            ),
        );
    }
    if let Some(kinds) = args.kinds {
        obj.insert(
            "kinds".into(),
            Value::Array(
                kinds
                    .split(',')
                    .filter_map(|value| value.parse::<u32>().ok())
                    .map(|value| Value::Number(value.into()))
                    .collect(),
            ),
        );
    }
    if let Some(d) = args.d {
        obj.insert("#d".into(), Value::Array(vec![Value::String(d)]));
    }
    if let Some(t) = args.t {
        obj.insert("#t".into(), Value::Array(vec![Value::String(t)]));
    }
    if let Some(search) = args.search {
        obj.insert("search".into(), Value::String(search));
    }
    if let Some(since) = args.since {
        obj.insert("since".into(), Value::Number(since.into()));
    }
    if let Some(until) = args.until {
        obj.insert("until".into(), Value::Number(until.into()));
    }
    if let Some(limit) = args.limit {
        obj.insert("limit".into(), Value::Number((limit as u64).into()));
    }
    storage::Query::from_value(&Value::Object(obj))
}

/// Execute the selected CLI subcommand.
async fn run(cli: Cli) -> anyhow::Result<()> {
    let cfg = Settings::from_env(&cli.env)?;
    let store = Store::with_limits(
        cfg.store_root.clone(),
        cfg.verify_sig,
        cfg.max_stored_events,
        cfg.max_stored_event_bytes,
    );
    match cli.command {
        Commands::Init => {
            // Create the on-disk directory structure.
            store.init()?;
        }
        Commands::Ingest { files } => {
            // Load each JSON file and store it if not already present.
            let file_store = store.files();
            for f in files {
                let data = std::fs::read_to_string(&f)?;
                let ev: event::Event = serde_json::from_str(&data)?;
                validate_event_with_files(&cfg, &file_store, &ev, current_unix_ts())?;
                if store.ingest_with_policy(&ev, cfg.delete_enabled(), cfg.expiration_enabled())? {
                    file_store.add_event_references(&ev)?;
                }
            }
        }
        Commands::Reindex => {
            // Rebuild indexes and latest pointers from existing events.
            store.reindex()?;
            let visible = store.query_with_policy(
                storage::Query {
                    authors: None,
                    kinds: None,
                    d: None,
                    t: None,
                    tags: vec![],
                    search: None,
                    since: None,
                    until: None,
                    limit: None,
                },
                cfg.delete_enabled(),
                cfg.expiration_enabled(),
            )?;
            store.files().rebuild_references(&visible)?;
            store.files().prune(&cfg, false)?;
        }
        Commands::Query {
            authors,
            kinds,
            d,
            t,
            search,
            since,
            until,
            limit,
            count,
        } => {
            let q = apply_query_policy(
                &cfg,
                cli_query(QueryArgs {
                authors,
                kinds,
                d,
                t,
                search,
                since,
                until,
                limit,
                }),
            );
            let events = store.query_with_policy(q, cfg.delete_enabled(), cfg.expiration_enabled())?;
            if count {
                println!("{}", events.len());
            } else {
                for event in events {
                    println!("{}", serde_json::to_string(&event)?);
                }
            }
        }
        Commands::PrintConfig => {
            println!("{}", serde_json::to_string(&cfg)?);
        }
        Commands::MirrorStatus => {
            println!(
                "{}",
                serde_json::to_string(
                    &crate::mirror::read_statuses(&cfg.store_root)?
                )?
            );
        }
        Commands::RetentionStatus => {
            println!("{}", serde_json::to_string(&store.retention_status()?)?);
        }
        Commands::MirrorCursor { command } => match command {
            MirrorCursorCommand::Get { relay, scope } => {
                println!(
                    "{}",
                    serde_json::to_string(&crate::mirror::get_cursor(
                        &cfg.store_root,
                        &relay,
                        &scope,
                    ))?
                );
            }
            MirrorCursorCommand::Set {
                relay,
                scope,
                since,
            } => {
                println!(
                    "{}",
                    serde_json::to_string(&crate::mirror::set_cursor(
                        &cfg.store_root,
                        &relay,
                        &scope,
                        since,
                    )?)?
                );
            }
            MirrorCursorCommand::Clear { relay, scope } => {
                println!(
                    "{}",
                    serde_json::to_string(&crate::mirror::clear_cursor(
                        &cfg.store_root,
                        &relay,
                        &scope,
                    )?)?
                );
            }
        },
        Commands::Backup { destination } => {
            store.init()?;
            println!(
                "{}",
                serde_json::to_string(&store.backup_to(std::path::Path::new(&destination))?)?
            );
        }
        Commands::Restore { source } => {
            println!(
                "{}",
                serde_json::to_string(&store.restore_from(std::path::Path::new(&source))?)?
            );
        }
        Commands::PrintService { manager, label } => {
            let exec_path = std::env::current_exe()?;
            let env_path = std::path::PathBuf::from(&cli.env);
            let rendered = crate::deploy::render_service(
                manager,
                &label,
                &exec_path,
                &env_path,
                &cfg.store_root,
            )?;
            print!("{rendered}");
        }
        Commands::PrintProxy {
            manager,
            domain,
            tls_cert,
            tls_key,
        } => {
            let rendered = crate::deploy::render_proxy(
                manager,
                &domain,
                &cfg.bind_http,
                &cfg.bind_ws,
                tls_cert.as_deref(),
                tls_key.as_deref(),
            )?;
            print!("{rendered}");
        }
        Commands::PruneRetention => {
            store.init()?;
            store.enforce_retention()?;
            let visible = store.query_with_policy(
                storage::Query {
                    authors: None,
                    kinds: None,
                    d: None,
                    t: None,
                    tags: vec![],
                    search: None,
                    since: None,
                    until: None,
                    limit: None,
                },
                cfg.delete_enabled(),
                cfg.expiration_enabled(),
            )?;
            store.files().rebuild_references(&visible)?;
            store.files().prune(&cfg, false)?;
        }
        Commands::RefreshStats => {
            store.init()?;
            store.refresh_stats_cache()?;
        }
        Commands::Serve => {
            // Initialize storage then start HTTP and WS servers.
            store.init()?;
            let http_addr: SocketAddr = cfg.bind_http.parse()?;
            let ws_addr: SocketAddr = cfg.bind_ws.parse()?;
            let (events_tx, _) = broadcast::channel(1024);
            let stats_store = store.clone();
                tokio::spawn(async move {
                if let Err(error) = stats_store.refresh_stats_cache() {
                    crate::log::warn(
                        "runtime",
                        "failed to refresh stats cache",
                        serde_json::json!({ "error": error.to_string() }),
                    );
                }
            });
            if cfg.max_stored_events.is_some() || cfg.max_stored_event_bytes.is_some() {
                let retention_store = store.clone();
                let retention_cfg = cfg.clone();
                tokio::spawn(async move {
                    loop {
                        let result = retention_store
                            .enforce_retention()
                            .and_then(|_| {
                                let visible = retention_store.query_with_policy(
                                    storage::Query {
                                        authors: None,
                                        kinds: None,
                                        d: None,
                                        t: None,
                                        tags: vec![],
                                        search: None,
                                        since: None,
                                        until: None,
                                        limit: None,
                                    },
                                    retention_cfg.delete_enabled(),
                                    retention_cfg.expiration_enabled(),
                                )?;
                                retention_store.files().rebuild_references(&visible)?;
                                retention_store.files().prune(&retention_cfg, false)?;
                                Ok(())
                            });
                        if let Err(error) = result {
                            crate::log::warn(
                                "retention",
                                "failed to enforce retention",
                                serde_json::json!({ "error": error.to_string() }),
                            );
                            let _ = retention_store.report_retention_error(&error.to_string());
                        }
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    }
                });
            }
            // If upstream relays are configured, start mirroring in the background.
            if cfg.enable_mirroring && !cfg.relays_upstream.is_empty() {
                let store_clone = store.clone();
                let cfg_clone = cfg.clone();
                let mirror_events_tx = events_tx.clone();
                tokio::spawn(async move { mirror::run(cfg_clone, store_clone, mirror_events_tx).await });
            }
            let store_http = store.clone();
            let store_ws = store.clone();
            let cfg_http = cfg.clone();
            let cfg_ws = cfg.clone();
            tokio::try_join!(
                server::serve_http(http_addr, store_http, cfg_http, std::future::pending()),
                ws::serve_ws(ws_addr, store_ws, cfg_ws, events_tx, std::future::pending())
            )?;
        }
        Commands::Verify { sample } => {
            // Randomly verify Schnorr signatures for `sample` events.
            store.verify_sample(sample)?;
        }
    }
    Ok(())
}

#[cfg(not(test))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    run(cli).await
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use crate::event::Event;
    use std::{fs, sync::Mutex, time::Duration};
    use tempfile::TempDir;
    use tokio::{net::TcpListener, task};

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    async fn write_env(dir: &TempDir, extra: &str) -> String {
        let env_path = dir.path().join(".env");
        let content = format!(
            "STORE_ROOT={}\nBIND_HTTP=127.0.0.1:0\nBIND_WS=127.0.0.1:0\nVERIFY_SIG=0\nRELAYS_UPSTREAM=\n{}",
            dir.path().to_str().unwrap(),
            extra
        );
        fs::write(&env_path, content).unwrap();
        env_path.to_str().unwrap().into()
    }

    #[tokio::test]
    async fn run_init_ingest_reindex_verify() {
        let _g = ENV_MUTEX.lock().unwrap();
        for v in [
            "STORE_ROOT",
            "BIND_HTTP",
            "BIND_WS",
            "VERIFY_SIG",
            "RELAYS_UPSTREAM",
            "TOR_SOCKS",
        ] {
            std::env::remove_var(v);
        }
        let dir = TempDir::new().unwrap();
        let env_file = write_env(&dir, "").await;

        // init
        run(Cli {
            env: env_file.clone(),
            command: Commands::Init,
        })
        .await
        .unwrap();

        // ingest
        let ev_path = dir.path().join("ev.json");
        let ev = Event {
            id: "0000000000000000000000000000000000000000000000000000000000000000".into(),
            pubkey: "p".into(),
            kind: 1,
            created_at: 1,
            tags: vec![],
            content: String::new(),
            sig: String::new(),
        };
        fs::write(&ev_path, serde_json::to_string(&ev).unwrap()).unwrap();
        run(Cli {
            env: env_file.clone(),
            command: Commands::Ingest {
                files: vec![ev_path.to_str().unwrap().into()],
            },
        })
        .await
        .unwrap();

        // reindex
        run(Cli {
            env: env_file.clone(),
            command: Commands::Reindex,
        })
        .await
        .unwrap();

        // verify with zero sample to avoid signature check
        run(Cli {
            env: env_file,
            command: Commands::Verify { sample: 0 },
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn run_serve_starts_http() {
        let _g = ENV_MUTEX.lock().unwrap();
        for v in [
            "STORE_ROOT",
            "BIND_HTTP",
            "BIND_WS",
            "VERIFY_SIG",
            "RELAYS_UPSTREAM",
            "TOR_SOCKS",
        ] {
            std::env::remove_var(v);
        }
        let dir = TempDir::new().unwrap();
        let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_port = http_listener.local_addr().unwrap().port();
        drop(http_listener);
        let ws_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_port = ws_listener.local_addr().unwrap().port();
        drop(ws_listener);
        let env_path = dir.path().join(".env");
        let content = format!(
            "STORE_ROOT={}\nBIND_HTTP=127.0.0.1:{}\nBIND_WS=127.0.0.1:{}\nVERIFY_SIG=0\nRELAYS_UPSTREAM=\n",
            dir.path().to_str().unwrap(),
            http_port,
            ws_port
        );
        fs::write(&env_path, content).unwrap();
        let env_str = env_path.to_str().unwrap().to_string();

        let handle = task::spawn(run(Cli {
            env: env_str.clone(),
            command: Commands::Serve,
        }));
        tokio::time::sleep(Duration::from_millis(200)).await;
        let url = format!("http://127.0.0.1:{}/healthz", http_port);
        let resp = reqwest::get(url).await.unwrap();
        assert!(resp.status().is_success());
        handle.abort();
    }

    #[tokio::test]
    async fn run_serve_spawns_mirror() {
        let _g = ENV_MUTEX.lock().unwrap();
        for v in [
            "STORE_ROOT",
            "BIND_HTTP",
            "BIND_WS",
            "VERIFY_SIG",
            "RELAYS_UPSTREAM",
            "TOR_SOCKS",
        ] {
            std::env::remove_var(v);
        }
        let dir = TempDir::new().unwrap();
        let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_port = http_listener.local_addr().unwrap().port();
        drop(http_listener);
        let ws_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_port = ws_listener.local_addr().unwrap().port();
        drop(ws_listener);
        let env_path = dir.path().join(".env");
        let content = format!(
            "STORE_ROOT={}\nBIND_HTTP=127.0.0.1:{}\nBIND_WS=127.0.0.1:{}\nVERIFY_SIG=0\nRELAYS_UPSTREAM=ws://127.0.0.1:9\n",
            dir.path().to_str().unwrap(),
            http_port,
            ws_port
        );
        fs::write(&env_path, content).unwrap();
        let env_str = env_path.to_str().unwrap().to_string();

        let handle = task::spawn(run(Cli {
            env: env_str.clone(),
            command: Commands::Serve,
        }));
        tokio::time::sleep(Duration::from_millis(200)).await;
        let url = format!("http://127.0.0.1:{}/healthz", http_port);
        let resp = reqwest::get(url).await.unwrap();
        assert!(resp.status().is_success());
        handle.abort();
    }

    #[tokio::test]
    async fn run_serve_starts_without_blocking_on_retention_scan() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = TempDir::new().unwrap();
        let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_port = http_listener.local_addr().unwrap().port();
        drop(http_listener);
        let ws_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_port = ws_listener.local_addr().unwrap().port();
        drop(ws_listener);
        let store_root = dir.path().join("store");
        fs::create_dir_all(store_root.join("events/aa/bb")).unwrap();
        fs::write(store_root.join("events/aa/bb/bad.json"), [0xff, 0xfe, 0xfd]).unwrap();
        let env_path = dir.path().join(".env");
        let content = format!(
            "STORE_ROOT={}\nBIND_HTTP=127.0.0.1:{}\nBIND_WS=127.0.0.1:{}\nVERIFY_SIG=0\nRELAYS_UPSTREAM=\nMAX_STORED_EVENT_BYTES=1\n",
            store_root.to_str().unwrap(),
            http_port,
            ws_port
        );
        fs::write(&env_path, content).unwrap();
        let env_str = env_path.to_str().unwrap().to_string();

        let handle = task::spawn(run(Cli {
            env: env_str,
            command: Commands::Serve,
        }));
        tokio::time::sleep(Duration::from_millis(200)).await;
        let url = format!("http://127.0.0.1:{}/healthz", http_port);
        let resp = reqwest::get(url).await.unwrap();
        assert!(resp.status().is_success());
        handle.abort();
    }

    #[tokio::test]
    async fn run_prune_retention_removes_oldest_events() {
        let _g = ENV_MUTEX.lock().unwrap();
        let dir = TempDir::new().unwrap();
        let env_file = write_env(&dir, "MAX_STORED_EVENTS=2\n").await;

        run(Cli {
            env: env_file.clone(),
            command: Commands::Init,
        })
        .await
        .unwrap();

        for (id, created_at) in [("aa11", 10u64), ("bb22", 20u64), ("cc33", 30u64)] {
            let ev_path = dir.path().join(format!("{id}.json"));
            let ev = Event {
                id: id.into(),
                pubkey: "p".into(),
                kind: 1,
                created_at,
                tags: vec![],
                content: String::new(),
                sig: String::new(),
            };
            fs::write(&ev_path, serde_json::to_string(&ev).unwrap()).unwrap();
            run(Cli {
                env: env_file.clone(),
                command: Commands::Ingest {
                    files: vec![ev_path.to_str().unwrap().into()],
                },
            })
            .await
            .unwrap();
        }

        run(Cli {
            env: env_file,
            command: Commands::PruneRetention,
        })
        .await
        .unwrap();

        assert!(!dir.path().join("events/aa/11/aa11.json").exists());
        assert!(dir.path().join("events/bb/22/bb22.json").exists());
        assert!(dir.path().join("events/cc/33/cc33.json").exists());
    }
}
