pub mod headers;
pub mod redact;

use std::io::{Cursor, Read};

const GZIP_ENCODING: &str = "gzip";
const DEFLATE_ENCODING: &str = "deflate";
const BROTLI_ENCODING: &str = "br";

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
    body_bytes: &[u8],
    headers: &[(String, String)],
    max_body: usize,
    redact_enabled: bool,
) -> Option<String> {
    if body_bytes.is_empty() {
        return None;
    }

    let (decoded_body, is_truncated) = match decode_content(body_bytes, headers, max_body) {
        Some(body) => body,
        None => return Some(redact::COMPRESSED_MARKER.to_string()),
    };

    let text = match std::str::from_utf8(&decoded_body) {
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

fn decode_content(
    body_bytes: &[u8],
    headers: &[(String, String)],
    max_body: usize,
) -> Option<(Vec<u8>, bool)> {
    let encoding = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-encoding"))
        .map(|(_, value)| value.as_str());
    let Some(encoding) = encoding else {
        return Some(limit_body(body_bytes, max_body));
    };

    let mut decoded = body_bytes.to_vec();
    for encoding in encoding.split(',').rev().map(str::trim) {
        decoded = decode_encoding(&decoded, encoding, max_body)?;
    }
    Some(limit_body(&decoded, max_body))
}

fn decode_encoding(body_bytes: &[u8], encoding: &str, max_body: usize) -> Option<Vec<u8>> {
    match encoding.to_ascii_lowercase().as_str() {
        "" | "identity" => Some(body_bytes.to_vec()),
        GZIP_ENCODING => read_decoder(
            flate2::read::GzDecoder::new(Cursor::new(body_bytes)),
            max_body,
        ),
        DEFLATE_ENCODING => read_decoder(
            flate2::read::ZlibDecoder::new(Cursor::new(body_bytes)),
            max_body,
        )
        .or_else(|| {
            read_decoder(
                flate2::read::DeflateDecoder::new(Cursor::new(body_bytes)),
                max_body,
            )
        }),
        BROTLI_ENCODING => read_decoder(
            brotli::Decompressor::new(Cursor::new(body_bytes), 4096),
            max_body,
        ),
        _ => None,
    }
}

fn read_decoder(decoder: impl Read, max_body: usize) -> Option<Vec<u8>> {
    let mut decoded = Vec::new();
    decoder
        .take(u64::try_from(max_body).ok()?.saturating_add(1))
        .read_to_end(&mut decoded)
        .ok()?;
    Some(decoded)
}

fn limit_body(body_bytes: &[u8], max_body: usize) -> (Vec<u8>, bool) {
    let is_truncated = body_bytes.len() > max_body;
    let slice = if is_truncated {
        &body_bytes[..max_body]
    } else {
        body_bytes
    };
    (slice.to_vec(), is_truncated)
}

#[cfg(test)]
mod tests {
    use super::process_body;
    use crate::capture::redact;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    #[test]
    fn decodes_gzip_before_redacting() {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(br#"{"status":"ok","token":"secret"}"#)
            .unwrap();
        let compressed = encoder.finish().unwrap();
        let headers = vec![("Content-Encoding".into(), "gzip".into())];

        let body = process_body(&compressed, &headers, 1024, true).unwrap();

        assert!(body.contains("\"status\":\"ok\""));
        assert!(body.contains("\"token\":\"[REDACTED]\""));
    }

    #[test]
    fn rejects_unknown_content_encoding() {
        let headers = vec![("Content-Encoding".into(), "zstd".into())];

        assert_eq!(
            process_body(b"payload", &headers, 1024, true).as_deref(),
            Some(redact::COMPRESSED_MARKER)
        );
    }
}
