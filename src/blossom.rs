//! Blossom protocol helpers: auth tokens and blob descriptors.

use anyhow::{anyhow, Result};
use axum::http::HeaderMap;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};

use crate::{event::Event, files, storage::verify_event};

const AUTH_KIND: u32 = 24_242;

/// Supported Blossom authorization actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Get,
    Upload,
    List,
    Delete,
}

impl Action {
    pub fn as_tag(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Upload => "upload",
            Self::List => "list",
            Self::Delete => "delete",
        }
    }
}

/// Verified Blossom auth data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAuth {
    pub pubkey: String,
    pub hashes: Vec<String>,
}

impl VerifiedAuth {
    /// Require the auth token to authorize a specific blob hash.
    pub fn require_hash(&self, hash: &str) -> Result<()> {
        let hash = hash.to_ascii_lowercase();
        if self.hashes.iter().any(|value| value == &hash) {
            Ok(())
        } else if self.hashes.is_empty() {
            Err(anyhow!("blocked: auth missing x tag"))
        } else {
            Err(anyhow!("blocked: auth x tag mismatch"))
        }
    }
}

/// A Blossom blob descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlobDescriptor {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub mime: String,
    pub uploaded: u64,
}

/// Verify a Blossom auth header and return the authenticated pubkey and hashes.
pub fn verify_auth(headers: &HeaderMap, action: Action, host_domain: &str) -> Result<VerifiedAuth> {
    let header = headers
        .get("authorization")
        .ok_or_else(|| anyhow!("blocked: missing Authorization header"))?
        .to_str()?;
    let encoded = header
        .strip_prefix("Nostr ")
        .ok_or_else(|| anyhow!("blocked: unsupported Authorization scheme"))?;
    let event: Event = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded)?)?;
    verify_event(&event)?;
    if event.kind != AUTH_KIND {
        return Err(anyhow!("blocked: invalid Blossom auth event kind"));
    }

    let now = now_unix();
    if event.created_at > now {
        return Err(anyhow!("blocked: auth created_at must be in the past"));
    }
    let expiration = tag_values(&event, "expiration")
        .last()
        .ok_or_else(|| anyhow!("blocked: auth missing expiration tag"))?
        .parse::<u64>()
        .map_err(|_| anyhow!("blocked: invalid expiration tag"))?;
    if expiration <= now {
        return Err(anyhow!("blocked: auth token expired"));
    }
    if !tag_values(&event, "t")
        .iter()
        .any(|value| value.eq_ignore_ascii_case(action.as_tag()))
    {
        return Err(anyhow!("blocked: auth action not permitted"));
    }

    let host_domain = host_domain.to_ascii_lowercase();
    let servers = tag_values(&event, "server")
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if !servers.is_empty() && !servers.iter().any(|value| value == &host_domain) {
        return Err(anyhow!("blocked: auth server scope mismatch"));
    }

    let mut hashes = tag_values(&event, "x")
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .collect::<Vec<_>>();
    hashes.sort();
    hashes.dedup();
    Ok(VerifiedAuth {
        pubkey: event.pubkey,
        hashes,
    })
}

/// Build a Blossom blob descriptor for a stored blob.
pub fn descriptor(meta: &files::BlobMeta, public_origin: &str) -> BlobDescriptor {
    let origin = public_origin.trim_end_matches('/');
    let suffix = files::blob_extension(meta)
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default();
    BlobDescriptor {
        url: format!("{origin}/{}{}", meta.sha256, suffix),
        sha256: meta.sha256.clone(),
        size: meta.size,
        mime: meta.mime.clone(),
        uploaded: meta.uploaded_at,
    }
}

fn tag_values(event: &Event, name: &str) -> Vec<String> {
    event
        .tags
        .iter()
        .filter_map(|fields| match fields.0.as_slice() {
            [tag, value, ..] if tag == name => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use secp256k1::{Keypair, Message, Secp256k1};
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::event::Tag;

    fn auth_event(tags: Vec<Tag>) -> Event {
        let secp = Secp256k1::new();
        let sk = [4u8; 32];
        let kp = Keypair::from_seckey_slice(&secp, &sk).unwrap();
        let pubkey = hex::encode(kp.x_only_public_key().0.serialize());
        let mut event = Event {
            id: String::new(),
            pubkey,
            kind: AUTH_KIND,
            created_at: now_unix() - 1,
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

    #[test]
    fn verify_auth_accepts_valid_token() {
        let event = auth_event(vec![
            Tag(vec!["t".into(), "upload".into()]),
            Tag(vec!["expiration".into(), (now_unix() + 60).to_string()]),
            Tag(vec!["server".into(), "example.test".into()]),
            Tag(vec!["x".into(), "aa".repeat(32)]),
        ]);
        let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&event).unwrap());
        let mut headers = HeaderMap::new();
        headers.insert("authorization", format!("Nostr {encoded}").parse().unwrap());
        let auth = verify_auth(&headers, Action::Upload, "example.test").unwrap();
        auth.require_hash(&"aa".repeat(32)).unwrap();
    }
}
