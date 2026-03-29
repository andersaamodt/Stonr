#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd -P)
APP_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
ONSTR_DIR=$(CDPATH= cd -- "$APP_DIR/.." && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$ONSTR_DIR/.." && pwd -P)
PREF_DIR=${XDG_CONFIG_HOME:-$HOME/.config}/onstr/app
PREF_FILE=$PREF_DIR/ui-prefs.env

ensure_pref_dir() {
  mkdir -p "$PREF_DIR"
}

pref_get() {
  key=${1-}
  [ -f "$PREF_FILE" ] || return 1
  awk -F= -v key="$key" '$1 == key { print substr($0, index($0, "=") + 1); found = 1 } END { exit found ? 0 : 1 }' "$PREF_FILE"
}

pref_set() {
  key=${1-}
  value=${2-}
  ensure_pref_dir
  tmp=$(mktemp "$PREF_DIR/.tmp.XXXXXX")
  if [ -f "$PREF_FILE" ]; then
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
    ' "$PREF_FILE" > "$tmp"
  else
    printf '%s=%s\n' "$key" "$value" > "$tmp"
  fi
  mv "$tmp" "$PREF_FILE"
}

list_themes() {
  themes_dir=$ONSTR_DIR/app/themes
  if [ ! -d "$themes_dir" ]; then
    printf '%s\n' "wizard"
    return 0
  fi
  find "$themes_dir" -maxdepth 1 -type f -name '*.css' -print \
    | awk -F/ '{print $NF}' \
    | sed 's/\.css$//' \
    | sort
}

run_core() {
  if command -v onstr-core >/dev/null 2>&1; then
    onstr-core "$@"
    return 0
  fi
  cargo run --quiet --manifest-path "$ONSTR_DIR/core/Cargo.toml" -- "$@"
}

run_stonr() {
  if command -v stonr >/dev/null 2>&1; then
    stonr "$@"
    return 0
  fi
  cargo run --quiet --manifest-path "$REPO_ROOT/stonr/Cargo.toml" -- "$@"
}

usage() {
  cat <<'USAGE'
Usage: onstr-backend.sh COMMAND [ARGS...]

Commands:
  get-ui-prefs
  set-ui-pref KEY VALUE
  list-themes
  profile-list
  profile-create NAME PASSWORD [SECRET_KEY] [SET_ACTIVE]
  profile-import NAME PASSWORD NCRYPTSEC [SET_ACTIVE]
  profile-export PASSWORD [PROFILE_ID]
  profile-use PROFILE_ID
  profile-unlock PASSWORD [PROFILE_ID] [TTL_SECS]
  profile-lock [PROFILE_ID]
  relay-list
  relay-add URL MODE
  relay-remove URL
  relay-set-home URL
  relay-probe [URL]
  timeline-fetch AUTHORS KINDS SEARCH SINCE UNTIL LIMIT INCLUDE_REMOTES [TAG_P]
  discover-search TERM LIMIT
  discover-count TERM
  discover-relay-info URL
  compose-note CONTENT TAGS DRAFT
  compose-reply CONTENT EVENT_ID DRAFT
  compose-longform TITLE IDENTIFIER CONTENT SUMMARY DRAFT
  compose-file-metadata URL HASH MIME SIZE DRAFT
  compose-delete EVENT_ID REASON DRAFT
  compose-list-drafts
  compose-preview DRAFT
  compose-sign-draft DRAFT PASSWORD [PROFILE_ID]
  publish-event-file PATH PASSWORD [PROFILE_ID] [RELAYS_CSV]
  publish-draft DRAFT PASSWORD [PROFILE_ID] [RELAYS_CSV]
  library-list [BUCKET]
  library-star EVENT_ID
  library-unstar EVENT_ID
  library-save EVENT_ID
  library-unsave EVENT_ID
  library-ingest-authored PATH
  library-reindex
  media-nip94-template URL HASH MIME SIZE [NAME]
  media-upload-nip96 RELAY_URL FILE [PASSWORD] [PROFILE_ID]
  stonr-print-config [ENV_PATH]
  stonr-mirror-status [ENV_PATH]
  stonr-retention-status [ENV_PATH]
  doctor
USAGE
}

csv_relays_to_args() {
  csv=${1-}
  if [ -z "$csv" ]; then
    return 0
  fi
  old_ifs=$IFS
  IFS=,
  for relay in $csv; do
    trimmed=$(printf '%s' "$relay" | awk '{gsub(/^[[:space:]]+|[[:space:]]+$/, ""); print}')
    if [ -n "$trimmed" ]; then
      printf '%s\n' "$trimmed"
    fi
  done
  IFS=$old_ifs
}

cmd=${1-}
if [ -z "$cmd" ]; then
  usage >&2
  exit 1
fi
shift

case "$cmd" in
  get-ui-prefs)
    theme=$(pref_get theme 2>/dev/null || printf 'wizard')
    active_tab=$(pref_get active_tab 2>/dev/null || printf 'home')
    printf 'theme=%s\n' "$theme"
    printf 'active_tab=%s\n' "$active_tab"
    ;;
  set-ui-pref)
    key=${1-}
    value=${2-}
    [ -n "$key" ] || { printf '%s\n' 'missing key' >&2; exit 2; }
    pref_set "$key" "$value"
    printf 'ok=1\n'
    ;;
  list-themes)
    list_themes
    ;;

  profile-list)
    run_core profile list
    ;;
  profile-create)
    name=${1-}
    password=${2-}
    secret_key=${3-}
    set_active=${4-0}
    [ -n "$name" ] && [ -n "$password" ] || { printf '%s\n' 'missing name/password' >&2; exit 2; }
    if [ "$set_active" = "1" ]; then
      if [ -n "$secret_key" ]; then
        run_core profile create --name "$name" --password "$password" --secret-key "$secret_key" --set-active
      else
        run_core profile create --name "$name" --password "$password" --set-active
      fi
    else
      if [ -n "$secret_key" ]; then
        run_core profile create --name "$name" --password "$password" --secret-key "$secret_key"
      else
        run_core profile create --name "$name" --password "$password"
      fi
    fi
    ;;
  profile-import)
    name=${1-}
    password=${2-}
    ncryptsec=${3-}
    set_active=${4-0}
    [ -n "$name" ] && [ -n "$password" ] && [ -n "$ncryptsec" ] || { printf '%s\n' 'missing arguments' >&2; exit 2; }
    if [ "$set_active" = "1" ]; then
      run_core profile import --name "$name" --password "$password" --ncryptsec "$ncryptsec" --set-active
    else
      run_core profile import --name "$name" --password "$password" --ncryptsec "$ncryptsec"
    fi
    ;;
  profile-export)
    password=${1-}
    profile_id=${2-}
    [ -n "$password" ] || { printf '%s\n' 'missing password' >&2; exit 2; }
    if [ -n "$profile_id" ]; then
      run_core profile export --password "$password" --id "$profile_id"
    else
      run_core profile export --password "$password"
    fi
    ;;
  profile-use)
    profile_id=${1-}
    [ -n "$profile_id" ] || { printf '%s\n' 'missing profile id' >&2; exit 2; }
    run_core profile use --id "$profile_id"
    ;;
  profile-unlock)
    password=${1-}
    profile_id=${2-}
    ttl_secs=${3-900}
    [ -n "$password" ] || { printf '%s\n' 'missing password' >&2; exit 2; }
    if [ -n "$profile_id" ]; then
      run_core profile unlock --password "$password" --id "$profile_id" --ttl-secs "$ttl_secs"
    else
      run_core profile unlock --password "$password" --ttl-secs "$ttl_secs"
    fi
    ;;
  profile-lock)
    profile_id=${1-}
    if [ -n "$profile_id" ]; then
      run_core profile lock --id "$profile_id"
    else
      run_core profile lock
    fi
    ;;

  relay-list)
    run_core relay list
    ;;
  relay-add)
    url=${1-}
    mode=${2-both}
    [ -n "$url" ] || { printf '%s\n' 'missing relay url' >&2; exit 2; }
    run_core relay add --url "$url" --mode "$mode"
    ;;
  relay-remove)
    url=${1-}
    [ -n "$url" ] || { printf '%s\n' 'missing relay url' >&2; exit 2; }
    run_core relay remove --url "$url"
    ;;
  relay-set-home)
    url=${1-}
    [ -n "$url" ] || { printf '%s\n' 'missing relay url' >&2; exit 2; }
    run_core relay set-home --url "$url"
    ;;
  relay-probe)
    url=${1-}
    if [ -n "$url" ]; then
      run_core relay probe --url "$url"
    else
      run_core relay probe
    fi
    ;;

  timeline-fetch)
    authors=${1-}
    kinds=${2-}
    search=${3-}
    since=${4-}
    until=${5-}
    limit=${6-50}
    include_remotes=${7-1}
    tag_p=${8-}
    set -- timeline fetch --limit "$limit"
    if [ -n "$authors" ]; then
      set -- "$@" --authors "$authors"
    fi
    if [ -n "$kinds" ]; then
      set -- "$@" --kinds "$kinds"
    fi
    if [ -n "$search" ]; then
      set -- "$@" --search "$search"
    fi
    if [ -n "$since" ]; then
      set -- "$@" --since "$since"
    fi
    if [ -n "$until" ]; then
      set -- "$@" --until "$until"
    fi
    if [ "$include_remotes" = "0" ]; then
      set -- "$@" --include-remotes false
    fi
    if [ -n "$tag_p" ]; then
      set -- "$@" --tag-p "$tag_p"
    fi
    run_core "$@"
    ;;

  discover-search)
    term=${1-}
    limit=${2-30}
    [ -n "$term" ] || { printf '%s\n' 'missing term' >&2; exit 2; }
    run_core discover search --term "$term" --limit "$limit"
    ;;
  discover-count)
    term=${1-}
    if [ -n "$term" ]; then
      run_core discover count --term "$term"
    else
      run_core discover count
    fi
    ;;
  discover-relay-info)
    url=${1-}
    [ -n "$url" ] || { printf '%s\n' 'missing relay url' >&2; exit 2; }
    run_core discover relay-info --url "$url"
    ;;

  compose-note)
    content=${1-}
    tags=${2-}
    draft=${3-}
    [ -n "$content" ] || { printf '%s\n' 'missing content' >&2; exit 2; }
    set -- compose note --content "$content"
    if [ -n "$tags" ]; then
      old_ifs=$IFS
      IFS=,
      for tag in $tags; do
        trimmed=$(printf '%s' "$tag" | awk '{gsub(/^[[:space:]]+|[[:space:]]+$/, ""); print}')
        if [ -n "$trimmed" ]; then
          set -- "$@" --tags "$trimmed"
        fi
      done
      IFS=$old_ifs
    fi
    if [ -n "$draft" ]; then
      set -- "$@" --draft "$draft"
    fi
    run_core "$@"
    ;;
  compose-reply)
    content=${1-}
    event_id=${2-}
    draft=${3-}
    [ -n "$content" ] && [ -n "$event_id" ] || { printf '%s\n' 'missing content/event id' >&2; exit 2; }
    set -- compose reply --content "$content" --event-id "$event_id"
    if [ -n "$draft" ]; then
      set -- "$@" --draft "$draft"
    fi
    run_core "$@"
    ;;
  compose-longform)
    title=${1-}
    identifier=${2-}
    content=${3-}
    summary=${4-}
    draft=${5-}
    [ -n "$title" ] && [ -n "$identifier" ] && [ -n "$content" ] || { printf '%s\n' 'missing longform args' >&2; exit 2; }
    set -- compose longform --title "$title" --identifier "$identifier" --content "$content"
    if [ -n "$summary" ]; then
      set -- "$@" --summary "$summary"
    fi
    if [ -n "$draft" ]; then
      set -- "$@" --draft "$draft"
    fi
    run_core "$@"
    ;;
  compose-file-metadata)
    url=${1-}
    hash=${2-}
    mime=${3-}
    size=${4-}
    draft=${5-}
    [ -n "$url" ] && [ -n "$hash" ] && [ -n "$mime" ] && [ -n "$size" ] || { printf '%s\n' 'missing metadata args' >&2; exit 2; }
    set -- compose file-metadata --url "$url" --hash "$hash" --mime "$mime" --size "$size"
    if [ -n "$draft" ]; then
      set -- "$@" --draft "$draft"
    fi
    run_core "$@"
    ;;
  compose-delete)
    event_id=${1-}
    reason=${2-}
    draft=${3-}
    [ -n "$event_id" ] || { printf '%s\n' 'missing event id' >&2; exit 2; }
    set -- compose delete --event-id "$event_id"
    if [ -n "$reason" ]; then
      set -- "$@" --reason "$reason"
    fi
    if [ -n "$draft" ]; then
      set -- "$@" --draft "$draft"
    fi
    run_core "$@"
    ;;
  compose-list-drafts)
    run_core compose list-drafts
    ;;
  compose-preview)
    draft=${1-}
    [ -n "$draft" ] || { printf '%s\n' 'missing draft name' >&2; exit 2; }
    run_core compose preview --draft "$draft"
    ;;
  compose-sign-draft)
    draft=${1-}
    password=${2-}
    profile_id=${3-}
    [ -n "$draft" ] && [ -n "$password" ] || { printf '%s\n' 'missing draft/password' >&2; exit 2; }
    if [ -n "$profile_id" ]; then
      run_core compose sign-draft --draft "$draft" --password "$password" --profile-id "$profile_id"
    else
      run_core compose sign-draft --draft "$draft" --password "$password"
    fi
    ;;

  publish-event-file)
    path=${1-}
    password=${2-}
    profile_id=${3-}
    relays_csv=${4-}
    [ -n "$path" ] && [ -n "$password" ] || { printf '%s\n' 'missing path/password' >&2; exit 2; }
    set -- publish event-file --path "$path" --password "$password"
    if [ -n "$profile_id" ]; then
      set -- "$@" --profile-id "$profile_id"
    fi
    for relay in $(csv_relays_to_args "$relays_csv"); do
      set -- "$@" --relay "$relay"
    done
    run_core "$@"
    ;;
  publish-draft)
    draft=${1-}
    password=${2-}
    profile_id=${3-}
    relays_csv=${4-}
    [ -n "$draft" ] && [ -n "$password" ] || { printf '%s\n' 'missing draft/password' >&2; exit 2; }
    set -- publish draft --draft "$draft" --password "$password"
    if [ -n "$profile_id" ]; then
      set -- "$@" --profile-id "$profile_id"
    fi
    for relay in $(csv_relays_to_args "$relays_csv"); do
      set -- "$@" --relay "$relay"
    done
    run_core "$@"
    ;;

  library-list)
    bucket=${1-}
    if [ -n "$bucket" ]; then
      run_core library list --bucket "$bucket"
    else
      run_core library list
    fi
    ;;
  library-star)
    event_id=${1-}
    [ -n "$event_id" ] || { printf '%s\n' 'missing event id' >&2; exit 2; }
    run_core library star --event-id "$event_id"
    ;;
  library-unstar)
    event_id=${1-}
    [ -n "$event_id" ] || { printf '%s\n' 'missing event id' >&2; exit 2; }
    run_core library unstar --event-id "$event_id"
    ;;
  library-save)
    event_id=${1-}
    [ -n "$event_id" ] || { printf '%s\n' 'missing event id' >&2; exit 2; }
    run_core library save --event-id "$event_id"
    ;;
  library-unsave)
    event_id=${1-}
    [ -n "$event_id" ] || { printf '%s\n' 'missing event id' >&2; exit 2; }
    run_core library unsave --event-id "$event_id"
    ;;
  library-ingest-authored)
    path=${1-}
    [ -n "$path" ] || { printf '%s\n' 'missing path' >&2; exit 2; }
    run_core library ingest-authored --path "$path"
    ;;
  library-reindex)
    run_core library reindex
    ;;

  media-nip94-template)
    url=${1-}
    hash=${2-}
    mime=${3-}
    size=${4-}
    name=${5-}
    [ -n "$url" ] && [ -n "$hash" ] && [ -n "$mime" ] && [ -n "$size" ] || { printf '%s\n' 'missing media args' >&2; exit 2; }
    if [ -n "$name" ]; then
      run_core media nip94-template --url "$url" --hash "$hash" --mime "$mime" --size "$size" --name "$name"
    else
      run_core media nip94-template --url "$url" --hash "$hash" --mime "$mime" --size "$size"
    fi
    ;;
  media-upload-nip96)
    relay_url=${1-}
    file=${2-}
    password=${3-}
    profile_id=${4-}
    [ -n "$relay_url" ] && [ -n "$file" ] || { printf '%s\n' 'missing relay/file' >&2; exit 2; }
    set -- media upload-nip96 --relay-url "$relay_url" --file "$file"
    if [ -n "$password" ]; then
      set -- "$@" --password "$password"
    fi
    if [ -n "$profile_id" ]; then
      set -- "$@" --profile-id "$profile_id"
    fi
    run_core "$@"
    ;;

  stonr-print-config)
    env_path=${1-.env}
    run_stonr --env "$env_path" print-config
    ;;
  stonr-mirror-status)
    env_path=${1-.env}
    run_stonr --env "$env_path" mirror-status
    ;;
  stonr-retention-status)
    env_path=${1-.env}
    run_stonr --env "$env_path" retention-status
    ;;

  doctor)
    run_core doctor
    ;;

  *)
    usage >&2
    exit 1
    ;;
esac
