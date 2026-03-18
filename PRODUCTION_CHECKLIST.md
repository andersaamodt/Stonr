# Stonr Production Checklist

Last updated: 2026-03-18

This checklist tracks the real backend state, not just what the desktop app exposes.

## Core Relay Surface

- [x] File-backed event storage with on-disk indexes and reindexing
- [x] Relay capability toggles enforced for profile, query, publish, live subscriptions, count, tag filters, search, and mirroring
- [x] Delete tombstones enforced across ingest/query paths (`NIP-09`)
- [x] Expiration filtering enforced across ingest/query paths (`NIP-40`)
- [x] HTTP `/healthz`
- [x] HTTP `/mirror-health`
- [x] HTTP `/query`
- [x] HTTP `/count`
- [x] WebSocket `REQ`
- [x] WebSocket `COUNT`
- [x] WebSocket `EVENT` publish
- [x] WebSocket `CLOSE`
- [x] Live WebSocket subscription fanout for newly published events
- [x] Live WebSocket subscription fanout for mirrored upstream events
- [x] Empty-filter queries return recent stored events instead of an empty set
- [x] Relay-side text search over stored event content
- [x] NIP-11 relay info reflects configured name/description and active basic relay capabilities

## Mirroring

- [x] Broad upstream mirror mode
- [x] Mirror stays subscribed after `EOSE`
- [x] Mirror reconnect loop after upstream disconnects
- [x] Author/kind/topic filter support for broad mirroring
- [x] One-site mirror mode for a single site author
- [x] Optional one-site comment mirroring by `#a` reference
- [x] Per-upstream mirror health, lag, and last-success visibility
- [ ] Repair tooling for broken cursors / replay from a chosen checkpoint

## Storage Safety

- [x] Retention cap by total stored event count
- [x] Retention cap by total stored event bytes
- [x] Oldest-first pruning
- [x] Private-message filtering for known encrypted private-message kinds
- [x] Stats cache refresh from the real event tree
- [ ] Clear operator warnings/metrics when retention is not meeting expectations
- [ ] Backup/restore documentation and tested operational workflow

## Desktop Control App

- [x] First-class mirror-mode controls
- [x] First-class background-on-close controls
- [x] Menu bar / tray host support on macOS and Linux
- [x] Event browser with search and live refresh
- [ ] Surface real mirror/ingest health in the UI instead of only stored-event snapshots
- [ ] Better operator-facing error presentation for long-running relay failures

## Nostr-Blog Backend Readiness

- [x] Mirror site-author `kind 30023` long-form posts
- [x] Mirror `kind 1` comments by `#a` reference to mirrored posts
- [x] Serve queried site content from a local file-backed store
- [ ] Add integration tests against `nostr-blog`’s actual mirror-mode expectations
- [ ] Add a documented first-class `nostr-blog` preset/setup flow
- [ ] Decide and document whether `nostr-blog` must copy authoritative content out of Stonr or can rely on relay retention settings alone

## Remaining Production Blockers

- [ ] Backend enforcement for the remaining desktop-exposed auth, moderation, and file/blob policy toggles
- [ ] End-to-end relay authentication (`NIP-42`) instead of UI-only controls
- [ ] Backend file/blob policy enforcement for all desktop-exposed controls
- [ ] Consistent capability advertisement across the remaining unimplemented relay/file/auth surfaces
- [ ] Structured production logging
- [ ] Load/performance testing on large stores and long-running mirror sessions

## Explicitly Not Planned Right Now

- [ ] Relay-local pinning, unless a concrete NIP-backed model is chosen first
