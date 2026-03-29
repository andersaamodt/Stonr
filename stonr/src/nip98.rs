//! NIP-98 HTTP authentication helpers.

use anyhow::Result;
use axum::http::HeaderMap;

/// Verify a NIP-98 Authorization header and return the authenticated pubkey.
pub fn verify_http_auth(
    headers: &HeaderMap,
    method: &str,
    url: &str,
    payload_hash_hex: Option<&str>,
) -> Result<String> {
    nostr_shared::nip98::verify_http_auth(headers, method, url, payload_hash_hex)
}

#[cfg(test)]
mod tests {
    use super::verify_http_auth;
    use axum::http::{HeaderMap, HeaderValue};
    use sha2::{Digest, Sha256};

    #[test]
    fn verify_http_auth_accepts_matching_event() {
        let payload = hex::encode(Sha256::digest(b"hello"));
        let header = nostr_shared::nip98::build_http_auth_header(
            &"22".repeat(32),
            "POST",
            "http://example.test/files",
            Some(&payload),
            None,
        )
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_str(&header).unwrap());
        let pubkey = verify_http_auth(
            &headers,
            "POST",
            "http://example.test/files",
            Some(&payload),
        )
        .unwrap();
        assert_eq!(pubkey.len(), 64);
    }
}
