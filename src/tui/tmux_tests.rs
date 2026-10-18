use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn adv_d_r1_tmux_own_sixel_does_not_require_passthrough() {
    let transport = Transport::Tmux { passthrough: false };
    let mut query = Vec::new();
    transport.query(&mut query, b"\x1b[c");
    assert_eq!(query, b"\x1b[c", "tmux must answer its own DA1");
    for packet in [b"\x1bPq#1~\x1b\\".as_slice(), b"\x1bP0;1;0q#1~\x1b\\"] {
        let mut output = Vec::new();
        write_graphics_with(transport, &mut output, packet).unwrap();
        assert_eq!(output, packet, "tmux must receive its own sixel packet");
    }
    let mut blocked = Vec::new();
    transport.query(&mut blocked, b"\x1b_Gi=1,a=q;AAAA\x1b\\");
    write_graphics_with(transport, &mut blocked, b"\x1bP+q524742\x1b\\\x1b_Ga=t;AAAA\x1b\\").unwrap();
    assert!(blocked.is_empty(), "outer-only protocols stay gated");
}

#[test]
fn adv_d_r1_unterminated_graphics_packets_are_reported() {
    for passthrough in [false, true] {
        for packet in [b"\x1b_Ga=t;AAAA\x1b\\".as_slice(), b"\x1bP0;1;0q#1~\x1b\\"] {
            for split in 2..packet.len() {
                let mut output = Vec::new();
                let result = write_graphics_with(Transport::Tmux { passthrough }, &mut output, &packet[..split]);
                assert!(result.is_err(), "incomplete packet silently accepted at {split}: {packet:?}");
                assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
                assert!(output.is_empty());
                let result = Transport::Tmux { passthrough }.write_packet(&mut output, &packet[..split]);
                assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
                assert!(output.is_empty());
            }
        }
    }
}

#[test]
fn tmux_passthrough_is_explicit_not_a_terminal_name_guess() {
    assert_eq!(parse("on\n"), Transport::Tmux { passthrough: true });
    assert_eq!(parse("all\n"), Transport::Tmux { passthrough: true });
    for text in ["", "off\n256\n", "kitty\nnotRGB\n", "onward\nRGBish\n"] {
        assert_eq!(parse(text), Transport::Tmux { passthrough: false });
    }
}
#[test]
fn graphics_packets_are_wrapped_and_text_cursor_controls_are_preserved() {
    let bytes = b"\x1b[s\x1b[4;8H\x1b_Ga=t;AAAA\x1b\\\x1b_Ga=p\x1b\\\x1b[u";
    let mut direct = Vec::new();
    write_graphics_with(Transport::Direct, &mut direct, bytes).unwrap();
    assert_eq!(direct, bytes);
    let mut partial = Vec::new();
    write_graphics_with(Transport::Direct, &mut partial, b"\x1b_Gpartial").unwrap();
    assert_eq!(partial, b"\x1b_Gpartial");
    let mut wrapped = Vec::new();
    write_graphics_with(Transport::Tmux { passthrough: true }, &mut wrapped, bytes).unwrap();
    assert_eq!(wrapped, b"\x1b[s\x1b[4;8H\x1bPtmux;\x1b\x1b_Ga=t;AAAA\x1b\x1b\\\x1b\\\x1bPtmux;\x1b\x1b_Ga=p\x1b\x1b\\\x1b\\\x1b[u");
    let mut blocked = Vec::new();
    write_graphics_with(Transport::Tmux { passthrough: false }, &mut blocked, bytes).unwrap();
    assert_eq!(blocked, b"\x1b[s\x1b[4;8H\x1b[u");
    let mut sixel = Vec::new();
    Transport::Tmux { passthrough: true }.write_packet(&mut sixel, b"\x1bPq#1~\x1b\\").unwrap();
    assert_eq!(sixel, b"\x1bPtmux;\x1b\x1bPq#1~\x1b\x1b\\\x1b\\");
}
#[test]
fn tmux_discovery_handles_success_refusal_timeout_and_bounded_output() {
    let dir = TestDir::new("tmux-discovery");
    for (name, body, expected) in [
        ("positive", "#!/bin/sh\nprintf 'on\\n'\n", Transport::Tmux { passthrough: true }),
        ("refused", "#!/bin/sh\nprintf 'on\\nRGB\\n'; exit 1\n", Transport::Tmux { passthrough: false }),
        ("timeout", "#!/bin/sh\nsleep 30\n", Transport::Tmux { passthrough: false }),
        ("oversized", "#!/bin/sh\nhead -c 8192 /dev/zero\n", Transport::Tmux { passthrough: false }),
    ] {
        let script = dir.script(name, body);
        let started = Instant::now();
        assert_eq!(discover_with(&mut Command::new(script), Duration::from_millis(100)), expected);
        let elapsed = started.elapsed();
        assert!(elapsed < Duration::from_secs(2), "discovery hung");
        println!("tmux-discovery {name}: {} ms", elapsed.as_millis());
    }
}
