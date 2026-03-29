# Changelog

## 1.0.0

- File-backed Nostr relay core with HTTP and WebSocket serving.
- Policy-driven relay behavior controls across reads, writes, live subscriptions, mirror mode, and limits.
- Unified `NIP policies` management for protocol capability toggles and related policy controls.
- Runtime relay operations in the control app: start/stop/restart, diagnostics, event browser, and retention actions.
- Authentication and authorization controls including relay login and signed HTTP request support.
- File and blob APIs with compatibility routes, Blossom support, metadata handling, and storage policy controls.
- Desktop-focused control plane behaviors including startup service management and runtime visibility.
- Deployment and operations documentation for supervision, reverse proxy/TLS integration, mirroring, and production checks.
