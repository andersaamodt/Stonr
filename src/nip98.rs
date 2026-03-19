//! NIP-98 HTTP authentication helpers.

use anyhow::{anyhow, Result};
use axum::http::HeaderMap;
use base64::{engine::general_purpose::STANDARD, Engine};

use crate::{event::Event, storage::verify_event};

const AUTH_KIND: u32 = 27_235;
const MAX_SKEW_SECS: u64 = 300;

/// Verify a NIP-98 Authorization header and return the authenticated pubkey.
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

fn tag_value(event: &Event, tag: &str) -> Option<String> {
    event.tags.iter().find_map(|fields| match fields.0.as_slice() {
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
    use axum::http::HeaderValue;
    use secp256k1::{Keypair, Message, Secp256k1};
    use sha2::{Digest, Sha256};

    use super::*;

    fn auth_event(method: &str, url: &str, payload_hash: Option<&str>) -> Event {
        let secp = Secp256k1::new();
        let sk = [2u8; 32];
        let kp = Keypair::from_seckey_slice(&secp, &sk).unwrap();
        let pubkey = hex::encode(kp.x_only_public_key().0.serialize());
        let mut tags = vec![
            crate::event::Tag(vec!["u".into(), url.into()]),
            crate::event::Tag(vec!["method".into(), method.into()]),
        ];
        if let Some(payload_hash) = payload_hash {
            tags.push(crate::event::Tag(vec!["payload".into(), payload_hash.into()]));
        }
        let mut event = Event {
            id: String::new(),
            pubkey,
            kind: AUTH_KIND,
            created_at: now_unix(),
            tags,
            content: String::new(),
            sig: String::new(),
        };
        let arr = serde_json::json!([
            0,
            event.pubkey,
            event.created_at,
            event.kind,
            event.tags,
            event.content
        ]);
        let digest = Sha256::digest(serde_json::to_vec(&arr).unwrap());
        event.id = hex::encode(digest);
        let msg = Message::from_digest_slice(&hex::decode(&event.id).unwrap()).unwrap();
        event.sig = hex::encode(secp.sign_schnorr_no_aux_rand(&msg, &kp).as_ref());
        event
    }

    fn auth_header(event: &Event) -> HeaderMap {
        let encoded = STANDARD.encode(serde_json::to_vec(event).unwrap());
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Nostr {encoded}")).unwrap(),
        );
        headers
    }

    #[test]
    fn verify_http_auth_accepts_matching_event() {
        let payload = hex::encode(Sha256::digest(b"hello"));
        let event = auth_event("POST", "http://example.test/files", Some(&payload));
        let headers = auth_header(&event);
        assert_eq!(
            verify_http_auth(
                &headers,
                "POST",
                "http://example.test/files",
                Some(&payload)
            )
            .unwrap(),
            event.pubkey
        );
    }
}
