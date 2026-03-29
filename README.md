# Stonr + Onstr Monorepo

This repository contains two production projects and one shared protocol crate:

- `stonr/` - file-backed Nostr relay
- `onstr/` - file-backed Nostr client (Onstr)
- `shared/` - shared Nostr protocol primitives and parity contracts

## App entrypoints

- `stonr/app/` - Stonr control GUI
- `onstr/app/` - Onstr client GUI
- `shared/themes/` - centralized Wizardry theme source (symlinked by both apps)

## Workspace commands

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Binary-specific commands

```bash
# Relay
cargo run --manifest-path stonr/Cargo.toml -- --help

# Client engine
cargo run --manifest-path onstr/core/Cargo.toml -- --help
```
