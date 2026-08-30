use std::collections::HashMap;

const BLACKLISTED_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "set-cookie",
    "proxy-authorization",
    "proxy-authenticate",
];

const ALLOWED_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "accept",
    "user-agent",
    "referer",
    "origin",
    "host",
    "x-request-id",
    "x-forwarded-for",
    "x-forwarded-proto",
];

pub fn serialize_headers(headers: &[(String, String)]) -> String {
    let mut map = HashMap::new();

    for (name, value) in headers {
        let name_str = name.to_ascii_lowercase();

        if BLACKLISTED_HEADERS.contains(&name_str.as_str()) {
            continue;
        }

        if ALLOWED_HEADERS.contains(&name_str.as_str()) {
            map.insert(name_str, value.clone());
        }
    }

    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headers_allowlist_and_blacklist() {
        let headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("User-Agent".to_string(), "ReqLens-Agent/1.0".to_string()),
            ("Authorization".to_string(), "Bearer secret".to_string()),
            ("Cookie".to_string(), "session=123".to_string()),
        ];

        let serialized = serialize_headers(&headers);
        let parsed: HashMap<String, String> =
            serde_json::from_str(&serialized).expect("Valid JSON");

        assert_eq!(
            parsed.get("content-type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(
            parsed.get("user-agent"),
            Some(&"ReqLens-Agent/1.0".to_string())
        );
        assert_eq!(parsed.get("authorization"), None);
        assert_eq!(parsed.get("cookie"), None);
    }
}
