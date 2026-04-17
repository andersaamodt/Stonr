use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use bech32::{self, Bech32, Hrp};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    Key, XChaCha20Poly1305, XNonce,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use futures_util::{SinkExt, StreamExt};
use nostr_shared::{
    crypto::sign_event,
    event::{Event, Tag},
    parity,
};
use rand::{rngs::OsRng, RngCore};
use reqwest::multipart;
use scrypt::{scrypt, Params as ScryptParams};
use secp256k1::{Keypair, Secp256k1, SecretKey};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

#[derive(Parser)]
#[command(name = "onstr-core", version, about = "Onstr Nostr client core engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Profile(ProfileCommand),
    Relay(RelayCommand),
    Timeline(TimelineCommand),
    Discover(DiscoverCommand),
    Compose(ComposeCommand),
    Publish(PublishCommand),
    Library(LibraryCommand),
    Media(MediaCommand),
    Doctor,
}

#[derive(Subcommand)]
enum ProfileAction {
    List,
    Create(ProfileCreate),
    Import(ProfileImport),
    Export(ProfileExport),
    Use(ProfileUse),
    Unlock(ProfileUnlock),
    Lock(ProfileLock),
}

#[derive(Args)]
struct ProfileCommand {
    #[command(subcommand)]
    action: ProfileAction,
}

#[derive(Args)]
struct ProfileCreate {
    #[arg(long)]
    name: String,
    #[arg(long)]
    password: String,
    #[arg(long)]
    secret_key: Option<String>,
    #[arg(long, default_value_t = false)]
    set_active: bool,
}

#[derive(Args)]
struct ProfileImport {
    #[arg(long)]
    name: String,
    #[arg(long)]
    password: String,
    #[arg(long)]
    ncryptsec: String,
    #[arg(long, default_value_t = false)]
    set_active: bool,
}

#[derive(Args)]
struct ProfileExport {
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    password: String,
}

#[derive(Args)]
struct ProfileUse {
    #[arg(long)]
    id: String,
}

#[derive(Args)]
struct ProfileUnlock {
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    password: String,
    #[arg(long, default_value_t = 900)]
    ttl_secs: u64,
}

#[derive(Args)]
struct ProfileLock {
    #[arg(long)]
    id: Option<String>,
}

#[derive(Subcommand)]
enum RelayAction {
    List,
    Add(RelayAdd),
    Remove(RelayRemove),
    SetHome(RelaySetHome),
    Probe(RelayProbe),
}

#[derive(Args)]
struct RelayCommand {
    #[command(subcommand)]
    action: RelayAction,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum RelayMode {
    Read,
    Write,
    Both,
}

#[derive(Args)]
struct RelayAdd {
    #[arg(long)]
    url: String,
    #[arg(long, value_enum, default_value_t = RelayMode::Both)]
    mode: RelayMode,
}

#[derive(Args)]
struct RelayRemove {
    #[arg(long)]
    url: String,
}

#[derive(Args)]
struct RelaySetHome {
    #[arg(long)]
    url: String,
}

#[derive(Args)]
struct RelayProbe {
    #[arg(long)]
    url: Option<String>,
}

#[derive(Subcommand)]
enum TimelineAction {
    Fetch(TimelineFetch),
}

#[derive(Args)]
struct TimelineCommand {
    #[command(subcommand)]
    action: TimelineAction,
}

#[derive(Args)]
struct TimelineFetch {
    #[arg(long)]
    authors: Option<String>,
    #[arg(long)]
    kinds: Option<String>,
    #[arg(long)]
    search: Option<String>,
    #[arg(long)]
    since: Option<u64>,
    #[arg(long)]
    until: Option<u64>,
    #[arg(long, default_value_t = 50)]
    limit: usize,
    #[arg(long, default_value_t = true)]
    include_remotes: bool,
    #[arg(long = "tag-p")]
    tag_p: Option<String>,
}

#[derive(Subcommand)]
enum DiscoverAction {
    Search(DiscoverSearch),
    Count(DiscoverCount),
    RelayInfo(DiscoverRelayInfo),
}

#[derive(Args)]
struct DiscoverCommand {
    #[command(subcommand)]
    action: DiscoverAction,
}

#[derive(Args)]
struct DiscoverSearch {
    #[arg(long)]
    term: String,
    #[arg(long, default_value_t = 30)]
    limit: usize,
}

#[derive(Args)]
struct DiscoverCount {
    #[arg(long)]
    term: Option<String>,
}

#[derive(Args)]
struct DiscoverRelayInfo {
    #[arg(long)]
    url: String,
}

#[derive(Subcommand)]
enum ComposeAction {
    Note(ComposeNote),
    Reply(ComposeReply),
    Longform(ComposeLongform),
    FileMetadata(ComposeFileMetadata),
    Delete(ComposeDelete),
    ListDrafts,
    Preview(ComposePreview),
    SignDraft(ComposeSignDraft),
}

#[derive(Args)]
struct ComposeCommand {
    #[command(subcommand)]
    action: ComposeAction,
}

#[derive(Args)]
struct ComposeNote {
    #[arg(long)]
    content: String,
    #[arg(long)]
    tags: Vec<String>,
    #[arg(long)]
    draft: Option<String>,
}

#[derive(Args)]
struct ComposeReply {
    #[arg(long)]
    content: String,
    #[arg(long)]
    event_id: String,
    #[arg(long)]
    draft: Option<String>,
}

#[derive(Args)]
struct ComposeLongform {
    #[arg(long)]
    title: String,
    #[arg(long)]
    identifier: String,
    #[arg(long)]
    content: String,
    #[arg(long)]
    summary: Option<String>,
    #[arg(long)]
    draft: Option<String>,
}

#[derive(Args)]
struct ComposeFileMetadata {
    #[arg(long)]
    url: String,
    #[arg(long)]
    hash: String,
    #[arg(long)]
    mime: String,
    #[arg(long)]
    size: u64,
    #[arg(long)]
    draft: Option<String>,
}

#[derive(Args)]
struct ComposeDelete {
    #[arg(long)]
    event_id: String,
    #[arg(long)]
    reason: Option<String>,
    #[arg(long)]
    draft: Option<String>,
}

#[derive(Args)]
struct ComposePreview {
    #[arg(long)]
    draft: String,
}

#[derive(Args)]
struct ComposeSignDraft {
    #[arg(long)]
    draft: String,
    #[arg(long)]
    profile_id: Option<String>,
    #[arg(long)]
    password: String,
}

#[derive(Subcommand)]
enum PublishAction {
    EventFile(PublishEventFile),
    Draft(PublishDraft),
}

#[derive(Args)]
struct PublishCommand {
    #[command(subcommand)]
    action: PublishAction,
}

#[derive(Args)]
struct PublishEventFile {
    #[arg(long)]
    path: PathBuf,
    #[arg(long)]
    profile_id: Option<String>,
    #[arg(long)]
    password: String,
    #[arg(long)]
    relay: Vec<String>,
}

#[derive(Args)]
struct PublishDraft {
    #[arg(long)]
    draft: String,
    #[arg(long)]
    profile_id: Option<String>,
    #[arg(long)]
    password: String,
    #[arg(long)]
    relay: Vec<String>,
}

#[derive(Subcommand)]
enum LibraryAction {
    List(LibraryList),
    Star(LibraryMutate),
    Unstar(LibraryMutate),
    Save(LibraryMutate),
    Unsave(LibraryMutate),
    IngestAuthored(LibraryIngest),
    Reindex,
}

#[derive(Args)]
struct LibraryCommand {
    #[command(subcommand)]
    action: LibraryAction,
}

#[derive(Args)]
struct LibraryMutate {
    #[arg(long)]
    event_id: String,
}

#[derive(Args)]
struct LibraryList {
    #[arg(long)]
    bucket: Option<String>,
}

#[derive(Args)]
struct LibraryIngest {
    #[arg(long)]
    path: PathBuf,
}

#[derive(Subcommand)]
enum MediaAction {
    Nip94Template(MediaNip94Template),
    UploadNip96(MediaUploadNip96),
}

#[derive(Args)]
struct MediaCommand {
    #[command(subcommand)]
    action: MediaAction,
}

#[derive(Args)]
struct MediaNip94Template {
    #[arg(long)]
    url: String,
    #[arg(long)]
    hash: String,
    #[arg(long)]
    mime: String,
    #[arg(long)]
    size: u64,
    #[arg(long)]
    name: Option<String>,
}

#[derive(Args)]
struct MediaUploadNip96 {
    #[arg(long)]
    relay_url: String,
    #[arg(long)]
    file: PathBuf,
    #[arg(long)]
    profile_id: Option<String>,
    #[arg(long)]
    password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfileRecord {
    id: String,
    name: String,
    pubkey: String,
    ncryptsec: String,
    created_at: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ProfileIndex {
    active_profile: Option<String>,
    profiles: Vec<ProfileRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RelayConfig {
    home: String,
    read: Vec<String>,
    write: Vec<String>,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            home: String::new(),
            read: Vec::new(),
            write: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct LibraryIndex {
    starred: Vec<String>,
    saved: Vec<String>,
    liked: Vec<String>,
    commented: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DraftRecord {
    name: String,
    category: String,
    event: Event,
    updated_at: u64,
}

#[derive(Debug, Clone)]
struct Paths {
    config_root: PathBuf,
    state_root: PathBuf,
    profiles_file: PathBuf,
    relays_file: PathBuf,
    drafts_dir: PathBuf,
    library_file: PathBuf,
    authored_cache: PathBuf,
    cursors_dir: PathBuf,
}

impl Paths {
    fn discover() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let config_root = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".config"))
            .join("onstr");
        let state_root = std::env::var("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".local/state"))
            .join("onstr");
        Self {
            profiles_file: config_root.join("profiles.json"),
            relays_file: config_root.join("relays.json"),
            drafts_dir: state_root.join("drafts"),
            library_file: state_root.join("library/index.json"),
            authored_cache: state_root.join("library/authored-events.ndjson"),
            cursors_dir: state_root.join("cursors"),
            config_root,
            state_root,
        }
    }

    fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.config_root)?;
        fs::create_dir_all(&self.state_root)?;
        fs::create_dir_all(&self.drafts_dir)?;
        if let Some(parent) = self.library_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(&self.cursors_dir)?;
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        print_json(&json!({"ok": false, "error": error.to_string()}));
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let paths = Paths::discover();
    paths.ensure()?;

    match cli.command {
        Commands::Profile(cmd) => handle_profile(cmd, &paths),
        Commands::Relay(cmd) => handle_relay(cmd, &paths).await,
        Commands::Timeline(cmd) => handle_timeline(cmd, &paths).await,
        Commands::Discover(cmd) => handle_discover(cmd, &paths).await,
        Commands::Compose(cmd) => handle_compose(cmd, &paths),
        Commands::Publish(cmd) => handle_publish(cmd, &paths).await,
        Commands::Library(cmd) => handle_library(cmd, &paths),
        Commands::Media(cmd) => handle_media(cmd, &paths).await,
        Commands::Doctor => handle_doctor(&paths),
    }
}

fn handle_profile(cmd: ProfileCommand, paths: &Paths) -> Result<()> {
    let mut index: ProfileIndex = load_or_default(&paths.profiles_file)?;
    match cmd.action {
        ProfileAction::List => {
            print_json(&json!({
                "ok": true,
                "active_profile": index.active_profile,
                "profiles": index.profiles,
            }));
        }
        ProfileAction::Create(args) => {
            let secret_hex = if let Some(secret) = args.secret_key {
                validate_secret_hex(&secret)?;
                secret
            } else {
                let mut raw = [0u8; 32];
                OsRng.fill_bytes(&mut raw);
                hex::encode(raw)
            };
            let pubkey = pubkey_from_secret(&secret_hex)?;
            let ncryptsec = encrypt_secret_key_nip49(&secret_hex, &args.password)?;
            let id = short_random_id();
            let record = ProfileRecord {
                id: id.clone(),
                name: args.name,
                pubkey,
                ncryptsec,
                created_at: unix_now(),
            };
            index.profiles.push(record.clone());
            if args.set_active || index.active_profile.is_none() {
                index.active_profile = Some(id.clone());
            }
            save_json_pretty(&paths.profiles_file, &index)?;
            print_json(&json!({
                "ok": true,
                "profile": record,
                "active_profile": index.active_profile,
            }));
        }
        ProfileAction::Import(args) => {
            let secret = decrypt_secret_key_nip49(&args.ncryptsec, &args.password)?;
            let pubkey = pubkey_from_secret(&secret)?;
            let id = short_random_id();
            let record = ProfileRecord {
                id: id.clone(),
                name: args.name,
                pubkey,
                ncryptsec: args.ncryptsec,
                created_at: unix_now(),
            };
            index.profiles.push(record.clone());
            if args.set_active || index.active_profile.is_none() {
                index.active_profile = Some(id);
            }
            save_json_pretty(&paths.profiles_file, &index)?;
            print_json(
                &json!({"ok": true, "profile": record, "active_profile": index.active_profile}),
            );
        }
        ProfileAction::Export(args) => {
            let profile = resolve_profile_record(&index, args.id.as_deref())?;
            // Verify password before export.
            let _ = decrypt_secret_key_nip49(&profile.ncryptsec, &args.password)?;
            print_json(
                &json!({"ok": true, "profile_id": profile.id, "ncryptsec": profile.ncryptsec}),
            );
        }
        ProfileAction::Use(args) => {
            ensure_profile_exists(&index, &args.id)?;
            index.active_profile = Some(args.id);
            save_json_pretty(&paths.profiles_file, &index)?;
            print_json(&json!({"ok": true, "active_profile": index.active_profile}));
        }
        ProfileAction::Unlock(args) => {
            let profile = resolve_profile_record(&index, args.id.as_deref())?;
            let secret_key = decrypt_secret_key_nip49(&profile.ncryptsec, &args.password)?;
            let expires_at = unix_now().saturating_add(args.ttl_secs);
            print_json(&json!({
                "ok": true,
                "profile_id": profile.id,
                "pubkey": profile.pubkey,
                "secret_key": secret_key,
                "expires_at": expires_at,
            }));
        }
        ProfileAction::Lock(args) => {
            print_json(&json!({"ok": true, "profile_id": args.id, "locked": true}));
        }
    }
    Ok(())
}

async fn handle_relay(cmd: RelayCommand, paths: &Paths) -> Result<()> {
    let mut relays: RelayConfig = load_or_default(&paths.relays_file)?;
    match cmd.action {
        RelayAction::List => {
            print_json(&json!({"ok": true, "relays": relays}));
        }
        RelayAction::Add(args) => {
            let relay = normalize_relay_url(&args.url)?;
            match args.mode {
                RelayMode::Read => push_unique(&mut relays.read, &relay),
                RelayMode::Write => push_unique(&mut relays.write, &relay),
                RelayMode::Both => {
                    push_unique(&mut relays.read, &relay);
                    push_unique(&mut relays.write, &relay);
                }
            }
            save_json_pretty(&paths.relays_file, &relays)?;
            print_json(&json!({"ok": true, "relays": relays}));
        }
        RelayAction::Remove(args) => {
            let relay = normalize_relay_url(&args.url)?;
            relays.read.retain(|value| value != &relay);
            relays.write.retain(|value| value != &relay);
            if relays.home == relay {
                relays.home = relays
                    .read
                    .first()
                    .cloned()
                    .or_else(|| relays.write.first().cloned())
                    .unwrap_or_default();
            }
            save_json_pretty(&paths.relays_file, &relays)?;
            print_json(&json!({"ok": true, "relays": relays}));
        }
        RelayAction::SetHome(args) => {
            let home = normalize_relay_url(&args.url)?;
            relays.home = home.clone();
            push_unique(&mut relays.read, &home);
            push_unique(&mut relays.write, &home);
            save_json_pretty(&paths.relays_file, &relays)?;
            print_json(&json!({"ok": true, "relays": relays}));
        }
        RelayAction::Probe(args) => {
            let targets = if let Some(url) = args.url {
                vec![normalize_relay_url(&url)?]
            } else {
                let mut set = BTreeSet::new();
                if !relays.home.trim().is_empty() {
                    set.insert(relays.home.clone());
                }
                for relay in &relays.read {
                    if !relay.trim().is_empty() {
                        set.insert(relay.clone());
                    }
                }
                for relay in &relays.write {
                    if !relay.trim().is_empty() {
                        set.insert(relay.clone());
                    }
                }
                set.into_iter().collect()
            };
            if targets.is_empty() {
                print_json(&json!({"ok": true, "needs_relay": true, "probes": []}));
                return Ok(());
            }
            let mut results = Vec::new();
            for relay in targets {
                results.push(probe_relay(&relay).await);
            }
            print_json(&json!({"ok": true, "probes": results}));
        }
    }
    Ok(())
}

async fn handle_timeline(cmd: TimelineCommand, paths: &Paths) -> Result<()> {
    let relays: RelayConfig = load_or_default(&paths.relays_file)?;
    match cmd.action {
        TimelineAction::Fetch(args) => {
            let filter = build_filter(
                args.authors.as_deref(),
                args.kinds.as_deref(),
                args.search.as_deref(),
                args.since,
                args.until,
                args.limit,
                args.tag_p.as_deref(),
            );
            let targets = configured_read_targets(&relays, args.include_remotes);
            if targets.is_empty() {
                print_json(&json!({
                    "ok": true,
                    "needs_relay": true,
                    "events": [],
                    "per_relay": {},
                }));
                return Ok(());
            }

            let mut merged = Vec::<Event>::new();
            let mut seen = BTreeSet::new();
            let mut per_relay = BTreeMap::new();
            for relay in targets {
                let events = fetch_events_ws(&relay, &filter, 10)
                    .await
                    .unwrap_or_default();
                per_relay.insert(relay.clone(), events.len());
                for event in events {
                    if seen.insert(event.id.clone()) {
                        merged.push(event);
                    }
                }
            }
            merged.sort_by(|left, right| {
                right
                    .created_at
                    .cmp(&left.created_at)
                    .then_with(|| right.id.cmp(&left.id))
            });
            merged.truncate(args.limit);
            print_json(&json!({"ok": true, "events": merged, "per_relay": per_relay}));
        }
    }
    Ok(())
}

async fn handle_discover(cmd: DiscoverCommand, paths: &Paths) -> Result<()> {
    let relays: RelayConfig = load_or_default(&paths.relays_file)?;
    match cmd.action {
        DiscoverAction::Search(args) => {
            if relays.home.trim().is_empty() {
                print_json(&json!({"ok": true, "needs_relay": true, "events": []}));
                return Ok(());
            }
            let filter = build_filter(
                None,
                None,
                Some(args.term.as_str()),
                None,
                None,
                args.limit,
                None,
            );
            let events = fetch_events_ws(&relays.home, &filter, 10)
                .await
                .unwrap_or_default();
            print_json(&json!({"ok": true, "events": events}));
        }
        DiscoverAction::Count(args) => {
            if relays.home.trim().is_empty() {
                print_json(&json!({"ok": true, "needs_relay": true, "count": 0}));
                return Ok(());
            }
            let filter = build_filter(None, None, args.term.as_deref(), None, None, 1_000, None);
            let count = count_events_ws(&relays.home, &filter, 10)
                .await
                .unwrap_or(0);
            print_json(&json!({"ok": true, "count": count}));
        }
        DiscoverAction::RelayInfo(args) => {
            let relay = normalize_relay_url(&args.url)?;
            let result = probe_relay(&relay).await;
            print_json(&json!({"ok": true, "relay": result}));
        }
    }
    Ok(())
}

fn handle_compose(cmd: ComposeCommand, paths: &Paths) -> Result<()> {
    match cmd.action {
        ComposeAction::Note(args) => {
            let tags = args
                .tags
                .into_iter()
                .map(|value| Tag(vec!["t".into(), value]))
                .collect::<Vec<_>>();
            let event = event_template(1, args.content, tags);
            maybe_save_draft(paths, args.draft.as_deref(), "note", &event)?;
            print_json(&json!({"ok": true, "event": event}));
        }
        ComposeAction::Reply(args) => {
            validate_hex(&args.event_id, 64, "event_id")?;
            let tags = vec![Tag(vec!["e".into(), args.event_id])];
            let event = event_template(1, args.content, tags);
            maybe_save_draft(paths, args.draft.as_deref(), "reply", &event)?;
            print_json(&json!({"ok": true, "event": event}));
        }
        ComposeAction::Longform(args) => {
            if args.title.trim().is_empty() {
                bail!("long-form title cannot be empty");
            }
            if args.identifier.trim().is_empty() {
                bail!("long-form identifier cannot be empty");
            }
            let mut tags = vec![
                Tag(vec!["d".into(), args.identifier]),
                Tag(vec!["title".into(), args.title]),
            ];
            if let Some(summary) = args.summary {
                tags.push(Tag(vec!["summary".into(), summary]));
            }
            let event = event_template(30_023, args.content, tags);
            maybe_save_draft(paths, args.draft.as_deref(), "longform", &event)?;
            print_json(&json!({"ok": true, "event": event}));
        }
        ComposeAction::FileMetadata(args) => {
            validate_hex(&args.hash, 64, "hash")?;
            let tags = vec![
                Tag(vec!["url".into(), args.url]),
                Tag(vec!["x".into(), args.hash]),
                Tag(vec!["m".into(), args.mime]),
                Tag(vec!["size".into(), args.size.to_string()]),
            ];
            let event = event_template(1_063, String::new(), tags);
            maybe_save_draft(paths, args.draft.as_deref(), "file-metadata", &event)?;
            print_json(&json!({"ok": true, "event": event}));
        }
        ComposeAction::Delete(args) => {
            validate_hex(&args.event_id, 64, "event_id")?;
            let mut tags = vec![Tag(vec!["e".into(), args.event_id])];
            if let Some(reason) = args.reason {
                tags.push(Tag(vec!["reason".into(), reason]));
            }
            let event = event_template(5, String::new(), tags);
            maybe_save_draft(paths, args.draft.as_deref(), "delete", &event)?;
            print_json(&json!({"ok": true, "event": event}));
        }
        ComposeAction::ListDrafts => {
            let drafts = list_drafts(paths)?;
            print_json(&json!({"ok": true, "drafts": drafts}));
        }
        ComposeAction::Preview(args) => {
            let draft = load_draft(paths, &args.draft)?;
            print_json(&json!({"ok": true, "draft": draft}));
        }
        ComposeAction::SignDraft(args) => {
            let index: ProfileIndex = load_or_default(&paths.profiles_file)?;
            let (profile, secret_key) =
                resolve_profile_secret(&index, args.profile_id.as_deref(), &args.password)?;
            let mut draft = load_draft(paths, &args.draft)?;
            sign_event(&mut draft.event, &secret_key)?;
            draft.updated_at = unix_now();
            save_draft(paths, &draft)?;
            print_json(&json!({"ok": true, "profile": profile.id, "event": draft.event}));
        }
    }
    Ok(())
}

async fn handle_publish(cmd: PublishCommand, paths: &Paths) -> Result<()> {
    let relays: RelayConfig = load_or_default(&paths.relays_file)?;
    let index: ProfileIndex = load_or_default(&paths.profiles_file)?;

    let (mut event, profile_id, password, explicit_relays) = match cmd.action {
        PublishAction::EventFile(args) => {
            let text = fs::read_to_string(&args.path)
                .with_context(|| format!("reading event file {}", args.path.display()))?;
            let event: Event = serde_json::from_str(&text)?;
            (event, args.profile_id, args.password, args.relay)
        }
        PublishAction::Draft(args) => {
            let draft = load_draft(paths, &args.draft)?;
            (draft.event, args.profile_id, args.password, args.relay)
        }
    };

    let (_, secret_key) = resolve_profile_secret(&index, profile_id.as_deref(), &password)?;
    if event.id.is_empty() || event.sig.is_empty() {
        sign_event(&mut event, &secret_key)?;
    }

    let targets = if explicit_relays.is_empty() {
        configured_write_targets(&relays)
    } else {
        explicit_relays
            .iter()
            .map(|relay| normalize_relay_url(relay))
            .collect::<Result<Vec<_>>>()?
    };
    if targets.is_empty() {
        bail!("no write relays configured");
    }

    let mut results = Vec::new();
    for relay in targets {
        results.push(publish_event_with_retry(&relay, &event, 1).await);
    }
    print_json(&json!({"ok": true, "event_id": event.id, "results": results}));
    Ok(())
}

fn handle_library(cmd: LibraryCommand, paths: &Paths) -> Result<()> {
    let mut library: LibraryIndex = load_or_default(&paths.library_file)?;
    match cmd.action {
        LibraryAction::List(args) => {
            if let Some(bucket) = args.bucket {
                let events = match bucket.as_str() {
                    "starred" => json!(library.starred),
                    "saved" => json!(library.saved),
                    "liked" => json!(library.liked),
                    "commented" => json!(library.commented),
                    _ => bail!("unknown bucket: {bucket}"),
                };
                print_json(&json!({"ok": true, "bucket": bucket, "events": events}));
            } else {
                print_json(&json!({"ok": true, "library": library}));
            }
        }
        LibraryAction::Star(args) => {
            push_unique(&mut library.starred, &args.event_id);
            save_json_pretty(&paths.library_file, &library)?;
            print_json(&json!({"ok": true, "starred": library.starred}));
        }
        LibraryAction::Unstar(args) => {
            library.starred.retain(|id| id != &args.event_id);
            save_json_pretty(&paths.library_file, &library)?;
            print_json(&json!({"ok": true, "starred": library.starred}));
        }
        LibraryAction::Save(args) => {
            push_unique(&mut library.saved, &args.event_id);
            save_json_pretty(&paths.library_file, &library)?;
            print_json(&json!({"ok": true, "saved": library.saved}));
        }
        LibraryAction::Unsave(args) => {
            library.saved.retain(|id| id != &args.event_id);
            save_json_pretty(&paths.library_file, &library)?;
            print_json(&json!({"ok": true, "saved": library.saved}));
        }
        LibraryAction::IngestAuthored(args) => {
            let source = fs::File::open(&args.path)
                .with_context(|| format!("opening {}", args.path.display()))?;
            if let Some(parent) = paths.authored_cache.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut sink = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&paths.authored_cache)?;
            let mut added = 0usize;
            for line in BufReader::new(source).lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let _: Event = serde_json::from_str(&line)?;
                writeln!(sink, "{line}")?;
                added = added.saturating_add(1);
            }
            print_json(&json!({"ok": true, "added": added}));
        }
        LibraryAction::Reindex => {
            let (liked, commented) = derive_library_from_authored(&paths.authored_cache)?;
            library.liked = liked;
            library.commented = commented;
            save_json_pretty(&paths.library_file, &library)?;
            print_json(&json!({"ok": true, "library": library}));
        }
    }
    Ok(())
}

async fn handle_media(cmd: MediaCommand, paths: &Paths) -> Result<()> {
    match cmd.action {
        MediaAction::Nip94Template(args) => {
            validate_hex(&args.hash, 64, "hash")?;
            let mut tags = vec![
                Tag(vec!["url".into(), args.url]),
                Tag(vec!["x".into(), args.hash]),
                Tag(vec!["m".into(), args.mime]),
                Tag(vec!["size".into(), args.size.to_string()]),
            ];
            if let Some(name) = args.name {
                tags.push(Tag(vec!["name".into(), name]));
            }
            let event = event_template(1_063, String::new(), tags);
            print_json(&json!({"ok": true, "event": event}));
        }
        MediaAction::UploadNip96(args) => {
            let relay = normalize_relay_url(&args.relay_url)?;
            let endpoint = relay_to_http_root(&relay)?.join("files")?;
            let file_bytes =
                fs::read(&args.file).with_context(|| format!("reading {}", args.file.display()))?;
            let filename = args
                .file
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| anyhow!("file path has no valid filename"))?
                .to_string();

            let mut req = reqwest::Client::new().post(endpoint.clone()).multipart(
                multipart::Form::new().part(
                    "file",
                    multipart::Part::bytes(file_bytes.clone()).file_name(filename),
                ),
            );

            if let Some(password) = args.password {
                let index: ProfileIndex = load_or_default(&paths.profiles_file)?;
                let (_, secret) =
                    resolve_profile_secret(&index, args.profile_id.as_deref(), &password)?;
                let payload_hash = hex::encode(Sha256::digest(&file_bytes));
                let auth = nostr_shared::nip98::build_http_auth_header(
                    &secret,
                    "POST",
                    endpoint.as_str(),
                    Some(&payload_hash),
                    None,
                )?;
                req = req.header("Authorization", auth);
            }

            let response = req.send().await?;
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            print_json(&json!({
                "ok": true,
                "status": status,
                "body": body,
                "endpoint": endpoint.as_str()
            }));
        }
    }
    Ok(())
}

fn handle_doctor(paths: &Paths) -> Result<()> {
    let profiles: ProfileIndex = load_or_default(&paths.profiles_file)?;
    let relays: RelayConfig = load_or_default(&paths.relays_file)?;
    let library: LibraryIndex = load_or_default(&paths.library_file)?;
    let manifest = parity::manifest()?;

    print_json(&json!({
        "ok": true,
        "paths": {
            "config_root": paths.config_root,
            "state_root": paths.state_root,
            "profiles_file": paths.profiles_file,
            "relays_file": paths.relays_file,
            "drafts_dir": paths.drafts_dir,
            "library_file": paths.library_file,
            "authored_cache": paths.authored_cache,
            "cursors_dir": paths.cursors_dir,
        },
        "profiles": {
            "count": profiles.profiles.len(),
            "active_profile": profiles.active_profile,
        },
        "relays": relays,
        "relay_ready": relay_configured(&relays),
        "library_counts": {
            "starred": library.starred.len(),
            "saved": library.saved.len(),
            "liked": library.liked.len(),
            "commented": library.commented.len(),
        },
        "nip_parity": manifest,
    }));

    Ok(())
}

fn build_filter(
    authors: Option<&str>,
    kinds: Option<&str>,
    search: Option<&str>,
    since: Option<u64>,
    until: Option<u64>,
    limit: usize,
    tag_p: Option<&str>,
) -> Value {
    let mut object = serde_json::Map::new();
    if let Some(authors) = authors {
        let values = split_csv_strings(authors);
        if !values.is_empty() {
            object.insert(
                "authors".into(),
                Value::Array(values.into_iter().map(Value::String).collect()),
            );
        }
    }
    if let Some(kinds) = kinds {
        let values = split_csv_u32(kinds);
        if !values.is_empty() {
            object.insert(
                "kinds".into(),
                Value::Array(
                    values
                        .into_iter()
                        .map(|value| Value::Number(value.into()))
                        .collect(),
                ),
            );
        }
    }
    if let Some(search) = search.filter(|value| !value.trim().is_empty()) {
        object.insert("search".into(), Value::String(search.trim().to_string()));
    }
    if let Some(since) = since {
        object.insert("since".into(), Value::Number(since.into()));
    }
    if let Some(until) = until {
        object.insert("until".into(), Value::Number(until.into()));
    }
    if let Some(tag_p) = tag_p {
        let values = split_csv_strings(tag_p);
        if !values.is_empty() {
            object.insert(
                "#p".into(),
                Value::Array(values.into_iter().map(Value::String).collect()),
            );
        }
    }
    object.insert("limit".into(), Value::Number((limit as u64).into()));
    Value::Object(object)
}

async fn fetch_events_ws(relay: &str, filter: &Value, timeout_secs: u64) -> Result<Vec<Event>> {
    let (mut socket, _) = connect_async(relay).await?;
    let sub = format!("onstr-{}", short_random_id());
    let req = json!(["REQ", sub, filter]);
    socket.send(Message::Text(req.to_string())).await?;

    let mut events = Vec::new();
    loop {
        let next = timeout(Duration::from_secs(timeout_secs), socket.next())
            .await
            .map_err(|_| anyhow!("timeout waiting for relay"))?;
        let Some(message) = next else {
            break;
        };
        let message = message?;
        match message {
            Message::Text(text) => {
                let parsed: Value = match serde_json::from_str(&text) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let Some(array) = parsed.as_array() else {
                    continue;
                };
                let Some(kind) = array.first().and_then(Value::as_str) else {
                    continue;
                };
                match kind {
                    "EVENT" if array.len() >= 3 => {
                        if let Ok(event) = serde_json::from_value::<Event>(array[2].clone()) {
                            events.push(event);
                        }
                    }
                    "EOSE" => break,
                    _ => {}
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    let _ = socket
        .send(Message::Text(json!(["CLOSE", sub]).to_string()))
        .await;
    Ok(events)
}

async fn count_events_ws(relay: &str, filter: &Value, timeout_secs: u64) -> Result<usize> {
    let (mut socket, _) = connect_async(relay).await?;
    let sub = format!("onstr-count-{}", short_random_id());
    socket
        .send(Message::Text(json!(["COUNT", sub, filter]).to_string()))
        .await?;

    loop {
        let next = timeout(Duration::from_secs(timeout_secs), socket.next())
            .await
            .map_err(|_| anyhow!("timeout waiting for relay"))?;
        let Some(message) = next else {
            break;
        };
        let message = message?;
        if let Message::Text(text) = message {
            let parsed: Value = match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let Some(array) = parsed.as_array() else {
                continue;
            };
            if array.first().and_then(Value::as_str) == Some("COUNT") {
                let count = array
                    .get(2)
                    .and_then(|value| value.get("count"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                return Ok(count);
            }
        }
    }
    Ok(0)
}

#[derive(Debug, Serialize)]
struct RelayProbeResult {
    relay: String,
    info_url: String,
    reachable: bool,
    status: Option<u16>,
    supported_nips: Vec<u32>,
    relay_name: Option<String>,
    error: Option<String>,
}

async fn probe_relay(relay: &str) -> RelayProbeResult {
    let info_url = match relay_to_http_root(relay) {
        Ok(url) => url,
        Err(error) => {
            return RelayProbeResult {
                relay: relay.to_string(),
                info_url: String::new(),
                reachable: false,
                status: None,
                supported_nips: vec![],
                relay_name: None,
                error: Some(error.to_string()),
            }
        }
    };

    let response = reqwest::Client::new()
        .get(info_url.clone())
        .header("Accept", "application/nostr+json")
        .send()
        .await;
    match response {
        Ok(response) => {
            let status = response.status().as_u16();
            let reachable = response.status().is_success();
            let body = response.text().await.unwrap_or_default();
            let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
            let supported_nips = parsed
                .get("supported_nips")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_u64)
                        .map(|value| value as u32)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let relay_name = parsed
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string);
            RelayProbeResult {
                relay: relay.to_string(),
                info_url: info_url.to_string(),
                reachable,
                status: Some(status),
                supported_nips,
                relay_name,
                error: None,
            }
        }
        Err(error) => RelayProbeResult {
            relay: relay.to_string(),
            info_url: info_url.to_string(),
            reachable: false,
            status: None,
            supported_nips: vec![],
            relay_name: None,
            error: Some(error.to_string()),
        },
    }
}

#[derive(Debug, Serialize)]
struct PublishResult {
    relay: String,
    accepted: bool,
    attempts: u32,
    message: String,
}

async fn publish_event_with_retry(relay: &str, event: &Event, retries: u32) -> PublishResult {
    let mut attempts = 0u32;
    loop {
        attempts = attempts.saturating_add(1);
        match publish_once(relay, event).await {
            Ok(message) => {
                return PublishResult {
                    relay: relay.to_string(),
                    accepted: true,
                    attempts,
                    message,
                }
            }
            Err(error) => {
                if attempts > retries {
                    return PublishResult {
                        relay: relay.to_string(),
                        accepted: false,
                        attempts,
                        message: error.to_string(),
                    };
                }
                sleep(Duration::from_millis(250)).await;
            }
        }
    }
}

async fn publish_once(relay: &str, event: &Event) -> Result<String> {
    let (mut socket, _) = connect_async(relay).await?;
    socket
        .send(Message::Text(json!(["EVENT", event]).to_string()))
        .await?;

    loop {
        let next = timeout(Duration::from_secs(8), socket.next())
            .await
            .map_err(|_| anyhow!("timeout waiting for relay ACK"))?;
        let Some(message) = next else {
            bail!("relay closed before ACK");
        };
        let message = message?;
        if let Message::Text(text) = message {
            let parsed: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!([]));
            let Some(array) = parsed.as_array() else {
                continue;
            };
            if array.first().and_then(Value::as_str) == Some("OK") {
                let accepted = array.get(2).and_then(Value::as_bool).unwrap_or(false);
                let message = array
                    .get(3)
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if accepted {
                    return Ok(message);
                }
                bail!("relay rejected event: {message}");
            }
        }
    }
}

fn maybe_save_draft(
    paths: &Paths,
    name: Option<&str>,
    category: &str,
    event: &Event,
) -> Result<()> {
    if let Some(name) = name {
        let draft = DraftRecord {
            name: normalize_draft_name(name),
            category: category.to_string(),
            event: event.clone(),
            updated_at: unix_now(),
        };
        save_draft(paths, &draft)?;
    }
    Ok(())
}

fn save_draft(paths: &Paths, draft: &DraftRecord) -> Result<()> {
    fs::create_dir_all(&paths.drafts_dir)?;
    let path = paths
        .drafts_dir
        .join(format!("{}.json", normalize_draft_name(&draft.name)));
    save_json_pretty(path, draft)
}

fn load_draft(paths: &Paths, name: &str) -> Result<DraftRecord> {
    let path = paths
        .drafts_dir
        .join(format!("{}.json", normalize_draft_name(name)));
    load_json(path)
}

fn list_drafts(paths: &Paths) -> Result<Vec<DraftRecord>> {
    let mut drafts = Vec::new();
    if !paths.drafts_dir.exists() {
        return Ok(drafts);
    }
    for entry in fs::read_dir(&paths.drafts_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let draft: DraftRecord = load_json(path)?;
        drafts.push(draft);
    }
    drafts.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(drafts)
}

fn derive_library_from_authored(path: &Path) -> Result<(Vec<String>, Vec<String>)> {
    if !path.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    let file = fs::File::open(path)?;
    let mut liked = Vec::new();
    let mut commented = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = serde_json::from_str(&line)?;
        if event.kind == 7 {
            if let Some(id) = first_tag_value(&event, "e") {
                push_unique(&mut liked, &id);
            }
        }
        if event.kind == 1 && first_tag_value(&event, "e").is_some() {
            push_unique(&mut commented, &event.id);
        }
    }
    Ok((liked, commented))
}

fn resolve_profile_record<'a>(
    index: &'a ProfileIndex,
    profile_id: Option<&str>,
) -> Result<&'a ProfileRecord> {
    if let Some(profile_id) = profile_id {
        return index
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| anyhow!("profile not found: {profile_id}"));
    }

    let active = index
        .active_profile
        .as_deref()
        .ok_or_else(|| anyhow!("no active profile set"))?;
    index
        .profiles
        .iter()
        .find(|profile| profile.id == active)
        .ok_or_else(|| anyhow!("active profile not found"))
}

fn resolve_profile_secret(
    index: &ProfileIndex,
    profile_id: Option<&str>,
    password: &str,
) -> Result<(ProfileRecord, String)> {
    let profile = resolve_profile_record(index, profile_id)?.clone();
    let secret_key = decrypt_secret_key_nip49(&profile.ncryptsec, password)?;
    Ok((profile, secret_key))
}

fn ensure_profile_exists(index: &ProfileIndex, profile_id: &str) -> Result<()> {
    if index
        .profiles
        .iter()
        .any(|profile| profile.id == profile_id)
    {
        Ok(())
    } else {
        bail!("profile not found: {profile_id}")
    }
}

fn encrypt_secret_key_nip49(secret_key_hex: &str, password: &str) -> Result<String> {
    validate_secret_hex(secret_key_hex)?;
    let plaintext = hex::decode(secret_key_hex)?;

    // NIP-49 compatible envelope fields: version/log_n/salt/nonce/ciphertext.
    let version: u8 = 0x02;
    let log_n: u8 = 15;
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);

    let params = ScryptParams::new(log_n, 8, 1, 32)?;
    let mut key = [0u8; 32];
    scrypt(password.as_bytes(), &salt, &params, &mut key)?;

    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| anyhow!("failed to encrypt ncryptsec payload"))?;

    let mut bytes = Vec::with_capacity(2 + salt.len() + nonce.len() + ciphertext.len());
    bytes.push(version);
    bytes.push(log_n);
    bytes.extend_from_slice(&salt);
    bytes.extend_from_slice(&nonce);
    bytes.extend_from_slice(&ciphertext);

    let hrp = Hrp::parse("ncryptsec")?;
    Ok(bech32::encode::<Bech32>(hrp, &bytes)?)
}

fn decrypt_secret_key_nip49(ncryptsec: &str, password: &str) -> Result<String> {
    let (hrp, bytes) = bech32::decode(ncryptsec)?;
    if hrp != Hrp::parse("ncryptsec")? {
        bail!("invalid ncryptsec prefix");
    }
    if bytes.len() < 2 + 16 + 24 + 16 {
        bail!("invalid ncryptsec payload");
    }

    let version = bytes[0];
    let log_n = bytes[1];
    if version != 0x02 {
        bail!("unsupported ncryptsec version");
    }

    let salt_start = 2;
    let nonce_start = salt_start + 16;
    let cipher_start = nonce_start + 24;

    let salt = &bytes[salt_start..nonce_start];
    let nonce = &bytes[nonce_start..cipher_start];
    let ciphertext = &bytes[cipher_start..];

    let params = ScryptParams::new(log_n, 8, 1, 32)?;
    let mut key = [0u8; 32];
    scrypt(password.as_bytes(), salt, &params, &mut key)?;

    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
    let plaintext = cipher
        .decrypt(XNonce::from_slice(nonce), ciphertext.as_ref())
        .map_err(|_| anyhow!("failed to decrypt ncryptsec payload"))?;

    if plaintext.len() != 32 {
        bail!("decrypted key length is invalid");
    }
    Ok(hex::encode(plaintext))
}

fn pubkey_from_secret(secret_key_hex: &str) -> Result<String> {
    validate_secret_hex(secret_key_hex)?;
    let secp = Secp256k1::new();
    let secret = SecretKey::from_slice(&hex::decode(secret_key_hex)?)?;
    let keypair = Keypair::from_secret_key(&secp, &secret);
    Ok(hex::encode(keypair.x_only_public_key().0.serialize()))
}

fn validate_secret_hex(secret_key_hex: &str) -> Result<()> {
    validate_hex(secret_key_hex, 64, "secret_key")
}

fn validate_hex(value: &str, expected_len: usize, field: &str) -> Result<()> {
    if value.len() != expected_len || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{field} must be a {expected_len}-char hex string");
    }
    Ok(())
}

fn first_tag_value(event: &Event, name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find_map(|Tag(fields)| match fields.as_slice() {
            [tag, value, ..] if tag == name => Some(value.clone()),
            _ => None,
        })
}

fn event_template(kind: u32, content: String, tags: Vec<Tag>) -> Event {
    Event {
        id: String::new(),
        pubkey: String::new(),
        kind,
        created_at: unix_now(),
        tags,
        content,
        sig: String::new(),
    }
}

fn relay_to_http_root(relay: &str) -> Result<Url> {
    let relay = normalize_relay_url(relay)?;
    let url = Url::parse(&relay)?;
    let scheme = match url.scheme() {
        "ws" => "http",
        "wss" => "https",
        "http" => "http",
        "https" => "https",
        other => bail!("unsupported relay scheme: {other}"),
    };
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("relay URL has no host"))?;
    let mut root = format!("{scheme}://{host}");
    if let Some(port) = url.port() {
        root.push(':');
        root.push_str(&port.to_string());
    }
    root.push('/');
    Ok(Url::parse(&root)?)
}

fn normalize_relay_url(url: &str) -> Result<String> {
    let parsed = Url::parse(url.trim())?;
    match parsed.scheme() {
        "ws" | "wss" => {}
        other => bail!("relay URL must use ws or wss, got {other}"),
    }
    if parsed.host_str().is_none() {
        bail!("relay URL has no host");
    }
    Ok(parsed.to_string().trim_end_matches('/').to_string())
}

fn relay_configured(relays: &RelayConfig) -> bool {
    !relays.home.trim().is_empty()
        || relays.read.iter().any(|relay| !relay.trim().is_empty())
        || relays.write.iter().any(|relay| !relay.trim().is_empty())
}

fn configured_read_targets(relays: &RelayConfig, include_remotes: bool) -> Vec<String> {
    let mut targets = Vec::new();
    if !relays.home.trim().is_empty() {
        targets.push(relays.home.clone());
    }
    if include_remotes {
        for relay in &relays.read {
            if relay.trim().is_empty() || targets.iter().any(|value| value == relay) {
                continue;
            }
            targets.push(relay.clone());
        }
    }
    targets
}

fn configured_write_targets(relays: &RelayConfig) -> Vec<String> {
    let mut targets = Vec::new();
    for relay in &relays.write {
        if relay.trim().is_empty() || targets.iter().any(|value| value == relay) {
            continue;
        }
        targets.push(relay.clone());
    }
    targets
}

fn split_csv_strings(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn split_csv_u32(value: &str) -> Vec<u32> {
    value
        .split(',')
        .map(str::trim)
        .filter_map(|value| value.parse::<u32>().ok())
        .collect()
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn normalize_draft_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        short_random_id()
    } else {
        out
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn short_random_id() -> String {
    let mut bytes = [0u8; 6];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn load_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(serde_json::from_str(&text)?)
}

fn load_or_default<T>(path: impl AsRef<Path>) -> Result<T>
where
    T: DeserializeOwned + Default,
{
    let path = path.as_ref();
    if !path.exists() {
        return Ok(T::default());
    }
    load_json(path)
}

fn save_json_pretty(path: impl AsRef<Path>, value: impl Serialize) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(&value)?;
    fs::write(path, data).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
    );
}

#[cfg(test)]
mod tests {
    use super::RelayConfig;

    #[test]
    fn relay_default_does_not_assume_localhost_service() {
        let relays = RelayConfig::default();

        assert!(relays.home.is_empty());
        assert!(relays.read.is_empty());
        assert!(relays.write.is_empty());
    }
}
