use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

pub const REDACTED_MARKER: &str = "[REDACTED]";
pub const TRUNCATED_MARKER: &str = "[TRUNCATED]";
pub const BINARY_MARKER: &str = "[BINARY]";
pub const COMPRESSED_MARKER: &str = "[COMPRESSED]";

const SENSITIVE_KEYS: &[&str] = &[
    "password",
    "pass",
    "token",
    "secret",
    "api_key",
    "apikey",
    "authorization",
    "auth",
    "access_token",
    "refresh_token",
    "private_key",
    "client_secret",
    "credit_card",
];

static SENSITIVE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(password|pass|token|secret|api_key|apikey|authorization|auth|access_token|refresh_token|private_key|client_secret|credit_card)["']?\s*[:=]\s*["']?([^"'\s&,;]+)["']?"#)
        .expect("Valid regex compilation")
});

pub fn redact_payload(content: &str) -> String {
    if let Ok(mut json_val) = serde_json::from_str::<Value>(content) {
        redact_json_value(&mut json_val);
        return json_val.to_string();
    }
    redact_text_fallback(content)
}

fn redact_json_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if val.is_object() || val.is_array() {
                    redact_json_value(val);
                } else if is_sensitive_key(key) {
                    *val = Value::String(REDACTED_MARKER.to_string());
                }
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_json_value(item);
            }
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|&s| lower.contains(s))
}

fn redact_text_fallback(text: &str) -> String {
    SENSITIVE_REGEX
        .replace_all(text, |caps: &regex::Captures| {
            let key = &caps[1];
            format!("{}=\"{}\"", key, REDACTED_MARKER)
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_redaction() {
        let payload =
            r#"{"username":"alice","password":"secret123","token":"jwt.abc.xyz","age":30}"#;
        let redacted = redact_payload(payload);
        let parsed: Value = serde_json::from_str(&redacted).expect("Valid JSON");

        assert_eq!(parsed["username"], "alice");
        assert_eq!(parsed["password"], REDACTED_MARKER);
        assert_eq!(parsed["token"], REDACTED_MARKER);
        assert_eq!(parsed["age"], 30);
    }

    #[test]
    fn test_nested_json_redaction() {
        let payload = r#"{"auth":{"api_key":"sk-999","active":true}}"#;
        let redacted = redact_payload(payload);
        let parsed: Value = serde_json::from_str(&redacted).expect("Valid JSON");

        assert_eq!(parsed["auth"]["api_key"], REDACTED_MARKER);
        assert_eq!(parsed["auth"]["active"], true);
    }

    #[test]
    fn test_text_fallback_redaction() {
        let text = "username=admin&password=mysecretpassword123&action=login";
        let redacted = redact_payload(text);
        assert!(redacted.contains("password=\"[REDACTED]\""));
        assert!(!redacted.contains("mysecretpassword123"));
    }
}
