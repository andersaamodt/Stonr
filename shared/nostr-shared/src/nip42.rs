use anyhow::{anyhow, bail, Result};
use rand::RngCore;
use std::collections::HashSet;
use url::Url;

use crate::{
    crypto::verify_event,
    event::{Event, Tag},
};

pub struct SessionAuth {
    challenge: String,
    pubkeys: HashSet<String>,
}

impl SessionAuth {
    pub fn new() -> Self {
        Self {
            challenge: generate_challenge(),
            pubkeys: HashSet::new(),
        }
    }

    pub fn challenge(&self) -> &str {
        &self.challenge
    }

    pub fn authenticate(&mut self, pubkey: String) {
        self.pubkeys.insert(pubkey);
    }

    pub fn is_authenticated(&self) -> bool {
        !self.pubkeys.is_empty()
    }

    pub fn contains_pubkey(&self, pubkey: &str) -> bool {
        self.pubkeys.contains(pubkey)
    }

    pub fn actor_pubkey(&self) -> Option<&str> {
        self.pubkeys.iter().min().map(String::as_str)
    }
}

impl Default for SessionAuth {
    fn default() -> Self {
        Self::new()
    }
}

pub fn generate_challenge() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn verify_auth_event(
    event: &Event,
    challenge: &str,
    bind_ws: &str,
    max_age_secs: u64,
    now: u64,
) -> Result<()> {
    if event.kind != 22_242 {
        bail!("AUTH event must use kind 22242");
    }
    verify_event(event)?;
    let challenge_tag =
        tag_value(event, "challenge").ok_or_else(|| anyhow!("AUTH event missing challenge tag"))?;
    if challenge_tag != challenge {
        bail!("AUTH event challenge mismatch");
    }
    let relay_tag =
        tag_value(event, "relay").ok_or_else(|| anyhow!("AUTH event missing relay tag"))?;
    if !relay_tag_matches(&relay_tag, bind_ws) {
        bail!("AUTH event relay tag does not match this relay");
    }
    if event.created_at.abs_diff(now) > max_age_secs {
        bail!("AUTH event is outside the accepted age window");
    }
    Ok(())
}

fn tag_value(event: &Event, tag_name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find_map(|Tag(fields)| match fields.as_slice() {
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
    candidates
        .iter()
        .any(|candidate| same_relay_url(relay_tag, candidate))
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
