# Performance Smoke Testing

Stonr's production readiness includes repeatable performance smoke checks for:

- large local event stores
- long-running upstream mirror sessions that continue after `EOSE`

These are smoke checks, not portable benchmarks. They exist to catch obvious
regressions in hot paths and to give operators a quick sanity check before
shipping a build.

## Run The Large-Store Smoke

```bash
cargo test --release --bin stonr large_store_perf_smoke -- --ignored --nocapture
```

This test:

- creates a synthetic store with 20,000 events
- refreshes the on-disk count/byte stats cache
- runs an empty-filter recent query
- runs a relay-side text-search query

The test prints timings and fails if those operations exceed broad smoke
thresholds.

## Run The Long-Running Mirror Smoke

```bash
cargo test --release --bin stonr mirror_long_running_session_perf_smoke -- --ignored --nocapture
```

This test:

- opens a local upstream relay fixture
- sends `EOSE`
- keeps the socket open
- streams 2,000 more live events after `EOSE`
- checks that Stonr keeps ingesting those events and finishes within a broad
  runtime bound

## Baseline From 2026-03-18

Measured on the development machine used for this change:

- `large_store_perf_smoke`
  - synthetic store size: `20,000` events / `3,522,100` bytes
  - `refresh_stats_cache`: `58 ms`
  - recent empty-filter query (`limit 60`): `505 ms`
  - text search query (`limit 60`): `438 ms`
- `mirror_long_running_session_perf_smoke`
  - delayed live mirror ingest after `EOSE`: `2,000` events in `2,197 ms`

The exact numbers are expected to vary by disk, CPU, and build mode. The main
production requirement is that these smoke tests remain available, documented,
and green on release builds before deployment changes.
