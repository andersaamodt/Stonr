//! Minimal structured relay logging.

use serde_json::{Map, Value};

use crate::policy::current_unix_ts;

pub fn warn(component: &str, message: &str, fields: Value) {
    emit("warn", component, message, fields);
}

pub fn error(component: &str, message: &str, fields: Value) {
    emit("error", component, message, fields);
}

fn emit(level: &str, component: &str, message: &str, fields: Value) {
    eprintln!("{}", serde_json::to_string(&entry(level, component, message, fields)).unwrap_or_else(|_| {
        format!(
            r#"{{"ts":{},"level":"{}","component":"{}","message":"{}"}}"#,
            current_unix_ts(),
            level,
            component,
            message.replace('"', "'"),
        )
    }));
}

fn entry(level: &str, component: &str, message: &str, fields: Value) -> Value {
    let mut obj = Map::new();
    obj.insert("ts".into(), Value::Number(current_unix_ts().into()));
    obj.insert("level".into(), Value::String(level.to_string()));
    obj.insert("component".into(), Value::String(component.to_string()));
    obj.insert("message".into(), Value::String(message.to_string()));
    if let Value::Object(extra) = fields {
        for (key, value) in extra {
            obj.insert(key, value);
        }
    }
    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_includes_base_and_extra_fields() {
        let value = entry(
            "warn",
            "mirror",
            "upstream failed",
            serde_json::json!({
                "relay": "wss://example.com",
                "error": "boom",
            }),
        );
        assert_eq!(value["level"], "warn");
        assert_eq!(value["component"], "mirror");
        assert_eq!(value["message"], "upstream failed");
        assert_eq!(value["relay"], "wss://example.com");
        assert_eq!(value["error"], "boom");
        assert!(value["ts"].as_u64().is_some());
    }
}
