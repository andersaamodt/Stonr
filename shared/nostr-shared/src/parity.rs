use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NipCapabilityManifest {
    pub version: u32,
    pub baseline: String,
    pub required: Vec<NipCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NipCapability {
    pub id: String,
    pub number: Option<u32>,
    pub description: String,
}

pub fn manifest() -> Result<NipCapabilityManifest> {
    Ok(serde_json::from_str(include_str!(
        "../../nip-capabilities.json"
    ))?)
}

pub fn numeric_required_nips() -> Result<Vec<u32>> {
    let mut list = manifest()?
        .required
        .into_iter()
        .filter_map(|item| item.number)
        .collect::<Vec<_>>();
    list.sort_unstable();
    list.dedup();
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::{manifest, numeric_required_nips};

    #[test]
    fn manifest_contains_required_extensions() {
        let manifest = manifest().unwrap();
        assert_eq!(manifest.baseline, "NIP-01");
        let ids = manifest
            .required
            .into_iter()
            .map(|item| item.id)
            .collect::<Vec<_>>();
        for required in [
            "NIP-09", "NIP-11", "NIP-12", "NIP-40", "NIP-42", "NIP-45", "NIP-50", "NIP-94",
            "NIP-96", "NIP-98", "NIP-B7",
        ] {
            assert!(ids.iter().any(|id| id == required));
        }
    }

    #[test]
    fn numeric_list_excludes_non_numeric_entries() {
        let numbers = numeric_required_nips().unwrap();
        assert!(numbers.contains(&9));
        assert!(numbers.contains(&98));
        assert!(!numbers.contains(&7));
    }
}
