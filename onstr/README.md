# Onstr

Onstr is a file-backed Nostr client designed to pair with `stonr`.

## Layout

- `core/` - `onstr-core` Rust binary used by desktop/web adapters.
- `app/` - static UI (`index.html`, `style.css`, `app.js`) and backend adapter script.

## Core command surface

`onstr-core` exposes structured JSON I/O for:

- `profile`
- `relay`
- `timeline`
- `discover`
- `compose`
- `publish`
- `library`
- `media`
- `doctor`

## Local state model

Onstr stores durable state as plaintext files under XDG roots.

- profile metadata + encrypted NIP-49 `ncryptsec` payloads
- relay sets
- drafts
- local library indexes (`starred` / `saved` local-only; `liked` / `commented` derived)
- sync cursors and authored cache

## UI model

Onstr app is tabbed with five primary tabs:

- Home
- Discover
- Compose
- Library
- Network

Desktop persistence for theme and UI state is backend-file-backed through `onstr-backend.sh`.
