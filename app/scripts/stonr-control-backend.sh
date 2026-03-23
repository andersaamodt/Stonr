#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd -P)
APP_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
# The app now lives at <repo>/app in source and <bundle>/Resources/<slug>/app in desktop bundles.
# In both cases, the repo/workspace root is the direct parent of APP_DIR.
REPO_ROOT=$(CDPATH= cd -- "$APP_DIR/.." && pwd -P)
PREF_DIR=${XDG_CONFIG_HOME:-$HOME/.config}/stonr-control
PREFS_FILE=$PREF_DIR/ui-prefs.env
STONR_CONFIG_DIR=${XDG_CONFIG_HOME:-$HOME/.config}/stonr

usage() {
  cat <<'USAGE'
Usage: stonr-control-backend.sh COMMAND [ARGS...]

Commands:
  choose-dir [INITIAL_PATH]
  doctor [ENV_PATH]
  get-ui-prefs
  set-ui-pref KEY VALUE
  service-autostart-status [ENV_PATH]
  service-autostart-enable [ENV_PATH]
  service-autostart-disable [ENV_PATH]
  load-config [ENV_PATH]
  load-env [ENV_PATH]
  mirror-status [ENV_PATH]
  retention-status [ENV_PATH]
  count-events [ENV_PATH]
  size-events [ENV_PATH]
  refresh-stats [ENV_PATH]
  purge-events [ENV_PATH]
  apply-retention [ENV_PATH]
  apply-preset [ENV_PATH] PRESET
  query-events [ENV_PATH] [SEARCH] [LIMIT]
  save-env [ENV_PATH] KEY VALUE
  load-list [ENV_PATH] NAME
  save-list [ENV_PATH] NAME BASE64_TEXT
  open-store-root [ENV_PATH]
  open-relay-profile [ENV_PATH]
  relay-status [ENV_PATH]
  relay-start [ENV_PATH]
  relay-stop [ENV_PATH]
  relay-restart [ENV_PATH]
  tail-log [ENV_PATH] [LINES]
  verify [ENV_PATH] [SAMPLE]

List NAME values:
  pubkeys-allow
  pubkeys-deny
  file-hashes-deny
USAGE
}

ensure_pref_dir() {
  mkdir -p "$PREF_DIR"
}

pref_get() {
  key=${1-}
  [ -f "$PREFS_FILE" ] || return 1
  awk -F= -v key="$key" '$1 == key { print substr($0, index($0, "=") + 1); found = 1 } END { exit found ? 0 : 1 }' "$PREFS_FILE"
}

pref_set() {
  key=${1-}
  value=${2-}
  ensure_pref_dir
  set_kv_file "$PREFS_FILE" "$key" "$value"
}

default_env_path() {
  default_path=$STONR_CONFIG_DIR/relay.env
  if saved=$(pref_get env_path 2>/dev/null); then
    if is_volatile_env_path "$saved"; then
      migrate_env_path "$saved" "$default_path"
    else
      printf '%s\n' "$saved"
    fi
    return 0
  fi
  pref_set env_path "$default_path"
  printf '%s\n' "$default_path"
}

resolve_env_path() {
  hint=${1-}
  if [ -n "$hint" ]; then
    if is_volatile_env_path "$hint"; then
      migrate_env_path "$hint" "$STONR_CONFIG_DIR/relay.env"
    else
      printf '%s\n' "$hint"
    fi
  else
    default_env_path
  fi
}

is_volatile_env_path() {
  path=${1-}
  case "$path" in
    "$REPO_ROOT/.env"|"$APP_DIR/"*|*/Contents/Resources/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

migrate_env_path() {
  old_path=${1-}
  new_path=${2-}
  mkdir -p "$(dirname "$new_path")"
  if [ -f "$old_path" ] && [ ! -f "$new_path" ]; then
    cp "$old_path" "$new_path"
  fi
  pref_set env_path "$new_path"
  printf '%s\n' "$new_path"
}

env_get() {
  file=${1-}
  key=${2-}
  [ -f "$file" ] || return 1
  awk -F= -v key="$key" '$1 == key { print substr($0, index($0, "=") + 1); found = 1 } END { exit found ? 0 : 1 }' "$file"
}

set_kv_file() {
  file=${1-}
  key=${2-}
  value=${3-}
  dir=$(dirname "$file")
  lockdir=$dir/.lock.$(basename "$file")
  mkdir -p "$dir"
  while ! mkdir "$lockdir" 2>/dev/null; do
    sleep 0.05
  done
  trap 'rmdir "$lockdir" 2>/dev/null || :' EXIT HUP INT TERM
  tmp=$(mktemp "$dir/.tmp.XXXXXX")
  if [ -f "$file" ]; then
    awk -v key="$key" -v value="$value" '
      BEGIN { done = 0 }
      $0 ~ ("^" key "=") {
        print key "=" value
        done = 1
        next
      }
      { print }
      END {
        if (!done) {
          print key "=" value
        }
      }
    ' "$file" > "$tmp"
  else
    printf '%s=%s\n' "$key" "$value" > "$tmp"
  fi
  mv "$tmp" "$file"
  rmdir "$lockdir" 2>/dev/null || :
  trap - EXIT HUP INT TERM
}

sanitize_env_value() {
  printf '%s' "${1-}" | tr '\r\n' '  ' | awk '
    BEGIN { ORS = "" }
    {
      gsub(/[[:space:]]+/, " ")
      sub(/^ /, "")
      sub(/ $/, "")
      print
    }
  '
}

ensure_env_file() {
  env_path=${1-}
  if [ -f "$env_path" ]; then
    return 0
  fi
  mkdir -p "$(dirname "$env_path")"
  store_root=$(default_store_root)
  mkdir -p "$store_root/admin" "$store_root/runtime"
  cat >"$env_path" <<EOF
STORE_ROOT=$store_root
BIND_HTTP=127.0.0.1:7777
BIND_WS=127.0.0.1:7778
VERIFY_SIG=0
RELAY_NAME=stonr
RELAY_DESCRIPTION=File-backed Nostr relay
ENABLE_NIP11=1
ENABLE_QUERY=1
ENABLE_PUBLISH=1
ENABLE_LIVE_SUBSCRIPTIONS=1
ENABLE_COUNT=1
ENABLE_TAG_QUERIES=1
ENABLE_SEARCH=1
ENABLE_MIRRORING=1
ALLOW_KINDS=
DENY_KINDS=
ALLOW_PUBKEYS=
DENY_PUBKEYS=
ENABLE_NIP42=0
REQUIRE_AUTH_FOR_QUERY=0
REQUIRE_AUTH_FOR_COUNT=0
REQUIRE_AUTH_FOR_PUBLISH=0
AUTH_MUST_MATCH_EVENT_PUBKEY=0
AUTH_MAX_AGE_SECS=600
RATE_LIMIT_WINDOW_SECS=60
MAX_QUERIES_PER_WINDOW=120
MAX_COUNTS_PER_WINDOW=120
MAX_PUBLISHES_PER_WINDOW=60
MAX_LIMIT=1000
MAX_EVENT_BYTES=262144
MAX_EVENT_AGE_SECS=31536000
MAX_EVENT_FUTURE_SECS=900
SUPPORT_NIP11=1
SUPPORT_NIP09=1
SUPPORT_NIP12=1
SUPPORT_NIP42=1
SUPPORT_NIP40=1
SUPPORT_NIP45=1
SUPPORT_NIP50=1
SUPPORT_NIP94=1
SUPPORT_NIP96=1
SUPPORT_NIP98=1
SUPPORT_NIP_B7=1
FILTER_PRIVATE_MESSAGES=1
ENABLE_FILE_METADATA=1
ENABLE_FILE_API=1
ENABLE_BLOSSOM=1
ENABLE_BLOSSOM_LIST=1
ENABLE_BLOSSOM_MIRROR=0
REQUIRE_NIP98_AUTH=0
REQUIRE_BLOSSOM_AUTH=0
REQUIRE_BLOSSOM_GET_AUTH=0
FILE_API_URL=
BLOSSOM_PUBLIC_URL=
FILE_MAX_BYTES=33554432
FILE_ALLOW_MIME=
FILE_DENY_MIME=
FILE_KEEP_MODE=referenced
MAX_BLOB_BYTES_PER_PUBKEY=
MIRROR_MODE=broad
MIRROR_SITE_INCLUDE_COMMENTS=1
RELAYS_UPSTREAM=$(default_relays_upstream)
FILE_HASH_DENYLIST_PATH=$store_root/admin/file-hashes.deny
EOF
}

default_store_root() {
  printf '%s\n' "${XDG_STATE_HOME:-$HOME/.local/state}/stonr/relay"
}

default_relays_upstream() {
  printf '%s\n' "wss://relay.damus.io,wss://nos.lol,wss://purplepag.es,wss://relay.primal.net,wss://relay.nostr.band,wss://relay.snort.social,wss://relay.nsec.app"
}

apply_preset_nostr_blog() {
  env_path=${1-}
  normalize_env_file "$env_path"
  ensure_runtime_dirs "$env_path"
  set_kv_file "$env_path" ENABLE_MIRRORING 1
  set_kv_file "$env_path" MIRROR_MODE site
  set_kv_file "$env_path" MIRROR_SITE_INCLUDE_COMMENTS 1
  set_kv_file "$env_path" FILTER_SINCE_MODE cursor
  set_kv_file "$env_path" ENABLE_QUERY 1
  set_kv_file "$env_path" ENABLE_COUNT 1
  set_kv_file "$env_path" ENABLE_TAG_QUERIES 1
  set_kv_file "$env_path" ENABLE_LIVE_SUBSCRIPTIONS 1
  set_kv_file "$env_path" ENABLE_SEARCH 1
  set_kv_file "$env_path" ENABLE_PUBLISH 0
  set_kv_file "$env_path" FILTER_PRIVATE_MESSAGES 1
  set_kv_file "$env_path" FILTER_AUTHORS ""
  set_kv_file "$env_path" FILTER_KINDS ""
  set_kv_file "$env_path" FILTER_TAG_A ""
  set_kv_file "$env_path" FILTER_TAG_T ""
  if upstream=$(env_get "$env_path" RELAYS_UPSTREAM 2>/dev/null); then
    if [ -z "$upstream" ]; then
      set_kv_file "$env_path" RELAYS_UPSTREAM "$(default_relays_upstream)"
    fi
  else
    set_kv_file "$env_path" RELAYS_UPSTREAM "$(default_relays_upstream)"
  fi
  printf 'preset=nostr-blog\n'
  printf 'env_path=%s\n' "$env_path"
}

normalize_env_file() {
  env_path=${1-}
  ensure_env_file "$env_path"
  if store_root=$(env_get "$env_path" STORE_ROOT 2>/dev/null); then
    if [ -n "$store_root" ]; then
      return 0
    fi
  fi
  store_root=$(default_store_root)
  mkdir -p "$store_root/admin" "$store_root/runtime"
  set_kv_file "$env_path" STORE_ROOT "$store_root"
}

store_root_from_env() {
  env_path=${1-}
  if store_root=$(env_get "$env_path" STORE_ROOT 2>/dev/null); then
    if [ -n "$store_root" ]; then
      printf '%s\n' "$store_root"
      return 0
    fi
  else
    :
  fi
  printf '%s\n' "$(default_store_root)"
}

pid_path() {
  env_path=${1-}
  store_root=$(store_root_from_env "$env_path")
  printf '%s\n' "$store_root/runtime/relay.pid"
}

log_path() {
  env_path=${1-}
  store_root=$(store_root_from_env "$env_path")
  printf '%s\n' "$store_root/runtime/relay.log"
}

event_log_path() {
  env_path=${1-}
  store_root=$(store_root_from_env "$env_path")
  printf '%s\n' "$store_root/log/events.ndjson"
}

event_size_cache_path() {
  env_path=${1-}
  store_root=$(store_root_from_env "$env_path")
  printf '%s\n' "$store_root/runtime/events-bytes.cache"
}

event_count_cache_path() {
  env_path=${1-}
  store_root=$(store_root_from_env "$env_path")
  printf '%s\n' "$store_root/runtime/events-count.cache"
}

retention_apply_lockdir() {
  env_path=${1-}
  store_root=$(store_root_from_env "$env_path")
  printf '%s\n' "$store_root/runtime/retention-apply.lock"
}

refresh_event_size_cache() {
  events_dir=${1-}
  cache_path=${2-}
  lockdir=${cache_path}.lock
  mkdir -p "$(dirname "$cache_path")"
  if ! mkdir "$lockdir" 2>/dev/null; then
    return 0
  fi
  (
    tmp="${cache_path}.tmp.$$"
    if [ -d "$events_dir" ]; then
      du -sk "$events_dir" | awk '{print $1 * 1024}' > "$tmp"
      mv "$tmp" "$cache_path"
    else
      printf '0\n' > "$tmp"
      mv "$tmp" "$cache_path"
    fi
    rmdir "$lockdir" 2>/dev/null || :
  ) >/dev/null 2>&1 &
}

ensure_runtime_dirs() {
  env_path=${1-}
  normalize_env_file "$env_path"
  store_root=$(store_root_from_env "$env_path")
  mkdir -p "$store_root/admin" "$store_root/runtime"
}

choose_dir() {
  initial=${1-}
  if [ -n "$initial" ] && [ -d "$initial" ]; then
    start_dir=$initial
  else
    start_dir=$(dirname "${initial:-$(default_store_root)}")
    [ -d "$start_dir" ] || start_dir=$HOME
  fi
  if command -v osascript >/dev/null 2>&1; then
    osascript <<EOF
set startDir to POSIX file "$(printf '%s' "$start_dir" | sed "s/\"/\\\\\"/g")"
tell application "System Events"
  activate
end tell
set chosenFolder to choose folder with prompt "Choose relay store root" default location startDir
POSIX path of chosenFolder
EOF
    return 0
  fi
  if command -v zenity >/dev/null 2>&1; then
    zenity --file-selection --directory --filename "$start_dir/"
    return 0
  fi
  if command -v kdialog >/dev/null 2>&1; then
    kdialog --getexistingdirectory "$start_dir"
    return 0
  fi
  printf '%s\n' "stonr-control-backend: no directory picker available" >&2
  exit 1
}

service_label() {
  printf 'dev.stonr.relay\n'
}

detect_service_manager() {
  if command -v launchctl >/dev/null 2>&1 && [ "$(uname -s 2>/dev/null || printf unknown)" = "Darwin" ]; then
    printf 'launchd\n'
    return 0
  fi
  if command -v systemctl >/dev/null 2>&1; then
    printf 'systemd\n'
    return 0
  fi
  printf 'none\n'
}

launchd_domain() {
  printf 'gui/%s\n' "$(id -u)"
}

launchd_plist_path() {
  printf '%s\n' "$HOME/Library/LaunchAgents/$(service_label).plist"
}

launchd_label_disabled() {
  label=${1-}
  [ -n "$label" ] || return 1
  launchctl print-disabled "$(launchd_domain)" 2>/dev/null | grep -F "\"$label\" => disabled" >/dev/null 2>&1
}

systemd_unit_name() {
  printf 'stonr-relay.service\n'
}

systemd_unit_path() {
  printf '%s\n' "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/$(systemd_unit_name)"
}

service_autostart_status() {
  env_path=${1-}
  manager=$(detect_service_manager)
  enabled=0
  installed=0
  loaded=0
  active=0
  label=$(service_label)
  path=
  case "$manager" in
    launchd)
      path=$(launchd_plist_path)
      if [ -f "$path" ]; then
        installed=1
        if launchd_label_disabled "$label"; then
          enabled=0
        else
          enabled=1
        fi
      fi
      if launchctl print "$(launchd_domain)/$label" >/dev/null 2>&1; then
        loaded=1
      fi
      ;;
    systemd)
      unit_name=$(systemd_unit_name)
      path=$(systemd_unit_path)
      if [ -f "$path" ]; then
        installed=1
      fi
      if systemctl --user is-enabled "$unit_name" >/dev/null 2>&1; then
        enabled=1
      fi
      if systemctl --user is-active "$unit_name" >/dev/null 2>&1; then
        active=1
      fi
      ;;
    *)
      manager=none
      ;;
  esac
  printf '%s\n' "manager=$manager"
  printf '%s\n' "label=$label"
  printf '%s\n' "path=$path"
  printf '%s\n' "enabled=$enabled"
  printf '%s\n' "installed=$installed"
  printf '%s\n' "loaded=$loaded"
  printf '%s\n' "active=$active"
  if [ -n "${env_path:-}" ]; then
    printf '%s\n' "env_path=$env_path"
  fi
}

service_autostart_enable() {
  env_path=${1-}
  manager=$(detect_service_manager)
  label=$(service_label)
  case "$manager" in
    launchd)
      plist_path=$(launchd_plist_path)
      mkdir -p "$(dirname "$plist_path")"
      run_stonr --env "$env_path" print-service --manager launchd --label "$label" > "$plist_path"
      if command -v /usr/libexec/PlistBuddy >/dev/null 2>&1; then
        /usr/libexec/PlistBuddy -c 'Delete :WorkingDirectory' "$plist_path" >/dev/null 2>&1 || :
        /usr/libexec/PlistBuddy -c 'Delete :StandardOutPath' "$plist_path" >/dev/null 2>&1 || :
        /usr/libexec/PlistBuddy -c 'Delete :StandardErrorPath' "$plist_path" >/dev/null 2>&1 || :
      fi
      launchctl enable "$(launchd_domain)/$label" >/dev/null 2>&1 || :
      launchctl bootout "$(launchd_domain)" "$plist_path" >/dev/null 2>&1 || :
      launchctl bootstrap "$(launchd_domain)" "$plist_path"
      launchctl kickstart -k "$(launchd_domain)/$label" >/dev/null 2>&1 || :
      ;;
    systemd)
      unit_name=$(systemd_unit_name)
      unit_path=$(systemd_unit_path)
      mkdir -p "$(dirname "$unit_path")"
      run_stonr --env "$env_path" print-service --manager systemd --label "$label" > "$unit_path"
      systemctl --user daemon-reload
      systemctl --user enable --now "$unit_name"
      ;;
    *)
      printf '%s\n' "stonr-control-backend: startup service unsupported on this host" >&2
      exit 1
      ;;
  esac
  service_autostart_status "$env_path"
}

service_autostart_disable() {
  env_path=${1-}
  manager=$(detect_service_manager)
  label=$(service_label)
  case "$manager" in
    launchd)
      plist_path=$(launchd_plist_path)
      launchctl bootout "$(launchd_domain)" "$plist_path" >/dev/null 2>&1 || :
      launchctl disable "$(launchd_domain)/$label" >/dev/null 2>&1 || :
      rm -f "$plist_path"
      ;;
    systemd)
      unit_name=$(systemd_unit_name)
      unit_path=$(systemd_unit_path)
      systemctl --user disable --now "$unit_name" >/dev/null 2>&1 || :
      rm -f "$unit_path"
      systemctl --user daemon-reload >/dev/null 2>&1 || :
      ;;
    *)
      printf '%s\n' "stonr-control-backend: startup service unsupported on this host" >&2
      exit 1
      ;;
  esac
  service_autostart_status "$env_path"
}

relay_pid() {
  env_path=${1-}
  file=$(pid_path "$env_path")
  [ -f "$file" ] || return 1
  pid=$(tr -d ' \r\n' < "$file")
  [ -n "$pid" ] || return 1
  printf '%s\n' "$pid"
}

find_running_relay_pid() {
  env_path=${1-}
  ps ax -o pid=,command= | awk -v env_path="$env_path" '
    index($0, "stonr") && index($0, "--env " env_path) && index($0, " serve") {
      print $1
      found = 1
      exit 0
    }
    END { exit found ? 0 : 1 }
  '
}

relay_running() {
  env_path=${1-}
  if pid=$(relay_pid "$env_path" 2>/dev/null); then
    if kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
    rm -f "$(pid_path "$env_path")"
  fi
  if pid=$(find_running_relay_pid "$env_path" 2>/dev/null); then
    printf '%s\n' "$pid" > "$(pid_path "$env_path")"
    return 0
  fi
  return 1
}

relay_profile_url() {
  env_path=${1-}
  bind_http=$(env_get "$env_path" BIND_HTTP "127.0.0.1:7777")
  host_port=$bind_http
  case "$host_port" in
    http://*|https://*)
      printf '%s\n' "$host_port"
      return 0
      ;;
  esac
  case "$host_port" in
    0.0.0.0:*)
      host_port=127.0.0.1:${host_port#0.0.0.0:}
      ;;
    \[::\]:*)
      host_port=127.0.0.1:${host_port#\[::\]:}
      ;;
  esac
  printf 'http://%s/\n' "$host_port"
}

resolve_repo_root() {
  conf_path=$REPO_ROOT/wizardry.workspace.conf
  if [ -f "$conf_path" ]; then
    root=$(
      awk -F= '$1 == "root" { print substr($0, index($0, "=") + 1); exit }' "$conf_path"
    )
    if [ -n "${root:-}" ] && [ -f "$root/Cargo.toml" ]; then
      printf '%s\n' "$root"
      return 0
    fi
  fi
  printf '%s\n' "$REPO_ROOT"
}

resolve_stonr_bin() {
  repo_root=$(resolve_repo_root)
  if [ -x "$repo_root/target/debug/stonr" ]; then
    printf '%s\n' "$repo_root/target/debug/stonr"
    return 0
  fi
  if command -v stonr >/dev/null 2>&1; then
    command -v stonr
    return 0
  fi
  if [ "$repo_root" != "$REPO_ROOT" ] && [ -x "$REPO_ROOT/target/debug/stonr" ]; then
    printf '%s\n' "$REPO_ROOT/target/debug/stonr"
    return 0
  fi
  printf '%s\n' "$repo_root/target/debug/stonr"
}

ensure_stonr_bin() {
  repo_root=$(resolve_repo_root)
  bin=$(resolve_stonr_bin)
  if [ -x "$bin" ]; then
    printf '%s\n' "$bin"
    return 0
  fi
  if ! cargo build --quiet --manifest-path "$repo_root/Cargo.toml"; then
    printf '%s\n' "stonr-control-backend: failed to build stonr binary" >&2
    exit 1
  fi
  resolve_stonr_bin
}

run_stonr() {
  bin=$(ensure_stonr_bin)
  "$bin" "$@"
}

summarize_events_json() {
  limit=${1-50}
  filter_private=${2-0}
  python3 -c '
import json, re, sys, time

MAX_CONTENT = 220
IMAGE_RE = re.compile(r"https?://\S+\.(?:png|jpe?g|gif|webp|avif)(?:\?\S*)?$", re.I)
URL_RE = re.compile(r"https?://\S+")
LIMIT = int(sys.argv[1])
FILTER_PRIVATE = sys.argv[2] == "1"
NOW = int(time.time())
PRIVATE_KINDS = {4, 13, 14, 15, 1059}

def image_url_from_text(text):
    for match in URL_RE.findall(text or ""):
        if IMAGE_RE.match(match.rstrip(".,);]")):
            return match.rstrip(".,);]")
    return None

def image_url_from_tags(event):
    for tag in event.get("tags") or []:
        if not isinstance(tag, list):
            continue
        for value in tag[1:]:
            if isinstance(value, str):
                image = image_url_from_text(value)
                if image:
                    return image
    return None

def preview_text(event):
    kind = event.get("kind")
    content = str(event.get("content") or "").strip()
    if kind == 1059:
        return "Encrypted message payload"
    if kind == 1063:
        return "File metadata event"
    if not content:
        return "(empty content)"
    if len(content) > MAX_CONTENT:
        content = content[:MAX_CONTENT - 1].rstrip() + "…"
    return content

def summarize(event):
    return {
        "id": event.get("id", ""),
        "pubkey": event.get("pubkey", ""),
        "kind": event.get("kind"),
        "created_at": event.get("created_at"),
        "content": preview_text(event),
        "image_url": image_url_from_text(str(event.get("content") or "")) or image_url_from_tags(event),
    }

events = json.load(sys.stdin)
if FILTER_PRIVATE:
    events = [event for event in events if int(event.get("kind") or 0) not in PRIVATE_KINDS]
summaries = [summarize(event) for event in events]
summaries.sort(key=lambda event: str(event.get("id") or ""), reverse=True)
summaries.sort(key=lambda event: min(int(event.get("created_at") or 0), NOW), reverse=True)
summaries.sort(key=lambda event: int(int(event.get("created_at") or 0) > NOW))
json.dump(summaries[:LIMIT], sys.stdout)
' "$limit" "$filter_private"
}

query_events_from_log() {
  log_file=${1-}
  search=${2-}
  limit=${3-50}
  python3 - "$log_file" "$search" "$limit" <<'PY'
import json, os, sys

log_path = sys.argv[1]
needle = sys.argv[2].strip().lower()
limit = int(sys.argv[3])
target = max(limit * 2, limit)
chunk_size = 262144
matches = []

def event_matches(event):
    if not needle:
        return True
    content = str(event.get("content") or "")
    return needle in content.lower()

with open(log_path, "rb") as handle:
    handle.seek(0, os.SEEK_END)
    position = handle.tell()
    remainder = b""
    while position > 0 and len(matches) < target:
        read_size = chunk_size if position >= chunk_size else position
        position -= read_size
        handle.seek(position)
        chunk = handle.read(read_size)
        data = chunk + remainder
        parts = data.split(b"\n")
        remainder = parts[0]
        for raw in reversed(parts[1:]):
            if not raw.strip():
                continue
            try:
                event = json.loads(raw)
            except json.JSONDecodeError:
                continue
            if event_matches(event):
                matches.append(event)
                if len(matches) >= target:
                    break
    if position == 0 and remainder.strip() and len(matches) < target:
        try:
            event = json.loads(remainder)
        except json.JSONDecodeError:
            event = None
        if event and event_matches(event):
            matches.append(event)

matches.reverse()
json.dump(matches, sys.stdout)
PY
}

status_kv() {
  env_path=${1-}
  status=stopped
  running=0
  pid=
  if relay_running "$env_path"; then
    status=running
    running=1
    pid=$(relay_pid "$env_path")
  fi
  printf '%s\n' "env_path=$env_path"
  printf '%s\n' "store_root=$(store_root_from_env "$env_path")"
  printf '%s\n' "pid_path=$(pid_path "$env_path")"
  printf '%s\n' "log_path=$(log_path "$env_path")"
  printf '%s\n' "status=$status"
  printf '%s\n' "running=$running"
  printf '%s\n' "pid=$pid"
}

decode_base64() {
  if printf '' | base64 --decode >/dev/null 2>&1; then
    base64 --decode
    return 0
  fi
  if printf '' | base64 -D >/dev/null 2>&1; then
    base64 -D
    return 0
  fi
  if printf '' | base64 -d >/dev/null 2>&1; then
    base64 -d
    return 0
  fi
  printf '%s\n' "stonr-control-backend: base64 decoder not available" >&2
  exit 1
}

list_path() {
  env_path=${1-}
  name=${2-}
  store_root=$(store_root_from_env "$env_path")
  case "$name" in
    pubkeys-allow) printf '%s\n' "$store_root/admin/pubkeys.allow" ;;
    pubkeys-deny) printf '%s\n' "$store_root/admin/pubkeys.deny" ;;
    file-hashes-deny)
      if path=$(env_get "$env_path" FILE_HASH_DENYLIST_PATH 2>/dev/null); then
        printf '%s\n' "$path"
      else
        printf '%s\n' "$store_root/admin/file-hashes.deny"
      fi
      ;;
    *)
      printf '%s\n' "stonr-control-backend: unknown list name: $name" >&2
      exit 2
      ;;
  esac
}

maybe_bind_hash_denylist_path() {
  env_path=${1-}
  name=${2-}
  path=${3-}
  if [ "$name" = "file-hashes-deny" ]; then
    set_kv_file "$env_path" FILE_HASH_DENYLIST_PATH "$path"
  fi
}

case "${1-}" in
  -h|--help|help)
    usage
    exit 0
    ;;
esac

cmd=${1-}
[ -n "$cmd" ] || {
  usage >&2
  exit 2
}
shift

case "$cmd" in
  choose-dir)
    choose_dir "${1-}"
    ;;
  get-ui-prefs)
    ensure_pref_dir
    [ -f "$PREFS_FILE" ] && cat "$PREFS_FILE"
    ;;
  set-ui-pref)
    key=${1-}
    value=${2-}
    [ -n "$key" ] || {
      printf '%s\n' "stonr-control-backend: set-ui-pref requires KEY" >&2
      exit 2
    }
    pref_set "$key" "$value"
    ;;
  service-autostart-status)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    service_autostart_status "$env_path"
    ;;
  service-autostart-enable)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    ensure_runtime_dirs "$env_path"
    service_autostart_enable "$env_path"
    ;;
  service-autostart-disable)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    service_autostart_disable "$env_path"
    ;;
  doctor)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    ensure_runtime_dirs "$env_path"
    printf '%s\n' "repo_root=$(resolve_repo_root)"
    printf '%s\n' "app_dir=$APP_DIR"
    printf '%s\n' "env_path=$env_path"
    printf '%s\n' "store_root=$(store_root_from_env "$env_path")"
    printf '%s\n' "stonr_bin=$(resolve_stonr_bin)"
    status_kv "$env_path"
    ;;
  load-config)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    run_stonr --env "$env_path" print-config
    ;;
  load-env)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    cat "$env_path"
    ;;
  mirror-status)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    run_stonr --env "$env_path" mirror-status
    ;;
  retention-status)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    run_stonr --env "$env_path" retention-status
    ;;
  count-events)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    root=$(store_root_from_env "$env_path")
    count_cache=$(event_count_cache_path "$env_path")
    event_log=$(event_log_path "$env_path")
    mkdir -p "$(dirname "$count_cache")"
    if [ -f "$count_cache" ]; then
      cat "$count_cache"
    elif [ -f "$event_log" ] && ! find "$root/events" -type f -name '*.json' -print -quit 2>/dev/null | grep -q .; then
      count=$(wc -l < "$event_log" | awk '{print $1}')
      printf '%s\n' "$count" > "$count_cache"
      printf '%s\n' "$count"
    elif [ -d "$root/events" ]; then
      count=$(find "$root/events" -type f -name '*.json' | wc -l | awk '{print $1}')
      printf '%s\n' "$count" > "$count_cache"
      printf '%s\n' "$count"
    else
      printf '0\n'
    fi
    ;;
  size-events)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    root=$(store_root_from_env "$env_path")
    cache_path=$(event_size_cache_path "$env_path")
    event_log=$(event_log_path "$env_path")
    mkdir -p "$(dirname "$cache_path")"
    if [ -d "$root/events" ]; then
      if [ -f "$cache_path" ]; then
        cat "$cache_path"
      else
        bytes=$(du -sk "$root/events" | awk '{print $1 * 1024}')
        printf '%s\n' "$bytes" > "$cache_path"
        printf '%s\n' "$bytes"
      fi
    elif [ -f "$event_log" ]; then
      bytes=$(stat -f '%z' "$event_log")
      printf '%s\n' "$bytes" > "$cache_path"
      printf '%s\n' "$bytes"
    else
      printf '0\n'
    fi
    ;;
  refresh-stats)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    run_stonr --env "$env_path" refresh-stats >/dev/null
    ;;
  purge-events)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    ensure_runtime_dirs "$env_path"
    run_stonr --env "$env_path" purge-events
    ;;
  apply-retention)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    ensure_runtime_dirs "$env_path"
    lockdir=$(retention_apply_lockdir "$env_path")
    mkdir -p "$(dirname "$lockdir")"
    if ! mkdir "$lockdir" 2>/dev/null; then
      printf 'started=0\n'
      printf 'already_running=1\n'
      exit 0
    fi
    (
      trap 'rmdir "$lockdir" 2>/dev/null || :' EXIT HUP INT TERM
      was_running=0
      if relay_running "$env_path"; then
        was_running=1
        "$0" relay-stop "$env_path" >/dev/null || :
      fi
      run_stonr --env "$env_path" prune-retention >/dev/null 2>&1 || :
      if [ "$was_running" -eq 1 ]; then
        "$0" relay-start "$env_path" >/dev/null || :
      fi
    ) >/dev/null 2>&1 &
    printf 'started=1\n'
    ;;
  apply-preset)
    env_path=$(resolve_env_path "${1-}")
    preset=${2-}
    case "$preset" in
      nostr-blog)
        apply_preset_nostr_blog "$env_path"
        ;;
      *)
        printf '%s\n' "stonr-control-backend: unknown preset: $preset" >&2
        exit 2
        ;;
    esac
    ;;
  query-events)
    env_path=$(resolve_env_path "${1-}")
    search=${2-}
    limit=${3-50}
    query_limit=$((limit * 2))
    filter_private=$(env_get "$env_path" FILTER_PRIVATE_MESSAGES 2>/dev/null || printf '1')
    normalize_env_file "$env_path"
    event_log=$(event_log_path "$env_path")
    if relay_running "$env_path"; then
      if [ -n "$search" ]; then
        query_json=$(run_stonr --env "$env_path" query --search "$search" --limit "$query_limit")
      else
        query_json=$(run_stonr --env "$env_path" query --limit "$query_limit")
      fi
      printf '%s' "$query_json" | summarize_events_json "$limit" "$filter_private"
    elif [ -f "$event_log" ]; then
      if [ -n "$search" ]; then
        query_events_from_log "$event_log" "$search" "$limit" | summarize_events_json "$limit" "$filter_private"
      else
        tail -n "$query_limit" "$event_log" | python3 -c '
import json, sys
events = []
for raw in sys.stdin:
    raw = raw.strip()
    if not raw:
        continue
    try:
        events.append(json.loads(raw))
    except json.JSONDecodeError:
        continue
json.dump(events, sys.stdout)
' | summarize_events_json "$limit" "$filter_private"
      fi
    else
      if [ -n "$search" ]; then
        query_json=$(run_stonr --env "$env_path" query --search "$search" --limit "$query_limit")
      else
        query_json=$(run_stonr --env "$env_path" query --limit "$query_limit")
      fi
      printf '%s' "$query_json" | summarize_events_json "$limit" "$filter_private"
    fi
    ;;
  save-env)
    env_path=$(resolve_env_path "${1-}")
    key=${2-}
    value=${3-}
    [ -n "$key" ] || {
      printf '%s\n' "stonr-control-backend: save-env requires KEY" >&2
      exit 2
    }
    normalize_env_file "$env_path"
    value=$(sanitize_env_value "$value")
    if [ "$key" = "STORE_ROOT" ] && [ -z "$value" ]; then
      value=$(default_store_root)
    fi
    set_kv_file "$env_path" "$key" "$value"
    ensure_runtime_dirs "$env_path"
    ;;
  load-list)
    env_path=$(resolve_env_path "${1-}")
    name=${2-}
    [ -n "$name" ] || {
      printf '%s\n' "stonr-control-backend: load-list requires NAME" >&2
      exit 2
    }
    normalize_env_file "$env_path"
    ensure_runtime_dirs "$env_path"
    path=$(list_path "$env_path" "$name")
    [ -f "$path" ] && cat "$path"
    ;;
  save-list)
    env_path=$(resolve_env_path "${1-}")
    name=${2-}
    payload=${3-}
    [ -n "$name" ] || {
      printf '%s\n' "stonr-control-backend: save-list requires NAME" >&2
      exit 2
    }
    normalize_env_file "$env_path"
    ensure_runtime_dirs "$env_path"
    path=$(list_path "$env_path" "$name")
    mkdir -p "$(dirname "$path")"
    printf '%s' "$payload" | decode_base64 > "$path"
    maybe_bind_hash_denylist_path "$env_path" "$name" "$path"
    ;;
  open-store-root)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    root=$(store_root_from_env "$env_path")
    mkdir -p "$root"
    if command -v open >/dev/null 2>&1; then
      open "$root"
    elif command -v xdg-open >/dev/null 2>&1; then
      xdg-open "$root"
    else
      printf '%s\n' "stonr-control-backend: no folder opener available" >&2
      exit 1
    fi
    printf '%s\n' "$root"
    ;;
  open-relay-profile)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    url=$(relay_profile_url "$env_path")
    if command -v open >/dev/null 2>&1; then
      open "$url"
    elif command -v xdg-open >/dev/null 2>&1; then
      xdg-open "$url"
    else
      printf '%s\n' "stonr-control-backend: no URL opener available" >&2
      exit 1
    fi
    printf '%s\n' "$url"
    ;;
  relay-status)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    ensure_runtime_dirs "$env_path"
    status_kv "$env_path"
    ;;
  relay-start)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    ensure_runtime_dirs "$env_path"
    if relay_running "$env_path"; then
      status_kv "$env_path"
      exit 0
    fi
    bin=$(ensure_stonr_bin)
    log_file=$(log_path "$env_path")
    pid_file=$(pid_path "$env_path")
    mkdir -p "$(dirname "$log_file")"
    nohup "$bin" --env "$env_path" serve >>"$log_file" 2>&1 &
    pid=$!
    printf '%s\n' "$pid" > "$pid_file"
    sleep 1
    if relay_running "$env_path"; then
      status_kv "$env_path"
      exit 0
    fi
    rm -f "$pid_file"
    printf '%s\n' "stonr-control-backend: relay failed to start" >&2
    exit 1
    ;;
  relay-stop)
    env_path=$(resolve_env_path "${1-}")
    normalize_env_file "$env_path"
    if relay_running "$env_path"; then
      pid=$(relay_pid "$env_path")
      kill "$pid" 2>/dev/null || true
      i=0
      while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 20 ]; do
        sleep 0.2
        i=$((i + 1))
      done
      if kill -0 "$pid" 2>/dev/null; then
        kill -9 "$pid" 2>/dev/null || true
      fi
    fi
    rm -f "$(pid_path "$env_path")"
    if relay_running "$env_path"; then
      printf '%s\n' "stonr-control-backend: relay failed to stop" >&2
      exit 1
    fi
    status_kv "$env_path"
    ;;
  relay-restart)
    env_path=$(resolve_env_path "${1-}")
    "$0" relay-stop "$env_path" >/dev/null
    "$0" relay-start "$env_path"
    ;;
  tail-log)
    env_path=$(resolve_env_path "${1-}")
    lines=${2-200}
    normalize_env_file "$env_path"
    file=$(log_path "$env_path")
    if [ -f "$file" ]; then
      python3 - "$file" "$lines" <<'PY'
import datetime
import json
import subprocess
import sys

log_path = sys.argv[1]
lines = sys.argv[2]

result = subprocess.run(
    ["tail", "-n", lines, log_path],
    check=False,
    capture_output=True,
    text=True,
    errors="replace",
)
if result.returncode != 0:
    sys.stderr.write(result.stderr)
    sys.exit(result.returncode)

def format_timestamp(value):
    try:
        return datetime.datetime.fromtimestamp(int(value)).strftime("%Y-%m-%d %H:%M:%S")
    except Exception:
        return datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")

for raw_line in result.stdout.splitlines():
    line = raw_line.rstrip("\n")
    ts = None
    try:
        parsed = json.loads(line)
        if isinstance(parsed, dict) and "ts" in parsed:
            ts = format_timestamp(parsed["ts"])
    except Exception:
        parsed = None
    if ts is None:
        ts = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    print(f"[{ts}] {line}")
PY
    fi
    ;;
  verify)
    env_path=$(resolve_env_path "${1-}")
    sample=${2-100}
    normalize_env_file "$env_path"
    run_stonr --env "$env_path" verify --sample "$sample"
    ;;
  *)
    printf '%s\n' "stonr-control-backend: unknown command: $cmd" >&2
    usage >&2
    exit 2
    ;;
esac
