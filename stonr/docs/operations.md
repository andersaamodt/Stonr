# Operations

Stonr is designed so the authoritative relay state stays in ordinary files.
That makes two operator tasks straightforward:

- check whether retention is actually being met
- back up and restore the relay without relying on a database

## Retention status

Stonr writes structured retention status under:

```text
STORE_ROOT/runtime/retention-status.json
```

You can also print it directly:

```bash
stonr --env /path/to/relay.env retention-status
```

The status reports:

- current stored event count and bytes
- configured event and byte caps
- whether either cap is currently exceeded
- the last prune time and how many events it removed
- the last retention error, if one occurred

For HTTP-based monitoring, the same information is exposed at:

```text
GET /retention-health
```

## Backup

Create a relay snapshot:

```bash
stonr --env /path/to/relay.env backup --destination /path/to/backup-dir
```

The destination must be empty or not yet exist.

The backup includes only authoritative relay data:

- `events/`
- `files/`
- `admin/`
- `cursor/`

Derived directories such as `index/`, `latest/`, `mirror/`, `tombstones/`,
`log/`, and `runtime/` are intentionally not backed up. Stonr rebuilds them on
restore.

Each backup also includes a `manifest.json` file describing the snapshot
format and creation time.

## Restore

Restore a snapshot into the current `STORE_ROOT`:

```bash
stonr --env /path/to/relay.env restore --source /path/to/backup-dir
```

Recommended restore workflow:

1. Stop the relay.
2. Restore the snapshot.
3. Start the relay again.
4. Optionally run `stonr --env /path/to/relay.env verify --sample 1000`.

Restore replaces authoritative store data under the current `STORE_ROOT`,
recreates the directory layout, rebuilds derived metadata from stored events,
rebuilds blob references, and refreshes stats caches.

## Config handling

The backup/restore commands snapshot the relay store, not the `.env` file.
Treat the env file as operator configuration:

- keep it under config management
- back it up separately if needed
- restore it intentionally rather than blindly replacing local paths
