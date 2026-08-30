use hyper::HeaderMap;
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

pub fn serialize_headers(headers: &HeaderMap) -> String {
    let mut map = HashMap::new();

    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_ascii_lowercase();

        if BLACKLISTED_HEADERS.contains(&name_str.as_str()) {
            continue;
        }

        if ALLOWED_HEADERS.contains(&name_str.as_str())
            && let Ok(val_str) = value.to_str()
        {
            map.insert(name_str, val_str.to_string());
        }
    }

    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, HeaderValue, USER_AGENT};

    #[test]
    fn test_headers_allowlist_and_blacklist() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static("ReqLens-Agent/1.0"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer supersecrettoken"),
        );
        headers.insert(COOKIE, HeaderValue::from_static("session_id=12345"));

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
