# Reverse Proxy And TLS

Stonr listens on separate local ports for HTTP and WebSocket traffic:

- `BIND_HTTP` for relay info, health, query, and file/blob HTTP routes
- `BIND_WS` for Nostr WebSocket traffic

In production, front both with one public hostname through a reverse proxy.

## Generate A Caddy Config

```bash
stonr --env /etc/stonr/relay.env print-proxy --manager caddy --domain relay.example.com
```

The generated config routes upgraded WebSocket requests to `BIND_WS` and all
other traffic to `BIND_HTTP`.

## Generate An Nginx Config

```bash
stonr --env /etc/stonr/relay.env print-proxy --manager nginx --domain relay.example.com \
  --tls-cert /etc/letsencrypt/live/relay.example.com/fullchain.pem \
  --tls-key /etc/letsencrypt/live/relay.example.com/privkey.pem
```

The generated config:

- terminates TLS
- upgrades WebSocket requests to the WS port
- proxies normal HTTP requests to the HTTP port

## Recommended Proxy Checks

Use:

- `GET /healthz` for simple liveness
- `GET /readyz` for readiness checks before sending public traffic

`/readyz` returns `200` when the file-backed store layout is present and the
relay can read its runtime retention status. It returns `503` with a JSON issue
list when the store is not ready yet.

## Notes

- Keep `BIND_HTTP` and `BIND_WS` bound to localhost or a private interface.
- Pair the reverse proxy with the generated supervisor output from
  [deployment.md](deployment.md).
- If you rotate certificates manually, reload the proxy instead of the relay.
