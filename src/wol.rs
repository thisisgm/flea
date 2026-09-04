use std::net::UdpSocket;

const MAGIC_LEN: usize = 6 + 16 * 6;

pub fn normalize_mac(raw: &str) -> Result<[u8; 6], String> {
    let compact: String = raw.chars().filter(|c| *c != ':' && *c != '-').collect();
    if compact.len() != 12 || !compact.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Wake needs a MAC address like aa:bb:cc:dd:ee:ff".to_string());
    }
    let mut mac = [0u8; 6];
    for (i, byte) in mac.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&compact[i * 2..i * 2 + 2], 16)
            .map_err(|_| "Wake needs a valid hexadecimal MAC address".to_string())?;
    }
    Ok(mac)
}

pub fn magic_packet(mac: [u8; 6]) -> [u8; MAGIC_LEN] {
    let mut packet = [0u8; MAGIC_LEN];
    packet[..6].fill(0xff);
    for chunk in packet[6..].chunks_exact_mut(6) {
        chunk.copy_from_slice(&mac);
    }
    packet
}

pub fn wake(raw: &str) -> Result<(), String> {
    let mac = normalize_mac(raw)?;
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("could not open a Wake socket: {e}"))?;
    socket.set_broadcast(true).map_err(|e| format!("could not enable broadcast: {e}"))?;
    socket.send_to(&magic_packet(mac), "255.255.255.255:9")
        .map_err(|e| format!("could not send the Wake packet: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_colon_dash_and_compact_mac_addresses() {
        let want = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];
        assert_eq!(normalize_mac("aa:bb:cc:dd:ee:ff").unwrap(), want);
        assert_eq!(normalize_mac("AA-BB-CC-DD-EE-FF").unwrap(), want);
        assert_eq!(normalize_mac("aabbccddeeff").unwrap(), want);
    }

    #[test]
    fn rejects_wrong_length_non_hex_and_unrecognised_separators() {
        assert!(normalize_mac("aa:bb").is_err());
        assert!(normalize_mac("gg:bb:cc:dd:ee:ff").is_err());
        assert!(normalize_mac("aa.bb.cc.dd.ee.ff").is_err());
    }

    #[test]
    fn a_magic_packet_has_six_ff_bytes_then_sixteen_mac_copies() {
        let mac = [0, 1, 2, 3, 4, 5];
        let packet = magic_packet(mac);
        assert_eq!(packet.len(), 102);
        assert_eq!(&packet[..6], &[0xff; 6]);
        for chunk in packet[6..].chunks_exact(6) {
            assert_eq!(chunk, mac);
        }
    }
}
