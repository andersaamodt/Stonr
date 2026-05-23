# Stonr

Stonr is a personal Nostr relay and file server. It stores events and blobs on
plain files, can mirror selected upstream relays, and includes a desktop control
app for operating a local relay without hand-editing every setting.

It is built for people who want a relay they can inspect, back up, move between
machines, and run under their own control. The store is ordinary directories,
JSON files, indexes, cursors, and admin lists rather than a database.

## What Stonr Does

- Runs a NIP-01 relay over WebSocket for reads, writes, counts, auth, and live
  subscriptions.
- Exposes HTTP health, readiness, query, retention, mirror-health, and relay info
  endpoints.
- Stores Nostr events as standalone JSON files with file-backed indexes for
  authors, kinds, tags, and search.
- Mirrors upstream relays, optionally through Tor, with filters for authors,
  kinds, topics, and one-site blog publishing.
- Serves file metadata and blob APIs for NIP-94, NIP-96, NIP-98, and Blossom
  style upload/download flows.
- Enforces moderation lists, rate limits, retention caps, delete tombstones,
  expiration, upload limits, MIME rules, and owner/follow/pin retention policy.
- Generates systemd, launchd, Caddy, and Nginx config snippets from the exact
  relay environment you are using.

The companion Onstr app in this repository is a local file-backed Nostr client
that shares protocol code and theme assets with Stonr.

## Desktop Control App

Stonr includes a Wizardry-style desktop control app in `stonr/app/`. In the
native desktop host it can:

- create and edit a relay `.env`
- start, stop, and restart `stonr serve`
- inspect relay status, logs, mirror health, retention health, and stored events
- edit file-backed moderation lists
- configure mirroring, Blossom/file limits, retention, auth, and startup
- manage macOS login autostart through launchd

The same app can render as hosted web UI, but hosted web mode cannot control the
local relay process unless it is running inside the Wizardry desktop bridge.

## Quick Start

Build the relay:

```bash
cargo build --manifest-path stonr/Cargo.toml
```

Create a local relay environment:

```bash
cat > /tmp/stonr.env <<'EOF'
STORE_ROOT=/tmp/stonr-store
BIND_HTTP=127.0.0.1:7777
BIND_WS=127.0.0.1:7778
VERIFY_SIG=1
EOF
```

Initialize and run it:

```bash
cargo run --manifest-path stonr/Cargo.toml -- --env /tmp/stonr.env init
cargo run --manifest-path stonr/Cargo.toml -- --env /tmp/stonr.env serve
```

Check the HTTP surface from another terminal:

```bash
curl http://127.0.0.1:7777/healthz
curl "http://127.0.0.1:7777/query?limit=5"
```

For a long-running relay, put the `.env` under your normal config directory and
use a supervisor instead of `/tmp` paths.

## Configuration

Stonr reads runtime settings from a `.env` file passed with `--env`.

| Variable | Purpose | Example |
| --- | --- | --- |
| `STORE_ROOT` | File-backed relay store | `/srv/stonr` |
| `BIND_HTTP` | HTTP API address | `127.0.0.1:7777` |
| `BIND_WS` | Nostr WebSocket address | `127.0.0.1:7778` |
| `VERIFY_SIG` | Verify event signatures on ingest | `1` |
| `RELAYS_UPSTREAM` | Comma-separated relays to mirror | `wss://relay.example` |
| `TOR_SOCKS` | Optional Tor SOCKS proxy | `127.0.0.1:9050` |
| `FILTER_AUTHORS` | Mirror only these authors | `npub1...,npub2...` |
| `FILTER_KINDS` | Mirror only these event kinds | `1,30023` |
| `FILTER_TAG_T` | Mirror only these `#t` topics | `essay,notes` |
| `OWNER_PUBKEYS` | Owners with privileged retention/write policy | `npub1owner...` |
| `FOLLOW_PUBKEYS` | Followed authors to mirror and retain | `npub1alice...` |
| `PIN_EVENT_IDS` | Exact events to retain | `4a9f...` |

The desktop app exposes the common settings, but the `.env` remains the source
of truth.

## Common Commands

```bash
# Initialize the store layout
stonr --env /path/to/relay.env init

# Start HTTP and WebSocket servers
stonr --env /path/to/relay.env serve

# Rebuild indexes from stored events and blobs
stonr --env /path/to/relay.env reindex

# Check store health and policy state
stonr --env /path/to/relay.env verify --sample 1000
stonr --env /path/to/relay.env retention-status

# Back up and restore authoritative store data
stonr --env /path/to/relay.env backup --destination /path/to/backup
stonr --env /path/to/relay.env restore --source /path/to/backup

# Generate supervisor and reverse-proxy config
stonr --env /path/to/relay.env print-service --manager systemd --label stonr
stonr --env /path/to/relay.env print-service --manager launchd --label dev.stonr.relay
stonr --env /path/to/relay.env print-proxy --manager caddy --domain relay.example.com
stonr --env /path/to/relay.env print-proxy --manager nginx --domain relay.example.com
```

## Deployment Notes

Production Stonr should run under an external supervisor. The CLI can render
matching service files for systemd and launchd from the binary and `.env` you
are actually using.

Keep the relay bound to localhost or a private interface unless you are fronting
it with a real reverse proxy. For public HTTPS, use the generated Caddy or Nginx
config as a starting point and keep `BIND_HTTP` and `BIND_WS` private.

For Tor, configure the relay normally and point `TOR_SOCKS` at your local Tor
SOCKS listener. See the onion guide for hidden-service wiring.

## Documentation

- [API reference](stonr/docs/api.md)
- [Deployment and supervision](stonr/docs/deployment.md)
- [Mirroring](stonr/docs/mirroring.md)
- [Nostr-blog preset](stonr/docs/nostr-blog.md)
- [Operations, backup, restore, and retention](stonr/docs/operations.md)
- [Tor onion deployment](stonr/docs/onion.md)
- [Performance checks](stonr/docs/performance.md)
- [Reverse proxy setup](stonr/docs/reverse-proxy.md)
- [Production checklist](stonr/PRODUCTION_CHECKLIST.md)

## Repository Layout

```text
stonr/                 Stonr relay binary, control app, and relay docs
onstr/                 Onstr companion client app and engine
shared/nostr-shared/   Shared Nostr protocol primitives
shared/themes/         Shared Wizardry app themes
```

## Development

Run the normal Rust workspace checks before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features --quiet
```

The desktop app backend lives at `stonr/app/scripts/stonr-control-backend.sh`.
GUI actions use explicit argv allowlists through the Wizardry bridge, and local
state should stay in plain config/state files instead of generated artifacts in
the repository.

## License

Stonr is licensed under the Open Wizardry License 3.0. See [LICENSE](LICENSE).
