# Onstr App

`onstr/app` is the static UI for Onstr.

- `index.html` defines the five-tab shell (`Home`, `Discover`, `Compose`, `Library`, `Network`).
- `app.js` enforces a strict backend command allowlist and uses semantic `tablist/tab/tabpanel` navigation.
- `scripts/onstr-backend.sh` is the only shell entrypoint used by the UI.
- `themes/` is a symlink to the monorepo-shared Wizardry theme source.

## Persistence

Desktop UI preferences are persisted through backend key-value files in XDG config paths.
Browser `localStorage` is not used for durable state.

## Bridge posture

Hosted mode renders the interface but backend actions require the Wizardry desktop bridge.

## Backend contract

The UI only invokes code-defined intents that map to `onstr-core` subcommands and selected allowlisted `stonr` read-only status commands.
