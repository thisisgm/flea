use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn tmux_features_are_explicit_not_terminal_name_guesses() {
    assert_eq!(parse("on\nRGB,clipboard,extkeys\n"), Transport::Tmux { passthrough: true, rgb: true });
    assert_eq!(parse("all\n256,RGB\n"), Transport::Tmux { passthrough: true, rgb: true });
    for text in ["", "off\n256\n", "kitty\nnotRGB\n", "onward\nRGBish\n"] {
        assert_eq!(parse(text), Transport::Tmux { passthrough: false, rgb: false });
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
    write_graphics_with(Transport::Tmux { passthrough: true, rgb: false }, &mut wrapped, bytes).unwrap();
    assert_eq!(wrapped, b"\x1b[s\x1b[4;8H\x1bPtmux;\x1b\x1b_Ga=t;AAAA\x1b\x1b\\\x1b\\\x1bPtmux;\x1b\x1b_Ga=p\x1b\x1b\\\x1b\\\x1b[u");
    let mut blocked = Vec::new();
    write_graphics_with(Transport::Tmux { passthrough: false, rgb: false }, &mut blocked, bytes).unwrap();
    assert_eq!(blocked, b"\x1b[s\x1b[4;8H\x1b[u");
    let mut sixel = Vec::new();
    Transport::Tmux { passthrough: true, rgb: false }.write_packet(&mut sixel, b"\x1bPq#1~\x1b\\").unwrap();
    assert_eq!(sixel, b"\x1bPtmux;\x1b\x1bPq#1~\x1b\x1b\\\x1b\\");
}
#[test]
fn tmux_discovery_handles_success_refusal_timeout_and_bounded_output() {
    let dir = TestDir::new("tmux-discovery");
    for (name, body, expected) in [
        ("positive", "#!/bin/sh\nprintf 'on\\nRGB,extkeys\\n'\n", Transport::Tmux { passthrough: true, rgb: true }),
        ("refused", "#!/bin/sh\nprintf 'on\\nRGB\\n'; exit 1\n", Transport::Tmux { passthrough: false, rgb: false }),
        ("timeout", "#!/bin/sh\nsleep 30\n", Transport::Tmux { passthrough: false, rgb: false }),
        ("oversized", "#!/bin/sh\nhead -c 8192 /dev/zero\n", Transport::Tmux { passthrough: false, rgb: false }),
    ] {
        let script = dir.script(name, body);
        let started = Instant::now();
        assert_eq!(discover_with(&mut Command::new(script), Duration::from_millis(100)), expected);
        let elapsed = started.elapsed();
        assert!(elapsed < Duration::from_secs(2), "discovery hung");
        println!("tmux-discovery {name}: {} ms", elapsed.as_millis());
    }
}
