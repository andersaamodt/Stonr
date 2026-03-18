//! Relay authentication helpers for NIP-42.

use std::collections::HashSet;

use anyhow::{anyhow, bail, Result};
use rand::RngCore;
use url::Url;

use crate::{event::{Event, Tag}, storage::verify_signed_event};

pub(crate) struct SessionAuth {
    challenge: String,
    pubkeys: HashSet<String>,
}

impl SessionAuth {
    pub(crate) fn new() -> Self {
        Self {
            challenge: generate_challenge(),
            pubkeys: HashSet::new(),
        }
    }

    pub(crate) fn challenge(&self) -> &str {
        &self.challenge
    }

    pub(crate) fn authenticate(&mut self, pubkey: String) {
        self.pubkeys.insert(pubkey);
    }

    pub(crate) fn is_authenticated(&self) -> bool {
        !self.pubkeys.is_empty()
    }

    pub(crate) fn contains_pubkey(&self, pubkey: &str) -> bool {
        self.pubkeys.contains(pubkey)
    }
}

pub(crate) fn verify_auth_event(
    event: &Event,
    challenge: &str,
    bind_ws: &str,
    max_age_secs: u64,
    now: u64,
) -> Result<()> {
    if event.kind != 22242 {
        bail!("AUTH event must use kind 22242");
    }
    verify_signed_event(event)?;
    let challenge_tag = tag_value(event, "challenge")
        .ok_or_else(|| anyhow!("AUTH event missing challenge tag"))?;
    if challenge_tag != challenge {
        bail!("AUTH event challenge mismatch");
    }
    let relay_tag = tag_value(event, "relay").ok_or_else(|| anyhow!("AUTH event missing relay tag"))?;
    if !relay_tag_matches(&relay_tag, bind_ws) {
        bail!("AUTH event relay tag does not match this relay");
    }
    if event.created_at.abs_diff(now) > max_age_secs {
        bail!("AUTH event is outside the accepted age window");
    }
    Ok(())
}

fn generate_challenge() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event.tags.iter().find_map(|Tag(fields)| match fields.as_slice() {
        [tag, value, ..] if tag == tag_name => Some(value.clone()),
        _ => None,
    })
}

fn relay_tag_matches(relay_tag: &str, bind_ws: &str) -> bool {
    let candidates = [
        format!("ws://{bind_ws}"),
        format!("ws://{bind_ws}/"),
        format!("wss://{bind_ws}"),
        format!("wss://{bind_ws}/"),
    ];
    candidates.iter().any(|candidate| same_relay_url(relay_tag, candidate))
}

fn same_relay_url(left: &str, right: &str) -> bool {
    if normalize_url(left) == normalize_url(right) {
        return true;
    }
    let Ok(left_url) = Url::parse(left) else {
        return false;
    };
    let Ok(right_url) = Url::parse(right) else {
        return false;
    };
    left_url.host_str() == right_url.host_str()
        && left_url.port_or_known_default() == right_url.port_or_known_default()
        && normalize_path(left_url.path()) == normalize_path(right_url.path())
}

fn normalize_url(value: &str) -> String {
    value.trim_end_matches('/').to_string()
}

fn normalize_path(path: &str) -> &str {
    if path.is_empty() {
        "/"
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::event_hash;

    fn signed_auth_event(bind_ws: &str, challenge: &str, created_at: u64) -> Event {
        use secp256k1::{Keypair, Message, Secp256k1};

        let secp = Secp256k1::new();
        let sk = [7u8; 32];
        let kp = Keypair::from_seckey_slice(&secp, &sk).unwrap();
        let pubkey = hex::encode(kp.x_only_public_key().0.serialize());
        let mut event = Event {
            id: String::new(),
            pubkey,
            kind: 22242,
            created_at,
            tags: vec![
                Tag(vec!["relay".into(), format!("ws://{bind_ws}/")]),
                Tag(vec!["challenge".into(), challenge.into()]),
            ],
            content: String::new(),
            sig: String::new(),
        };
        let hash = event_hash(&event).unwrap();
        event.id = hex::encode(hash);
        let msg = Message::from_digest_slice(&hash).unwrap();
        let sig = secp.sign_schnorr_no_aux_rand(&msg, &kp);
        event.sig = hex::encode(sig.as_ref());
        event
    }

    #[test]
    fn verify_auth_event_accepts_valid_event() {
        let event = signed_auth_event("127.0.0.1:7778", "abc123", 1000);
        verify_auth_event(&event, "abc123", "127.0.0.1:7778", 600, 1000).unwrap();
    }

    #[test]
    fn verify_auth_event_rejects_bad_challenge() {
        let event = signed_auth_event("127.0.0.1:7778", "abc123", 1000);
        assert!(verify_auth_event(&event, "zzz", "127.0.0.1:7778", 600, 1000).is_err());
    }

    #[test]
    fn verify_auth_event_rejects_age_outside_window() {
        let event = signed_auth_event("127.0.0.1:7778", "abc123", 1);
        assert!(verify_auth_event(&event, "abc123", "127.0.0.1:7778", 10, 1000).is_err());
    }
}
