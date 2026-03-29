# Onstr App

`onstr/app` is the static UI for Onstr.

- `index.html` defines the two-tab shell (`Home`, `Discover`), a left-rail library listbox, and compose/settings drawers.
- `app.js` enforces a strict backend command allowlist, uses semantic `tablist/tab/tabpanel` navigation, and drives local-first auto-refresh.
- `scripts/onstr-backend.sh` is the only shell entrypoint used by the UI.
- `themes/` contains the in-repo Wizardry theme CSS files used by desktop packaging.

## Persistence

Desktop UI preferences are persisted through backend key-value files in XDG config paths.
Browser `localStorage` is not used for durable state.

## Bridge posture

Hosted mode renders the interface but backend actions require the Wizardry desktop bridge.

## Backend contract

The UI only invokes code-defined intents that map to `onstr-core` subcommands and selected allowlisted `stonr` read-only status commands.
