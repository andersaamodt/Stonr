//! Relay authentication helpers for NIP-42.

use anyhow::Result;

use crate::event::Event;

pub(crate) use nostr_shared::nip42::SessionAuth;

pub(crate) fn verify_auth_event(
    event: &Event,
    challenge: &str,
    bind_ws: &str,
    max_age_secs: u64,
    now: u64,
) -> Result<()> {
    nostr_shared::nip42::verify_auth_event(event, challenge, bind_ws, max_age_secs, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::event_hash;
    use secp256k1::{Keypair, Message, Secp256k1};

    fn signed_auth_event(bind_ws: &str, challenge: &str, created_at: u64) -> Event {
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
                crate::event::Tag(vec!["relay".into(), format!("ws://{bind_ws}/")]),
                crate::event::Tag(vec!["challenge".into(), challenge.into()]),
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
