# Stonr Control

`stonr-control` is a wizardry-style control plane for a local `stonr` relay.

It is intentionally thin:

- the UI is static HTML, CSS, and JavaScript
- backend actions go through [`scripts/stonr-control-backend.sh`](/Users/andersaamodt/git/stonr/app/scripts/stonr-control-backend.sh)
- relay configuration still lives in the relay `.env`
- moderation lists still live in the relay store under `admin/`

The app is designed for the same workspace as the relay and targets:

- hosted web
- macOS desktop
- Linux desktop

Hosted web mode renders the full interface but cannot start, stop, or edit the relay without the wizardry desktop bridge. Native host mode uses the shell backend to:

- load the resolved relay config with `stonr print-config`
- edit `.env` values
- edit file-backed moderation lists
- start, stop, and restart `stonr serve`
- tail the relay log
- run `stonr verify`
