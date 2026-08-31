use std::net::Ipv4Addr;

#[derive(Debug, Clone)]
pub struct TcpSegment {
    pub source_ip: Ipv4Addr,
    pub destination_ip: Ipv4Addr,
    pub source_port: u16,
    pub destination_port: u16,
    pub sequence: u32,
    pub syn: bool,
    pub fin: bool,
    pub rst: bool,
    pub payload: Vec<u8>,
}

pub fn parse_ipv4_tcp(packet: &[u8], observed_port: u16) -> Option<TcpSegment> {
    if packet.len() < 20 || packet[0] >> 4 != 4 {
        return None;
    }
    let ip_header_len = usize::from(packet[0] & 0x0f) * 4;
    if ip_header_len < 20
        || packet.len() < ip_header_len + 20
        || packet[9] != libc::IPPROTO_TCP as u8
    {
        return None;
    }
    // Non-initial IPv4 fragments do not contain a TCP header. AF_PACKET sees
    // the original fragments, so ignore them rather than misparse their body.
    let fragment = u16::from_be_bytes([packet[6], packet[7]]) & 0x1fff;
    if fragment != 0 {
        return None;
    }

    let total_len = usize::from(u16::from_be_bytes([packet[2], packet[3]])).min(packet.len());
    let tcp = &packet[ip_header_len..total_len];
    let source_port = u16::from_be_bytes([tcp[0], tcp[1]]);
    let destination_port = u16::from_be_bytes([tcp[2], tcp[3]]);
    if source_port != observed_port && destination_port != observed_port {
        return None;
    }
    let tcp_header_len = usize::from(tcp[12] >> 4) * 4;
    if tcp_header_len < 20 || tcp.len() < tcp_header_len {
        return None;
    }

    Some(TcpSegment {
        source_ip: Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]),
        destination_ip: Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]),
        source_port,
        destination_port,
        sequence: u32::from_be_bytes([tcp[4], tcp[5], tcp[6], tcp[7]]),
        syn: tcp[13] & 0x02 != 0,
        fin: tcp[13] & 0x01 != 0,
        rst: tcp[13] & 0x04 != 0,
        payload: tcp[tcp_header_len..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_ipv4_tcp;

    #[test]
    fn parses_ipv4_tcp_payload() {
        let payload = b"GET / HTTP/1.1\r\nHost: test\r\n\r\n";
        let mut packet = vec![0_u8; 40 + payload.len()];
        let packet_len = packet.len() as u16;
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&packet_len.to_be_bytes());
        packet[9] = 6;
        packet[12..16].copy_from_slice(&[10, 0, 0, 2]);
        packet[16..20].copy_from_slice(&[10, 0, 0, 1]);
        packet[20..22].copy_from_slice(&50_000_u16.to_be_bytes());
        packet[22..24].copy_from_slice(&80_u16.to_be_bytes());
        packet[24..28].copy_from_slice(&100_u32.to_be_bytes());
        packet[32] = 0x50;
        packet[33] = 0x18;
        packet[40..].copy_from_slice(payload);

        let segment = parse_ipv4_tcp(&packet, 80).unwrap();
        assert_eq!(segment.source_port, 50_000);
        assert_eq!(segment.destination_port, 80);
        assert_eq!(segment.payload, payload);
    }
}
