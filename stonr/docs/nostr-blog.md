# Stonr As A `nostr-blog` Backend

Stonr can run as a reduced one-site mirror backend for `nostr-blog`.

## What The Preset Does

The desktop app's `Apply nostr-blog preset` button configures Stonr for the
common `nostr-blog` shape:

- one-site mirror mode
- mirrored site comments by `#a` reference
- read/count/tag-query support kept on
- local publish turned off
- private-message filtering kept on
- broad mirror filters cleared
- default upstream relays seeded when none are configured yet

After applying the preset, set `Site author pubkey` to the website author's hex
pubkey.

## Mirror Model

The preset assumes the same data flow `nostr-blog` already uses:

- mirror `kind 30023` long-form posts from one site author
- mirror `kind 1` comments whose `#a` tag points at those posts
- serve queries from Stonr's local file-backed store

This is a one-way flow from Nostr into the site.

## Authoritative Content

`nostr-blog` should still copy or derive its authoritative site content into its
own local site data.

Stonr is the relay and ingest layer. It can retain a large local history, but
retention caps are still allowed to prune older relay data. A site should not
depend on relay retention alone as its only durable source of truth.

## Recommended Setup

1. Apply the `nostr-blog` preset in the `Network` section.
2. Set `Site author pubkey`.
3. Leave `Mirror comments for site posts` on unless the site should ignore
   comments entirely.
4. Confirm `Source relays` includes the upstream relays you want to mirror.
5. Set retention limits that match how much relay-side cache you want to keep.
