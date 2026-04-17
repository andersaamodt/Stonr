use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Filter {
    pub ids: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub kinds: Option<Vec<u32>>,
    pub d: Option<String>,
    pub t: Option<String>,
    pub tags: Vec<(String, Vec<String>)>,
    pub search: Option<String>,
    pub since: Option<u64>,
    pub until: Option<u64>,
    pub limit: Option<usize>,
}

impl Filter {
    pub fn from_value(val: &Value) -> Self {
        let ids = val.get("ids").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        });
        let authors = val.get("authors").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        });
        let kinds = val.get("kinds").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_u64().map(|u| u as u32))
                .collect()
        });
        let d = val
            .get("#d")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let t = val
            .get("#t")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let tags = val
            .as_object()
            .map(|obj| {
                obj.iter()
                    .filter_map(|(key, value)| {
                        let tag_key = key.strip_prefix('#')?;
                        if tag_key == "d" || tag_key == "t" {
                            return None;
                        }
                        let values = value
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|item| item.as_str().map(str::to_string))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        if values.is_empty() {
                            None
                        } else {
                            Some((tag_key.to_string(), values))
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let since = val.get("since").and_then(|v| v.as_u64());
        let until = val.get("until").and_then(|v| v.as_u64());
        let search = val
            .get("search")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        let limit = val
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|value| value as usize);
        Self {
            ids,
            authors,
            kinds,
            d,
            t,
            tags,
            search,
            since,
            until,
            limit,
        }
    }

    pub fn has_tag_filters(&self) -> bool {
        self.d.is_some() || self.t.is_some() || !self.tags.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::Filter;

    #[test]
    fn parses_filter_with_tags_and_bounds() {
        let value = serde_json::json!({
            "ids": ["event-1"],
            "authors": ["a"],
            "kinds": [1, 30023],
            "#d": ["slug"],
            "#p": ["peer"],
            "search": "hello",
            "since": 10,
            "until": 20,
            "limit": 30
        });
        let filter = Filter::from_value(&value);
        assert_eq!(filter.ids.as_deref(), Some(&["event-1".to_string()][..]));
        assert_eq!(filter.authors.as_deref(), Some(&["a".to_string()][..]));
        assert_eq!(filter.kinds.as_deref(), Some(&[1, 30023][..]));
        assert_eq!(filter.d.as_deref(), Some("slug"));
        assert_eq!(filter.tags[0].0, "p");
        assert_eq!(filter.search.as_deref(), Some("hello"));
        assert_eq!(filter.since, Some(10));
        assert_eq!(filter.until, Some(20));
        assert_eq!(filter.limit, Some(30));
    }
}
