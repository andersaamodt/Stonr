# Deployment And Supervision

Stonr is intended to run under an external supervisor in production.

The relay itself stays simple: one process, one `.env` file, one file-backed
store. Use the CLI to generate a supervisor definition that matches the exact
binary path and env file you are deploying.

## Generate A systemd Unit

```bash
stonr --env /etc/stonr/relay.env print-service --manager systemd --label stonr \
  > /etc/systemd/system/stonr.service
```

Then enable it:

```bash
systemctl daemon-reload
systemctl enable --now stonr.service
systemctl status stonr.service
```

## Generate A launchd Plist

```bash
stonr --env /Users/you/.config/stonr/relay.env print-service --manager launchd --label dev.stonr.relay \
  > ~/Library/LaunchAgents/dev.stonr.relay.plist
```

Then load it:

```bash
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/dev.stonr.relay.plist
launchctl kickstart -k gui/$(id -u)/dev.stonr.relay
launchctl print gui/$(id -u)/dev.stonr.relay
```

To unload it later:

```bash
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/dev.stonr.relay.plist
```

## Notes

- The generated service uses the exact `stonr` binary you ran and the exact
  `--env` path you supplied.
- `systemd` output includes restart-on-failure behavior and a higher file
  descriptor limit.
- `launchd` output writes stdout/stderr into `STORE_ROOT/runtime/`.
- Keep the `.env` file under config management separately from store backups.
- Pair supervision with the operational checks in [operations.md](operations.md)
  and the release smoke checks in [performance.md](performance.md).
- For public HTTPS exposure, pair supervision with the generated reverse proxy
  configs in [reverse-proxy.md](reverse-proxy.md).
