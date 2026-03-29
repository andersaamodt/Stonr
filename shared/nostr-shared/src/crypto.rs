use anyhow::{anyhow, Result};
use secp256k1::{schnorr::Signature, Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};
use sha2::{Digest, Sha256};

use crate::event::Event;

pub fn event_hash(event: &Event) -> Result<[u8; 32]> {
    let payload = serde_json::json!([
        0,
        event.pubkey,
        event.created_at,
        event.kind,
        event.tags,
        event.content
    ]);
    let data = serde_json::to_vec(&payload)?;
    Ok(Sha256::digest(data).into())
}

pub fn verify_event(event: &Event) -> Result<()> {
    let hash = event_hash(event)?;
    let calculated = hex::encode(hash);
    if calculated != event.id {
        return Err(anyhow!("id mismatch"));
    }
    let signature = Signature::from_slice(&hex::decode(&event.sig)?)?;
    let pubkey = XOnlyPublicKey::from_slice(&hex::decode(&event.pubkey)?)?;
    let secp = Secp256k1::verification_only();
    let msg = Message::from_digest_slice(&hash)?;
    secp.verify_schnorr(&signature, &msg, &pubkey)?;
    Ok(())
}

pub fn sign_event(event: &mut Event, secret_key_hex: &str) -> Result<()> {
    let secret_key = SecretKey::from_slice(&hex::decode(secret_key_hex)?)?;
    let secp = Secp256k1::new();
    let keypair = Keypair::from_secret_key(&secp, &secret_key);
    let pubkey = keypair.x_only_public_key().0;
    event.pubkey = hex::encode(pubkey.serialize());
    let hash = event_hash(event)?;
    event.id = hex::encode(hash);
    let msg = Message::from_digest_slice(&hash)?;
    let sig = secp.sign_schnorr_no_aux_rand(&msg, &keypair);
    event.sig = hex::encode(sig.as_ref());
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::event::Tag;

    use super::{sign_event, verify_event};

    #[test]
    fn signs_and_verifies_event() {
        let mut event = crate::event::Event {
            id: String::new(),
            pubkey: String::new(),
            kind: 1,
            created_at: 1,
            tags: vec![Tag(vec!["t".into(), "test".into()])],
            content: "hello".into(),
            sig: String::new(),
        };
        sign_event(&mut event, &"11".repeat(32)).unwrap();
        verify_event(&event).unwrap();
    }
}
