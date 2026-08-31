use std::net::Ipv4Addr;

const ETHERNET_HEADER_LEN: usize = 14;
const ETHER_TYPE_IPV4: u16 = 0x0800;
const ETHER_TYPE_VLAN: u16 = 0x8100;
const ETHER_TYPE_QINQ: u16 = 0x88A8;
const VLAN_TAG_LEN: usize = 4;
const IPV4_MIN_HEADER_LEN: usize = 20;
const TCP_MIN_HEADER_LEN: usize = 20;

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
    let ipv4_offset = ipv4_offset(packet)?;
    let packet = &packet[ipv4_offset..];
    if packet.len() < IPV4_MIN_HEADER_LEN || packet[0] >> 4 != 4 {
        return None;
    }
    let ip_header_len = usize::from(packet[0] & 0x0f) * 4;
    if ip_header_len < IPV4_MIN_HEADER_LEN
        || packet.len() < ip_header_len + TCP_MIN_HEADER_LEN
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
    if total_len < ip_header_len + TCP_MIN_HEADER_LEN {
        return None;
    }
    let tcp = &packet[ip_header_len..total_len];
    let source_port = u16::from_be_bytes([tcp[0], tcp[1]]);
    let destination_port = u16::from_be_bytes([tcp[2], tcp[3]]);
    if source_port != observed_port && destination_port != observed_port {
        return None;
    }
    let tcp_header_len = usize::from(tcp[12] >> 4) * 4;
    if tcp_header_len < TCP_MIN_HEADER_LEN || tcp.len() < tcp_header_len {
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

fn ipv4_offset(packet: &[u8]) -> Option<usize> {
    if packet.len() >= IPV4_MIN_HEADER_LEN && packet[0] >> 4 == 4 {
        return Some(0);
    }
    if packet.len() < ETHERNET_HEADER_LEN {
        return None;
    }

    let mut offset = ETHERNET_HEADER_LEN;
    let mut ether_type = u16::from_be_bytes([packet[12], packet[13]]);
    while ether_type == ETHER_TYPE_VLAN || ether_type == ETHER_TYPE_QINQ {
        if packet.len() < offset + VLAN_TAG_LEN {
            return None;
        }
        ether_type = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]);
        offset += VLAN_TAG_LEN;
    }
    (ether_type == ETHER_TYPE_IPV4).then_some(offset)
}

#[cfg(test)]
mod tests {
    use super::parse_ipv4_tcp;

    const HTTP_PORT: u16 = 80;
    const CLIENT_PORT: u16 = 50_000;
    const OTHER_PORT: u16 = 8_080;
    const IPV4_HEADER_LEN: usize = 20;
    const TCP_HEADER_LEN: usize = 20;
    const ETHER_TYPE_IPV4: u16 = 0x0800;
    const ETHER_TYPE_VLAN: u16 = 0x8100;

    fn ipv4_tcp_packet(destination_port: u16) -> Vec<u8> {
        let payload = b"GET / HTTP/1.1\r\nHost: test\r\n\r\n";
        let mut packet = vec![0_u8; IPV4_HEADER_LEN + TCP_HEADER_LEN + payload.len()];
        let packet_len = packet.len() as u16;
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&packet_len.to_be_bytes());
        packet[9] = libc::IPPROTO_TCP as u8;
        packet[12..16].copy_from_slice(&[10, 0, 0, 2]);
        packet[16..20].copy_from_slice(&[10, 0, 0, 1]);
        packet[20..22].copy_from_slice(&CLIENT_PORT.to_be_bytes());
        packet[22..24].copy_from_slice(&destination_port.to_be_bytes());
        packet[24..28].copy_from_slice(&100_u32.to_be_bytes());
        packet[32] = 0x50;
        packet[33] = 0x18;
        packet[40..].copy_from_slice(payload);
        packet
    }

    fn ethernet_frame(ether_types: &[u16], ipv4_packet: &[u8]) -> Vec<u8> {
        let vlan_count = ether_types.len().saturating_sub(1);
        let mut frame = vec![0_u8; 14 + vlan_count * 4];
        frame[12..14].copy_from_slice(&ether_types[0].to_be_bytes());
        for (index, ether_type) in ether_types[1..].iter().enumerate() {
            let offset = 14 + index * 4;
            frame[offset + 2..offset + 4].copy_from_slice(&ether_type.to_be_bytes());
        }
        frame.extend_from_slice(ipv4_packet);
        frame
    }

    #[test]
    fn parses_direct_ipv4_tcp() {
        let packet = ipv4_tcp_packet(HTTP_PORT);

        let segment = parse_ipv4_tcp(&packet, HTTP_PORT).unwrap();
        assert_eq!(segment.source_port, CLIENT_PORT);
        assert_eq!(segment.destination_port, HTTP_PORT);
    }

    #[test]
    fn parses_ethernet_ipv4_tcp_for_observed_port() {
        let packet = ethernet_frame(&[ETHER_TYPE_IPV4], &ipv4_tcp_packet(HTTP_PORT));

        let segment = parse_ipv4_tcp(&packet, HTTP_PORT).unwrap();
        assert_eq!(segment.destination_port, HTTP_PORT);
    }

    #[test]
    fn parses_vlan_ethernet_ipv4_tcp_for_observed_port() {
        let packet = ethernet_frame(
            &[ETHER_TYPE_VLAN, ETHER_TYPE_IPV4],
            &ipv4_tcp_packet(HTTP_PORT),
        );

        let segment = parse_ipv4_tcp(&packet, HTTP_PORT).unwrap();
        assert_eq!(segment.destination_port, HTTP_PORT);
    }

    #[test]
    fn rejects_tcp_for_another_port() {
        let packet = ethernet_frame(&[ETHER_TYPE_IPV4], &ipv4_tcp_packet(OTHER_PORT));

        assert!(parse_ipv4_tcp(&packet, HTTP_PORT).is_none());
    }
}
