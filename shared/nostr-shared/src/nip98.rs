use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use http::HeaderMap;

use crate::{
    crypto::{sign_event, verify_event},
    event::{Event, Tag},
};

const AUTH_KIND: u32 = 27_235;
const MAX_SKEW_SECS: u64 = 300;

pub fn verify_http_auth(
    headers: &HeaderMap,
    method: &str,
    url: &str,
    payload_hash_hex: Option<&str>,
) -> Result<String> {
    let header = headers
        .get("authorization")
        .ok_or_else(|| anyhow!("blocked: missing Authorization header"))?
        .to_str()?;
    let encoded = header
        .strip_prefix("Nostr ")
        .ok_or_else(|| anyhow!("blocked: unsupported Authorization scheme"))?;
    let decoded = STANDARD.decode(encoded)?;
    let event: Event = serde_json::from_slice(&decoded)?;
    verify_event(&event)?;
    if event.kind != AUTH_KIND {
        return Err(anyhow!("blocked: invalid auth event kind"));
    }
    let now = now_unix();
    if event.created_at > now + MAX_SKEW_SECS
        || now.saturating_sub(event.created_at) > MAX_SKEW_SECS
    {
        return Err(anyhow!("blocked: stale auth event"));
    }
    let auth_url =
        tag_value(&event, "u").ok_or_else(|| anyhow!("blocked: auth missing url tag"))?;
    if auth_url != url {
        return Err(anyhow!("blocked: auth url mismatch"));
    }
    let auth_method =
        tag_value(&event, "method").ok_or_else(|| anyhow!("blocked: auth missing method tag"))?;
    if !auth_method.eq_ignore_ascii_case(method) {
        return Err(anyhow!("blocked: auth method mismatch"));
    }
    if let Some(payload_hash_hex) = payload_hash_hex {
        let tag = tag_value(&event, "payload")
            .ok_or_else(|| anyhow!("blocked: auth missing payload tag"))?;
        if !payload_tag_matches(&tag, payload_hash_hex)? {
            return Err(anyhow!("blocked: auth payload hash mismatch"));
        }
    }
    Ok(event.pubkey)
}

pub fn build_http_auth_header(
    secret_key_hex: &str,
    method: &str,
    url: &str,
    payload_hash_hex: Option<&str>,
    created_at: Option<u64>,
) -> Result<String> {
    let mut tags = vec![
        Tag(vec!["u".into(), url.to_string()]),
        Tag(vec!["method".into(), method.to_uppercase()]),
    ];
    if let Some(hash) = payload_hash_hex {
        tags.push(Tag(vec!["payload".into(), hash.to_string()]));
    }
    let mut event = Event {
        id: String::new(),
        pubkey: String::new(),
        kind: AUTH_KIND,
        created_at: created_at.unwrap_or_else(now_unix),
        tags,
        content: String::new(),
        sig: String::new(),
    };
    sign_event(&mut event, secret_key_hex)?;
    let payload = STANDARD.encode(serde_json::to_vec(&event)?);
    Ok(format!("Nostr {payload}"))
}

fn tag_value(event: &Event, tag: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find_map(|fields| match fields.0.as_slice() {
            [name, value, ..] if name == tag => Some(value.clone()),
            _ => None,
        })
}

fn payload_tag_matches(tag_value: &str, payload_hash_hex: &str) -> Result<bool> {
    if tag_value.eq_ignore_ascii_case(payload_hash_hex) {
        return Ok(true);
    }
    let raw = hex::decode(payload_hash_hex)?;
    Ok(STANDARD.encode(raw) == tag_value)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use http::{HeaderMap, HeaderValue};

    use super::{build_http_auth_header, verify_http_auth};

    #[test]
    fn builds_and_verifies_header() {
        let payload = "aa".repeat(32);
        let header = build_http_auth_header(
            &"22".repeat(32),
            "POST",
            "https://example.test/files",
            Some(&payload),
            None,
        )
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_str(&header).unwrap());
        let pubkey = verify_http_auth(
            &headers,
            "POST",
            "https://example.test/files",
            Some(&payload),
        )
        .unwrap();
        assert_eq!(pubkey.len(), 64);
    }
}
