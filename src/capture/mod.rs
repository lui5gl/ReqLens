pub mod headers;
pub mod redact;

use bytes::Bytes;
use hyper::HeaderMap;

#[derive(Debug, Clone)]
pub struct HttpEvent {
    pub timestamp: String,
    pub duration_ms: i64,
    pub client_ip: String,
    pub client_ua: Option<String>,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub req_headers: String,
    pub req_body: Option<String>,
    pub resp_status: u16,
    pub resp_headers: String,
    pub resp_body: Option<String>,
}

pub fn process_body(
    body_bytes: &Bytes,
    headers: &HeaderMap,
    max_body: usize,
    redact_enabled: bool,
) -> Option<String> {
    if body_bytes.is_empty() {
        return None;
    }

    if let Some(encoding) = headers.get("content-encoding")
        && let Ok(enc_str) = encoding.to_str()
    {
        let enc_lower = enc_str.to_ascii_lowercase();
        if enc_lower.contains("gzip") || enc_lower.contains("br") || enc_lower.contains("deflate") {
            return Some(redact::COMPRESSED_MARKER.to_string());
        }
    }

    let is_truncated = body_bytes.len() > max_body;
    let slice = if is_truncated {
        &body_bytes[..max_body]
    } else {
        body_bytes.as_ref()
    };

    let text = match std::str::from_utf8(slice) {
        Ok(valid_str) => valid_str,
        Err(_) => return Some(redact::BINARY_MARKER.to_string()),
    };

    let processed = if redact_enabled {
        redact::redact_payload(text)
    } else {
        text.to_string()
    };

    if is_truncated {
        Some(format!("{} {}", processed, redact::TRUNCATED_MARKER))
    } else {
        Some(processed)
    }
}
