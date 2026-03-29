use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tag(pub Vec<String>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub pubkey: String,
    pub kind: u32,
    pub created_at: u64,
    pub tags: Vec<Tag>,
    pub content: String,
    pub sig: String,
}
