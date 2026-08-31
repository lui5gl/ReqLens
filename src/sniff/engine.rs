use super::TcpSegment;
use crate::capture::headers::serialize_headers;
use crate::capture::{HttpEvent, process_body};
use chrono::Utc;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

const MAX_FLOWS: usize = 16_384;
const MAX_STREAM_BUFFER: usize = 512 * 1024;
const MAX_OUT_OF_ORDER_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct FlowKey {
    client_ip: Ipv4Addr,
    client_port: u16,
    server_ip: Ipv4Addr,
    server_port: u16,
}

#[derive(Default)]
struct Direction {
    next_sequence: Option<u32>,
    pending: BTreeMap<u32, Vec<u8>>,
    pending_bytes: usize,
    stream: Vec<u8>,
}

struct PendingRequest {
    started: Instant,
    timestamp: String,
    client_ip: String,
    client_ua: Option<String>,
    method: String,
    path: String,
    query: Option<String>,
    headers: String,
    body: Option<String>,
}

struct Flow {
    client_to_server: Direction,
    server_to_client: Direction,
    requests: VecDeque<PendingRequest>,
    last_seen: Instant,
}

impl Flow {
    fn new() -> Self {
        Self {
            client_to_server: Direction::default(),
            server_to_client: Direction::default(),
            requests: VecDeque::new(),
            last_seen: Instant::now(),
        }
    }
}

pub struct SniffEngine {
    server_ip: Option<Ipv4Addr>,
    port: u16,
    max_body: usize,
    redact_enabled: bool,
    flows: HashMap<FlowKey, Flow>,
}

impl SniffEngine {
    pub fn new(
        server_ip: Option<Ipv4Addr>,
        port: u16,
        max_body: usize,
        redact_enabled: bool,
    ) -> Self {
        Self {
            server_ip,
            port,
            max_body,
            redact_enabled,
            flows: HashMap::new(),
        }
    }

    pub fn process(&mut self, segment: TcpSegment) -> Vec<HttpEvent> {
        let Some((key, from_client)) = self.classify(&segment) else {
            return Vec::new();
        };

        if segment.rst && segment.payload.is_empty() {
            self.flows.remove(&key);
            return Vec::new();
        }

        if !self.flows.contains_key(&key) && self.flows.len() >= MAX_FLOWS {
            self.expire_oldest();
        }
        let flow = self.flows.entry(key.clone()).or_insert_with(Flow::new);
        flow.last_seen = Instant::now();
        let direction = if from_client {
            &mut flow.client_to_server
        } else {
            &mut flow.server_to_client
        };
        direction.push(&segment);

        let mut events = Vec::new();
        if from_client {
            while let Some(message) =
                take_http_message(&mut flow.client_to_server.stream, true, false, segment.fin)
            {
                if let Some(request) =
                    parse_request(&message, key.client_ip, self.max_body, self.redact_enabled)
                {
                    flow.requests.push_back(request);
                }
            }
        } else {
            while let Some(message) = take_http_message(
                &mut flow.server_to_client.stream,
                false,
                flow.requests
                    .front()
                    .is_some_and(|request| request.method == "HEAD"),
                segment.fin,
            ) {
                // Informational responses do not complete the request/response
                // exchange. 101 is the explicit exception: it finalizes the
                // HTTP request before the connection switches protocols.
                if response_status(&message)
                    .is_some_and(|status| (100..200).contains(&status) && status != 101)
                {
                    continue;
                }
                let Some(request) = flow.requests.pop_front() else {
                    continue;
                };
                if let Some(event) =
                    parse_response(&message, request, self.max_body, self.redact_enabled)
                {
                    events.push(event);
                }
            }
        }

        if segment.rst {
            self.flows.remove(&key);
        }
        events
    }

    pub fn expire_idle(&mut self, timeout: Duration) {
        self.flows
            .retain(|_, flow| flow.last_seen.elapsed() < timeout);
    }

    fn classify(&self, segment: &TcpSegment) -> Option<(FlowKey, bool)> {
        let from_client = segment.destination_port == self.port
            && self
                .server_ip
                .is_none_or(|server| segment.destination_ip == server);
        let from_server = segment.source_port == self.port
            && self
                .server_ip
                .is_none_or(|server| segment.source_ip == server);

        if from_client {
            Some((
                FlowKey {
                    client_ip: segment.source_ip,
                    client_port: segment.source_port,
                    server_ip: segment.destination_ip,
                    server_port: segment.destination_port,
                },
                true,
            ))
        } else if from_server {
            Some((
                FlowKey {
                    client_ip: segment.destination_ip,
                    client_port: segment.destination_port,
                    server_ip: segment.source_ip,
                    server_port: segment.source_port,
                },
                false,
            ))
        } else {
            None
        }
    }

    fn expire_oldest(&mut self) {
        if let Some(key) = self
            .flows
            .iter()
            .min_by_key(|(_, flow)| flow.last_seen)
            .map(|(key, _)| key.clone())
        {
            self.flows.remove(&key);
        }
    }
}

impl Direction {
    fn push(&mut self, segment: &TcpSegment) {
        let mut sequence = segment.sequence.wrapping_add(u32::from(segment.syn));
        let mut payload = segment.payload.as_slice();
        if payload.is_empty() {
            // Only SYN provides a reliable sequence origin. An empty ACK may
            // come from a connection established before capture started; using
            // it would make later payload look permanently out of order.
            if segment.syn {
                self.next_sequence.get_or_insert(sequence);
            }
            return;
        }

        let expected = *self.next_sequence.get_or_insert(sequence);
        if sequence < expected {
            let already_seen = expected.wrapping_sub(sequence) as usize;
            if already_seen >= payload.len() {
                return;
            }
            payload = &payload[already_seen..];
            sequence = expected;
        }

        if sequence == expected {
            self.append(payload);
            self.next_sequence = Some(expected.wrapping_add(payload.len() as u32));
            self.drain_pending();
        } else if self.pending_bytes + payload.len() <= MAX_OUT_OF_ORDER_BYTES
            && let std::collections::btree_map::Entry::Vacant(entry) = self.pending.entry(sequence)
        {
            self.pending_bytes += payload.len();
            entry.insert(payload.to_vec());
        }
    }

    fn append(&mut self, payload: &[u8]) {
        if self.stream.len() + payload.len() > MAX_STREAM_BUFFER {
            self.stream.clear();
            self.pending.clear();
            self.pending_bytes = 0;
        } else {
            self.stream.extend_from_slice(payload);
        }
    }

    fn drain_pending(&mut self) {
        loop {
            let Some(expected) = self.next_sequence else {
                return;
            };
            let Some(payload) = self.pending.remove(&expected) else {
                return;
            };
            self.pending_bytes = self.pending_bytes.saturating_sub(payload.len());
            self.append(&payload);
            self.next_sequence = Some(expected.wrapping_add(payload.len() as u32));
        }
    }
}

fn header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|p| p + 4)
}

fn response_status(message: &[u8]) -> Option<u16> {
    let header_len = header_end(message)?;
    let mut header_slots = [httparse::EMPTY_HEADER; 64];
    let mut response = httparse::Response::new(&mut header_slots);
    response.parse(&message[..header_len]).ok()?;
    response.code
}

fn take_http_message(
    stream: &mut Vec<u8>,
    request: bool,
    force_no_body: bool,
    end_of_stream: bool,
) -> Option<Vec<u8>> {
    let end = header_end(stream)?;
    let header_text = std::str::from_utf8(&stream[..end]).ok()?;
    if request && !header_text.contains(" HTTP/1.") {
        stream.clear();
        return None;
    }
    if !request && !header_text.starts_with("HTTP/1.") {
        stream.clear();
        return None;
    }

    let content_length = header_text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    });
    let chunked = header_text.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("transfer-encoding")
                && value.to_ascii_lowercase().contains("chunked")
        })
    });

    let response_has_no_body = !request
        && (force_no_body
            || header_text
                .split_whitespace()
                .nth(1)
                .and_then(|status| status.parse::<u16>().ok())
                .is_some_and(|status| {
                    (100..200).contains(&status) || status == 204 || status == 304
                }));
    let length = if chunked {
        let body = &stream[end..];
        let terminal = body.windows(5).position(|window| window == b"0\r\n\r\n")?;
        end + terminal + 5
    } else if let Some(content_length) = content_length {
        end + content_length
    } else if request || response_has_no_body {
        end
    } else if end_of_stream {
        stream.len()
    } else {
        return None;
    };
    (stream.len() >= length).then(|| stream.drain(..length).collect())
}

fn parse_request(
    message: &[u8],
    client_ip: Ipv4Addr,
    max_body: usize,
    redact_enabled: bool,
) -> Option<PendingRequest> {
    let header_len = header_end(message)?;
    let mut header_slots = [httparse::EMPTY_HEADER; 64];
    let mut parsed = httparse::Request::new(&mut header_slots);
    parsed.parse(&message[..header_len]).ok()?;
    let method = parsed.method?.to_string();
    let raw_path = parsed.path?.to_string();
    let (path, query) = raw_path
        .split_once('?')
        .map_or((raw_path.clone(), None), |(path, query)| {
            (path.to_string(), Some(query.to_string()))
        });
    let headers = parsed
        .headers
        .iter()
        .filter_map(|header| {
            Some((
                header.name.to_string(),
                std::str::from_utf8(header.value).ok()?.to_string(),
            ))
        })
        .collect::<Vec<_>>();
    let client_ua = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
        .map(|(_, value)| value.clone());

    Some(PendingRequest {
        started: Instant::now(),
        timestamp: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        client_ip: client_ip.to_string(),
        client_ua,
        method,
        path,
        query,
        headers: serialize_headers(&headers),
        body: process_body(&message[header_len..], &headers, max_body, redact_enabled),
    })
}

fn parse_response(
    message: &[u8],
    request: PendingRequest,
    max_body: usize,
    redact_enabled: bool,
) -> Option<HttpEvent> {
    let header_len = header_end(message)?;
    let mut header_slots = [httparse::EMPTY_HEADER; 64];
    let mut parsed = httparse::Response::new(&mut header_slots);
    parsed.parse(&message[..header_len]).ok()?;
    let headers = parsed
        .headers
        .iter()
        .filter_map(|header| {
            Some((
                header.name.to_string(),
                std::str::from_utf8(header.value).ok()?.to_string(),
            ))
        })
        .collect::<Vec<_>>();

    Some(HttpEvent {
        timestamp: request.timestamp,
        duration_ms: request.started.elapsed().as_millis() as i64,
        client_ip: request.client_ip,
        client_ua: request.client_ua,
        method: request.method,
        path: request.path,
        query: request.query,
        req_headers: request.headers,
        req_body: request.body,
        resp_status: parsed.code?,
        resp_headers: serialize_headers(&headers),
        resp_body: process_body(&message[header_len..], &headers, max_body, redact_enabled),
    })
}

#[cfg(test)]
mod tests {
    use super::SniffEngine;
    use crate::sniff::TcpSegment;
    use std::net::Ipv4Addr;

    fn segment(from_client: bool, sequence: u32, payload: &[u8]) -> TcpSegment {
        let client = Ipv4Addr::new(10, 0, 0, 2);
        let server = Ipv4Addr::new(10, 0, 0, 1);
        TcpSegment {
            source_ip: if from_client { client } else { server },
            destination_ip: if from_client { server } else { client },
            source_port: if from_client { 50_000 } else { 80 },
            destination_port: if from_client { 80 } else { 50_000 },
            sequence,
            syn: false,
            fin: false,
            rst: false,
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn correlates_split_request_and_response() {
        let mut engine = SniffEngine::new(Some(Ipv4Addr::new(10, 0, 0, 1)), 80, 65_536, true);
        let request_a =
            b"POST /save HTTP/1.1\r\nHost: test\r\nContent-Length: 15\r\n\r\npassword=se";
        let request_b = b"cret";
        assert!(engine.process(segment(true, 100, request_a)).is_empty());
        assert!(
            engine
                .process(segment(true, 100 + request_a.len() as u32, request_b))
                .is_empty()
        );

        let response =
            b"HTTP/1.1 500 Error\r\nContent-Length: 4\r\nContent-Type: text/plain\r\n\r\nfail";
        let events = engine.process(segment(false, 900, response));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].path, "/save");
        assert_eq!(events[0].resp_status, 500);
        assert_eq!(
            events[0].req_body.as_deref(),
            Some("password=\"[REDACTED]\"")
        );
        assert_eq!(events[0].resp_body.as_deref(), Some("fail"));
    }

    #[test]
    fn reassembles_out_of_order_segments_after_sequence_is_known() {
        let mut engine = SniffEngine::new(None, 80, 65_536, true);
        let first = b"GET / HTTP/1.1\r\n";
        let last = b"Host: test\r\n\r\n";
        engine.process(segment(true, 10, first));
        engine.process(segment(true, 10 + first.len() as u32 + 4, &last[4..]));
        engine.process(segment(true, 10 + first.len() as u32, &last[..4]));
        let response = b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n";
        assert_eq!(engine.process(segment(false, 90, response)).len(), 1);
    }

    #[test]
    fn waits_for_fin_before_completing_close_delimited_response() {
        let mut engine = SniffEngine::new(None, 80, 65_536, true);
        engine.process(segment(true, 10, b"GET /old HTTP/1.0\r\n\r\n"));
        let response = b"HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\n\r\nlegacy body";
        assert!(engine.process(segment(false, 90, response)).is_empty());
        let mut fin = segment(false, 90 + response.len() as u32, b"");
        fin.fin = true;
        let events = engine.process(fin);
        assert_eq!(events[0].resp_body.as_deref(), Some("legacy body"));
    }

    #[test]
    fn captures_existing_connection_after_empty_ack() {
        let mut engine = SniffEngine::new(Some(Ipv4Addr::new(10, 0, 0, 1)), 80, 65_536, true);

        // Capture starts after the browser has already established keep-alive.
        assert!(engine.process(segment(true, 100, b"")).is_empty());
        let request = b"GET /browser HTTP/1.1\r\nHost: test\r\nConnection: keep-alive\r\n\r\n";
        assert!(engine.process(segment(true, 5_000, request)).is_empty());

        // The reverse direction can also first appear as an unrelated ACK.
        assert!(engine.process(segment(false, 200, b"")).is_empty());
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
        let events = engine.process(segment(false, 9_000, response));

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].path, "/browser");
        assert_eq!(events[0].resp_status, 200);
        assert_eq!(events[0].resp_body.as_deref(), Some("OK"));
    }

    #[test]
    fn informational_response_does_not_consume_pending_request() {
        let mut engine = SniffEngine::new(None, 80, 65_536, true);
        let request = b"POST /guardar HTTP/1.1\r\nHost: test\r\nContent-Length: 4\r\nExpect: 100-continue\r\n\r\ndata";
        assert!(engine.process(segment(true, 100, request)).is_empty());

        let informational = b"HTTP/1.1 100 Continue\r\n\r\n";
        assert!(
            engine
                .process(segment(false, 500, informational))
                .is_empty()
        );

        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
        let events = engine.process(segment(false, 500 + informational.len() as u32, response));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].method, "POST");
        assert_eq!(events[0].path, "/guardar");
        assert_eq!(events[0].resp_status, 200);
        assert_eq!(events[0].resp_body.as_deref(), Some("OK"));
    }
}
