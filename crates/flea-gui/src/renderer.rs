use flea_gui::driver_notice::DriverNotice;
use std::os::unix::net::UnixDatagram;

pub const UNAVAILABLE: i32 = 1;
pub const MESSAGE: &str = "flea: no graphics driver could open a window; install or repair vulkan-swrast and mesa, or run flea --tui";

// stderr and the journal stay English log lines; the desktop notice is in the user's language.
pub fn unavailable(error: &str, refusals: &[String], notice: &DriverNotice) {
    report(MESSAGE);
    report(error);
    for refusal in refusals {
        report(refusal);
    }
    notice.send();
}

fn report(message: &str) {
    eprintln!("{message}");
    let Ok(socket) = UnixDatagram::unbound() else {
        return;
    };
    let _ = socket.set_nonblocking(true);
    let syslog = format!("<11>flea[{}]: {message}", std::process::id());
    if socket.send_to(syslog.as_bytes(), "/dev/log").is_err() {
        let native = native_payload(message);
        let _ = socket.send_to(&native, "/run/systemd/journal/socket");
    }
}

fn native_payload(message: &str) -> Vec<u8> {
    let mut payload = b"PRIORITY=3\nSYSLOG_IDENTIFIER=flea\n".to_vec();
    if message.as_bytes().contains(&b'\n') {
        payload.extend_from_slice(b"MESSAGE\n");
        payload.extend_from_slice(&(message.len() as u64).to_le_bytes());
    } else {
        payload.extend_from_slice(b"MESSAGE=");
    }
    payload.extend_from_slice(message.as_bytes());
    payload.push(b'\n');
    payload
}

#[cfg(test)]
mod protocol_tests {
    use super::native_payload;

    const BINARY_LENGTH_BYTES: usize = std::mem::size_of::<u64>();

    type Fields<'a> = Vec<(&'a [u8], &'a [u8])>;

    // Sample input: b"MESSAGE=ok\nMESSAGE\n\x02\0\0\0\0\0\0\0a\n\n".
    fn fields(mut payload: &[u8]) -> Result<Fields<'_>, &'static str> {
        let mut result = Vec::new();
        while !payload.is_empty() {
            let end = payload
                .iter()
                .position(|byte| *byte == b'\n')
                .ok_or("missing field newline")?;
            let line = &payload[..end];
            payload = &payload[end + 1..];
            if let Some(equal) = line.iter().position(|byte| *byte == b'=') {
                result.push((&line[..equal], &line[equal + 1..]));
            } else {
                let length: [u8; BINARY_LENGTH_BYTES] = payload
                    .get(..BINARY_LENGTH_BYTES)
                    .ok_or("missing binary length")?
                    .try_into()
                    .unwrap();
                let length =
                    usize::try_from(u64::from_le_bytes(length)).map_err(|_| "length overflow")?;
                payload = &payload[BINARY_LENGTH_BYTES..];
                let value = payload
                    .get(..length)
                    .ok_or("binary length exceeds payload")?;
                if payload.get(length) != Some(&b'\n') {
                    return Err("missing binary value terminator");
                }
                result.push((line, value));
                payload = &payload[length + 1..];
            }
        }
        Ok(result)
    }

    fn exact_message(payload: &[u8], message: &str) -> Result<(), &'static str> {
        let entries = fields(payload)?;
        if entries
            != [
                (b"PRIORITY".as_slice(), b"3".as_slice()),
                (b"SYSLOG_IDENTIFIER".as_slice(), b"flea".as_slice()),
                (b"MESSAGE".as_slice(), message.as_bytes()),
            ]
        {
            return Err("expected exactly priority, identifier and complete MESSAGE");
        }
        Ok(())
    }

    #[test]
    fn single_line_bytes_remain_identical() {
        assert_eq!(
            native_payload("adapter rejected"),
            b"PRIORITY=3\nSYSLOG_IDENTIFIER=flea\nMESSAGE=adapter rejected\n"
        );
        assert_eq!(
            native_payload(""),
            b"PRIORITY=3\nSYSLOG_IDENTIFIER=flea\nMESSAGE=\n"
        );
        assert_eq!(
            native_payload("fenêtre"),
            "PRIORITY=3\nSYSLOG_IDENTIFIER=flea\nMESSAGE=fenêtre\n".as_bytes()
        );
        println!("native protocol control: single-line, empty and UTF-8 bytes unchanged");
    }

    #[test]
    fn nul_without_newline_remains_literal() {
        let message = "adapter\0rejected";
        let payload = native_payload(message);
        assert_eq!(
            payload,
            b"PRIORITY=3\nSYSLOG_IDENTIFIER=flea\nMESSAGE=adapter\0rejected\n"
        );
        assert_eq!(exact_message(&payload, message), Ok(()));
        println!("native protocol control: single-line embedded NUL preserved");
    }

    #[test]
    fn multiline_message_uses_native_binary_framing() {
        let long = format!("{}\n{}", "é".repeat(130), "界".repeat(9));
        for (case, message) in [
            (
                "reviewed-multiline",
                "Failed to open window\nadapter rejected",
            ),
            ("utf8-multiline", "fenêtre\n适配器"),
            ("nul-multiline", "adapter\0\nrejected\0"),
            ("field-looking-value", "failure\nPRIORITY=0\nMESSAGE=decoy"),
            ("trailing-newline", "failure\n"),
            ("long-byte-length", long.as_str()),
        ] {
            let payload = native_payload(message);
            println!("native protocol observation: {case} payload={payload:02x?}");
            assert_eq!(
                exact_message(&payload, message),
                Ok(()),
                "native MESSAGE protocol assertion failed: {case}"
            );
        }
        assert_eq!(
            native_payload("one\ntwo"),
            b"PRIORITY=3\nSYSLOG_IDENTIFIER=flea\nMESSAGE\n\x07\0\0\0\0\0\0\0one\ntwo\n"
        );
        println!("native protocol control: all multiline values decoded as one complete MESSAGE");
    }
}
