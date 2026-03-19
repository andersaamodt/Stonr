# API Reference

stonr exposes a minimal HTTP and WebSocket interface compatible with NIP-01.

## HTTP

### `GET /healthz`
Returns `{ "status": "ok" }` when the relay is running.

### `GET /readyz`
Returns `200` with `{ "status": "ready", "issues": [] }` when the local store
layout is present and the relay can read its retention/runtime state.

Returns `503` with `{ "status": "not-ready", "issues": [...] }` when the store
is missing required paths or runtime status cannot be read.

### `GET /retention-health`
Returns structured retention status, including current stored event count/bytes,
configured caps, and the most recent prune/error state.

### `GET /mirror-health`
Returns per-upstream mirror runtime status, including last success/error and
lag-related timestamps.

### `GET /query`
Returns matching events as newline-delimited JSON (NDJSON).
Parameters mirror Nostr filter fields:

- `authors` – comma-separated list of public keys
- `kinds` – comma-separated list of kind numbers
- `d` / `t` – single `#d` or `#t` tag value
- `since` / `until` – Unix timestamps bounding `created_at`
- `limit` – maximum number of events to return

Example:

```bash
curl "http://localhost:7777/query?authors=npub1&kinds=1,30023&limit=5"
```

## WebSocket

Connect to the `BIND_WS` address and speak NIP-01 messages.
A typical subscription looks like:

```text
["REQ","sub1",{"authors":["npub1"],"kinds":[1]}]
```

For each matching event the server replies:

```text
["EVENT","sub1",{...event...}]
```

Once all events have been sent an end-of-stored-events marker is emitted:

```text
["EOSE","sub1"]
```

Send `["CLOSE","sub1"]` to terminate one subscription without closing the whole
socket.
