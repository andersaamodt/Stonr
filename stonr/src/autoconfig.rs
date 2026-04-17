use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct AppSupportStatus {
    pub list_path: PathBuf,
    pub profiles: Vec<AppSupportProfile>,
    pub locks: Vec<AppSupportLock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppSupportProfile {
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
    pub locked_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppSupportLock {
    pub env_key: String,
    pub value: String,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct AppSupportListFile {
    #[serde(default)]
    paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MergeKind {
    BoolOr,
    CsvStrings,
    CsvU32,
    Exact,
}

#[derive(Debug, Clone)]
struct NormalizedSetting {
    env_key: String,
    merge_kind: MergeKind,
    value: String,
}

pub fn app_support_list_path(env_path: &Path) -> PathBuf {
    let parent = env_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = env_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("relay");
    parent.join(format!("{stem}.app-support.json"))
}

pub fn validate_support_file(path: &Path) -> Result<AppSupportProfile> {
    parse_support_file(path)
}

pub fn load_app_support_status(env_path: &Path) -> Result<AppSupportStatus> {
    let list_path = app_support_list_path(env_path);
    let list = load_list_file(&list_path)?;
    let base_dir = list_path.parent().unwrap_or_else(|| Path::new("."));
    let mut profiles = Vec::new();
    let mut merged = BTreeMap::<String, (MergeKind, String, BTreeSet<String>)>::new();
    for raw_path in list.paths {
        let resolved_path = resolve_support_path(base_dir, &raw_path);
        let profile = parse_support_file(&resolved_path)?;
        for setting in profile_settings(&profile.path)? {
            let entry = merged.entry(setting.env_key.clone()).or_insert_with(|| {
                (
                    setting.merge_kind,
                    setting.value.clone(),
                    BTreeSet::<String>::new(),
                )
            });
            if entry.0 != setting.merge_kind {
                bail!(
                    "app support merge mismatch for {} in {}",
                    setting.env_key,
                    profile.name
                );
            }
            match setting.merge_kind {
                MergeKind::BoolOr => {
                    let current = parse_bool_string(&entry.1).unwrap_or(false);
                    let incoming = parse_bool_string(&setting.value).unwrap_or(false);
                    entry.1 = if current || incoming { "1" } else { "0" }.to_string();
                }
                MergeKind::CsvStrings => {
                    entry.1 = merge_csv_strings(&entry.1, &setting.value);
                }
                MergeKind::CsvU32 => {
                    entry.1 = merge_csv_u32(&entry.1, &setting.value);
                }
                MergeKind::Exact => {
                    if entry.1 != setting.value {
                        bail!(
                            "conflicting app support values for {}: {} vs {}",
                            setting.env_key,
                            entry.1,
                            setting.value
                        );
                    }
                }
            }
            entry.2.insert(profile.name.clone());
        }
        profiles.push(profile);
    }
    let locks = merged
        .into_iter()
        .map(|(env_key, (_, value, sources))| AppSupportLock {
            env_key,
            value,
            sources: sources.into_iter().collect(),
        })
        .collect();
    Ok(AppSupportStatus {
        list_path,
        profiles,
        locks,
    })
}

pub fn apply_env_overrides(env_path: &Path, env: &mut HashMap<String, String>) -> Result<()> {
    let status = load_app_support_status(env_path)?;
    for lock in status.locks {
        env.insert(lock.env_key, lock.value);
    }
    Ok(())
}

fn load_list_file(path: &Path) -> Result<AppSupportListFile> {
    match fs::read_to_string(path) {
        Ok(data) => {
            let mut list: AppSupportListFile =
                serde_json::from_str(&data).context("parsing app support list")?;
            dedup_strings(&mut list.paths);
            Ok(list)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(AppSupportListFile::default())
        }
        Err(error) => {
            Err(error).with_context(|| format!("reading app support list {}", path.display()))
        }
    }
}

fn resolve_support_path(base_dir: &Path, raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

fn parse_support_file(path: &Path) -> Result<AppSupportProfile> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("reading app support file {}", path.display()))?;
    let root = match path.extension().and_then(|value| value.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("json") => {
            serde_json::from_str::<Value>(&data).context("parsing JSON app support file")?
        }
        _ => serde_yaml::from_str::<Value>(&data)
            .or_else(|_| serde_json::from_str::<Value>(&data))
            .context("parsing YAML/JSON app support file")?,
    };
    let object = root
        .as_object()
        .context("app support file must be a top-level object")?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("app support file must include a non-empty name")?
        .to_string();
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let settings = parse_settings_map(object)?;
    Ok(AppSupportProfile {
        name,
        path: path.to_path_buf(),
        description,
        locked_keys: settings
            .into_iter()
            .map(|setting| setting.env_key)
            .collect(),
    })
}

fn profile_settings(path: &Path) -> Result<Vec<NormalizedSetting>> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("reading app support file {}", path.display()))?;
    let root = match path.extension().and_then(|value| value.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("json") => {
            serde_json::from_str::<Value>(&data).context("parsing JSON app support file")?
        }
        _ => serde_yaml::from_str::<Value>(&data)
            .or_else(|_| serde_json::from_str::<Value>(&data))
            .context("parsing YAML/JSON app support file")?,
    };
    let object = root
        .as_object()
        .context("app support file must be a top-level object")?;
    parse_settings_map(object)
}

fn parse_settings_map(object: &serde_json::Map<String, Value>) -> Result<Vec<NormalizedSetting>> {
    let mut merged = serde_json::Map::new();
    for field in ["settings", "requires"] {
        if let Some(value) = object.get(field) {
            let settings = value
                .as_object()
                .with_context(|| format!("{field} must be an object"))?;
            for (key, value) in settings {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    if merged.is_empty() {
        bail!("app support file must include a non-empty settings object");
    }
    let mut normalized = Vec::new();
    for (key, value) in merged {
        normalized.push(normalize_setting(&key, &value)?);
    }
    normalized.sort_by(|left, right| left.env_key.cmp(&right.env_key));
    Ok(normalized)
}

fn normalize_setting(env_key: &str, value: &Value) -> Result<NormalizedSetting> {
    let key = env_key.trim().to_string();
    if key.is_empty() {
        bail!("app support setting keys must not be empty");
    }
    let (merge_kind, value) = if is_bool_key(&key) {
        (
            MergeKind::BoolOr,
            if parse_bool_value(value)? { "1" } else { "0" }.to_string(),
        )
    } else if is_csv_string_key(&key) {
        (
            MergeKind::CsvStrings,
            parse_string_list_value(value)?.join(","),
        )
    } else if is_csv_u32_key(&key) {
        (
            MergeKind::CsvU32,
            parse_u32_list_value(value)?
                .into_iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    } else if is_usize_key(&key) || is_u64_key(&key) {
        (MergeKind::Exact, parse_u64_value(value)?.to_string())
    } else if key == "MIRROR_MODE" {
        (
            MergeKind::Exact,
            parse_enum_string(value, &["broad", "site"])?,
        )
    } else if key == "FILE_KEEP_MODE" {
        (
            MergeKind::Exact,
            parse_enum_string(value, &["referenced", "all"])?,
        )
    } else if key == "FILTER_SINCE_MODE" {
        (MergeKind::Exact, parse_since_mode(value)?)
    } else if is_string_key(&key) {
        (MergeKind::Exact, parse_string_value(value)?)
    } else {
        bail!("unsupported app support setting: {key}");
    };
    Ok(NormalizedSetting {
        env_key: key,
        merge_kind,
        value,
    })
}

fn parse_bool_value(value: &Value) -> Result<bool> {
    if let Some(boolean) = value.as_bool() {
        return Ok(boolean);
    }
    if let Some(number) = value.as_u64() {
        return Ok(number != 0);
    }
    if let Some(text) = value.as_str() {
        return parse_bool_string(text).context("expected a boolean value");
    }
    bail!("expected a boolean value")
}

fn parse_bool_string(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_u64_value(value: &Value) -> Result<u64> {
    if let Some(number) = value.as_u64() {
        return Ok(number);
    }
    if let Some(text) = value.as_str() {
        return text
            .trim()
            .parse::<u64>()
            .context("expected an unsigned integer");
    }
    bail!("expected an unsigned integer")
}

fn parse_string_value(value: &Value) -> Result<String> {
    let text = value.as_str().context("expected a string value")?.trim();
    Ok(text.to_string())
}

fn parse_enum_string(value: &Value, allowed: &[&str]) -> Result<String> {
    let text = parse_string_value(value)?.to_ascii_lowercase();
    if allowed.iter().any(|allowed_value| *allowed_value == text) {
        Ok(text)
    } else {
        bail!("unsupported value: {text}")
    }
}

fn parse_since_mode(value: &Value) -> Result<String> {
    let text = parse_string_value(value)?;
    let normalized = text.trim().to_ascii_lowercase();
    if normalized == "cursor" {
        return Ok(normalized);
    }
    if let Some(rest) = normalized.strip_prefix("fixed:") {
        rest.parse::<u64>()
            .context("fixed FILTER_SINCE_MODE values must be numeric")?;
        return Ok(normalized);
    }
    bail!("FILTER_SINCE_MODE must be \"cursor\" or \"fixed:<unix-seconds>\"");
}

fn parse_string_list_value(value: &Value) -> Result<Vec<String>> {
    let mut values = Vec::new();
    match value {
        Value::Array(items) => {
            for item in items {
                let text = parse_string_value(item)?;
                if !text.is_empty() {
                    values.push(text);
                }
            }
        }
        Value::String(text) => {
            for part in text.split(',') {
                let item = part.trim();
                if !item.is_empty() {
                    values.push(item.to_string());
                }
            }
        }
        _ => bail!("expected a string or string array"),
    }
    dedup_strings(&mut values);
    Ok(values)
}

fn parse_u32_list_value(value: &Value) -> Result<Vec<u32>> {
    let mut values = Vec::new();
    match value {
        Value::Array(items) => {
            for item in items {
                if let Some(number) = item.as_u64() {
                    values.push(number as u32);
                } else if let Some(text) = item.as_str() {
                    values.push(
                        text.trim()
                            .parse::<u32>()
                            .context("expected u32 list item")?,
                    );
                } else {
                    bail!("expected a u32 or string u32 in array");
                }
            }
        }
        Value::String(text) => {
            for part in text.split(',') {
                let item = part.trim();
                if !item.is_empty() {
                    values.push(item.parse::<u32>().context("expected u32 list item")?);
                }
            }
        }
        _ => bail!("expected a string or numeric array"),
    }
    values.sort_unstable();
    values.dedup();
    Ok(values)
}

fn merge_csv_strings(existing: &str, incoming: &str) -> String {
    let mut values = existing
        .split(',')
        .chain(incoming.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    dedup_strings(&mut values);
    values.join(",")
}

fn merge_csv_u32(existing: &str, incoming: &str) -> String {
    let mut values = existing
        .split(',')
        .chain(incoming.split(','))
        .filter_map(|value| value.trim().parse::<u32>().ok())
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    values
        .into_iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn dedup_strings(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

fn is_bool_key(key: &str) -> bool {
    matches!(
        key,
        "VERIFY_SIG"
            | "ENABLE_NIP11"
            | "ENABLE_QUERY"
            | "ENABLE_PUBLISH"
            | "ENABLE_LIVE_SUBSCRIPTIONS"
            | "ENABLE_COUNT"
            | "ENABLE_TAG_QUERIES"
            | "ENABLE_SEARCH"
            | "ENABLE_MIRRORING"
            | "PIN_PROTECT_FROM_DELETES"
            | "ENABLE_NIP42"
            | "REQUIRE_AUTH_FOR_QUERY"
            | "REQUIRE_AUTH_FOR_COUNT"
            | "REQUIRE_AUTH_FOR_PUBLISH"
            | "AUTH_MUST_MATCH_EVENT_PUBKEY"
            | "SUPPORT_NIP11"
            | "SUPPORT_NIP09"
            | "SUPPORT_NIP12"
            | "SUPPORT_NIP42"
            | "SUPPORT_NIP40"
            | "SUPPORT_NIP45"
            | "SUPPORT_NIP50"
            | "SUPPORT_NIP94"
            | "SUPPORT_NIP96"
            | "SUPPORT_NIP98"
            | "SUPPORT_NIP_B7"
            | "FILTER_PRIVATE_MESSAGES"
            | "ENABLE_FILE_METADATA"
            | "ENABLE_FILE_API"
            | "ENABLE_BLOSSOM"
            | "ENABLE_BLOSSOM_LIST"
            | "ENABLE_BLOSSOM_MIRROR"
            | "REQUIRE_NIP98_AUTH"
            | "REQUIRE_BLOSSOM_AUTH"
            | "REQUIRE_BLOSSOM_GET_AUTH"
            | "MIRROR_SITE_INCLUDE_COMMENTS"
    )
}

fn is_csv_string_key(key: &str) -> bool {
    matches!(
        key,
        "ALLOW_PUBKEYS"
            | "DENY_PUBKEYS"
            | "OWNER_PUBKEYS"
            | "FOLLOW_PUBKEYS"
            | "PIN_EVENT_IDS"
            | "RELAYS_UPSTREAM"
            | "FILTER_AUTHORS"
            | "FILTER_TAG_T"
            | "FILTER_TAG_A"
            | "FILE_ALLOW_MIME"
            | "FILE_DENY_MIME"
    )
}

fn is_csv_u32_key(key: &str) -> bool {
    matches!(key, "ALLOW_KINDS" | "DENY_KINDS" | "FILTER_KINDS")
}

fn is_string_key(key: &str) -> bool {
    matches!(
        key,
        "RELAY_NAME"
            | "RELAY_DESCRIPTION"
            | "PUBLIC_RELAY_URL"
            | "TOR_SOCKS"
            | "MIRROR_SITE_AUTHOR"
            | "FILE_API_URL"
            | "BLOSSOM_PUBLIC_URL"
    )
}

fn is_usize_key(key: &str) -> bool {
    matches!(
        key,
        "MAX_STORED_EVENTS"
            | "MAX_LIMIT"
            | "MAX_EVENT_BYTES"
            | "MAX_QUERIES_PER_WINDOW"
            | "MAX_COUNTS_PER_WINDOW"
            | "MAX_PUBLISHES_PER_WINDOW"
            | "FILE_MAX_BYTES"
    )
}

fn is_u64_key(key: &str) -> bool {
    matches!(
        key,
        "AUTH_MAX_AGE_SECS"
            | "MAX_STORED_EVENT_BYTES"
            | "MAX_EVENT_AGE_SECS"
            | "MAX_EVENT_FUTURE_SECS"
            | "RATE_LIMIT_WINDOW_SECS"
            | "MAX_BLOB_BYTES_PER_PUBKEY"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_yaml_profile_and_reports_locks() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("binder.yaml");
        fs::write(
            &path,
            concat!(
                "name: Binder\n",
                "description: Enable Binder-facing relay features\n",
                "settings:\n",
                "  ENABLE_QUERY: true\n",
                "  ENABLE_COUNT: true\n",
                "  RELAYS_UPSTREAM:\n",
                "    - wss://relay.example\n",
                "    - wss://relay.example\n",
            ),
        )
        .unwrap();
        let profile = validate_support_file(&path).unwrap();
        assert_eq!(profile.name, "Binder");
        assert_eq!(
            profile.description.as_deref(),
            Some("Enable Binder-facing relay features")
        );
        assert!(profile.locked_keys.contains(&"ENABLE_QUERY".to_string()));
        assert!(profile.locked_keys.contains(&"RELAYS_UPSTREAM".to_string()));
    }

    #[test]
    fn bools_or_and_lists_union_across_profiles() {
        let dir = tempdir().unwrap();
        let env_path = dir.path().join("relay.env");
        fs::write(
            &env_path,
            "STORE_ROOT=/tmp\nBIND_HTTP=127.0.0.1:1\nBIND_WS=127.0.0.1:2\n",
        )
        .unwrap();
        let binder = dir.path().join("binder.json");
        let blossom = dir.path().join("blossom.yaml");
        fs::write(
            &binder,
            r#"{"name":"Binder","settings":{"ENABLE_QUERY":true,"RELAYS_UPSTREAM":["wss://a"],"FILTER_AUTHORS":["alice"]}}"#,
        )
        .unwrap();
        fs::write(
            &blossom,
            concat!(
                "name: Blossom\n",
                "settings:\n",
                "  ENABLE_QUERY: false\n",
                "  ENABLE_BLOSSOM: true\n",
                "  RELAYS_UPSTREAM: [wss://b]\n",
                "  FILTER_AUTHORS: [bob]\n",
            ),
        )
        .unwrap();
        let list_path = app_support_list_path(&env_path);
        fs::write(
            &list_path,
            serde_json::to_string(&AppSupportListFile {
                paths: vec![binder.display().to_string(), blossom.display().to_string()],
            })
            .unwrap(),
        )
        .unwrap();
        let status = load_app_support_status(&env_path).unwrap();
        let query = status
            .locks
            .iter()
            .find(|lock| lock.env_key == "ENABLE_QUERY")
            .unwrap();
        assert_eq!(query.value, "1");
        let upstream = status
            .locks
            .iter()
            .find(|lock| lock.env_key == "RELAYS_UPSTREAM")
            .unwrap();
        assert_eq!(upstream.value, "wss://a,wss://b");
        let authors = status
            .locks
            .iter()
            .find(|lock| lock.env_key == "FILTER_AUTHORS")
            .unwrap();
        assert_eq!(authors.value, "alice,bob");
    }

    #[test]
    fn conflicting_exact_values_fail() {
        let dir = tempdir().unwrap();
        let env_path = dir.path().join("relay.env");
        fs::write(
            &env_path,
            "STORE_ROOT=/tmp\nBIND_HTTP=127.0.0.1:1\nBIND_WS=127.0.0.1:2\n",
        )
        .unwrap();
        let one = dir.path().join("one.yaml");
        let two = dir.path().join("two.yaml");
        fs::write(&one, "name: One\nsettings:\n  MIRROR_MODE: broad\n").unwrap();
        fs::write(&two, "name: Two\nsettings:\n  MIRROR_MODE: site\n").unwrap();
        let list_path = app_support_list_path(&env_path);
        fs::write(
            &list_path,
            serde_json::to_string(&AppSupportListFile {
                paths: vec![one.display().to_string(), two.display().to_string()],
            })
            .unwrap(),
        )
        .unwrap();
        let error = load_app_support_status(&env_path).unwrap_err();
        assert!(error.to_string().contains("conflicting app support values"));
    }
}
