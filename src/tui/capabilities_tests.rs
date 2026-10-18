use super::*;
use crate::tui::input::{Decoder, Key};

fn decoder(transport: Transport, now: Instant) -> Decoder {
    Decoder::new(Negotiation::begin(transport, (31, 32), true, now))
}
#[test]
fn replies_are_correlated_one_shot_and_bounded_by_the_query_window() {
    let now = Instant::now();
    let mut d = decoder(Transport::Direct, now);
    let keys = d.feed_at(b"j\x1b_Gi=30;OK\x1b\\\x1b[?62;4;22ck", false, now);
    assert_eq!(keys, [Key::character('j', ""), Key::character('k', "")]);
    assert_eq!(d.negotiation.detected.graphics, Protocol::Sixel);
    d.feed_at(b"\x1b_Gi=31;ENOENT\x1b\\\x1b_Gi=31;OK\x1b\\", false, now);
    assert_eq!(d.negotiation.detected.graphics, Protocol::Sixel);
    d.feed_at(b"\x1b_Gi=32;OK\x1b\\", false, now + QUERY_TIME);
    assert!(!d.negotiation.detected.shared_memory);
    assert_eq!(d.negotiation.detected.keyboard, Keyboard::Legacy);
    assert!(d.feed_at(b"\x1b[?5u\x1b[?62;4c\x1b_Gi=31;OK\x1b\\q", false, now + QUERY_TIME).ends_with(&[Key::character('q', "")]));
    assert!(!d.negotiation.take_keyboard_enable());
    let mut unasked = Decoder::default();
    assert!(unasked.feed(b"\x1b[?5u\x1b[?62;4c\x1b_Gi=31;OK\x1b\\", false).is_empty());
    assert_eq!(unasked.negotiation.detected, Capabilities::default());
}
#[test]
fn every_fragment_boundary_preserves_keys_and_positive_capabilities() {
    let now = Instant::now();
    let input = b"j\x1b_Gi=32;OK\x1b\\\x1b_Gi=31;OK\x1b\\\x1b[?0u\x1b[?62;4c\x1bP1+r524742\x1b\\k";
    for split in 0..=input.len() {
        let mut d = decoder(Transport::Direct, now);
        let mut keys = d.feed_at(&input[..split], false, now);
        keys.extend(d.feed_at(&input[split..], false, now));
        assert_eq!(keys, [Key::character('j', ""), Key::character('k', "")], "split {split}");
        assert_eq!(d.negotiation.detected.graphics, Protocol::Kitty);
        assert!(d.negotiation.detected.shared_memory);
        assert_eq!(d.negotiation.detected.color, Color::Rgb);
        assert_eq!(d.negotiation.detected.keyboard, Keyboard::Kitty { previous_flags: 0 });
        assert!(d.negotiation.take_keyboard_enable());
        d.feed_at(b"\x1b[?1u", false, now);
        assert!(!d.negotiation.take_keyboard_enable());
    }
    let mut d = decoder(Transport::Direct, now);
    let shared = format!("\x1b_Gi=32,s=1,v=1,a=q,t=s,f=24;{}\x1b\\", crate::tui::graphics::base64(b"/flea-unit-pixel"));
    let query = d.negotiation.queries(&shared);
    for byte in input { d.feed_at(&[*byte], false, now); }
    assert_eq!(d.negotiation.detected.graphics, Protocol::Kitty);
    println!("positive query={:?} response={:?} selected={:?}", query, input, d.negotiation.detected);
}
#[test]
fn malformed_negative_and_stale_replies_never_enable_a_feature() {
    let now = Instant::now();
    let mut d = decoder(Transport::Direct, now);
    for bytes in [b"\x1b[?1;4c".as_slice(), b"\x1b[?62;4x", b"\x1b[?62;4;badc", b"\x1b[?xu", b"\x1bP0+r524742\x1b\\", b"\x1b_Gi=31;NOT OK\x1b\\"] {
        d.feed_at(bytes, false, now);
    }
    let oversized = format!("\x1b[?62;4;{}0c", "0;".repeat(100));
    d.feed_at(oversized.as_bytes(), false, now);
    assert_eq!(d.negotiation.detected.graphics, Protocol::None);
    assert_eq!(d.negotiation.detected.color, Color::Indexed);
    assert_eq!(d.negotiation.detected.keyboard, Keyboard::Unknown);
    let mut fresh = Decoder::new(Negotiation::begin(Transport::Direct, (41, 42), true, now));
    fresh.feed_at(b"\x1b_Gi=31;OK\x1b\\\x1b_Gi=32;OK\x1b\\", false, now);
    assert_eq!(fresh.negotiation.detected.graphics, Protocol::None);
    assert!(!fresh.negotiation.detected.shared_memory);
    let mut d = decoder(Transport::Direct, now);
    d.feed_at(b"\x1b_Gi=31;O", false, now);
    d.feed_at(b"K\x1b\\", false, now + QUERY_TIME);
    assert_eq!(d.negotiation.detected.graphics, Protocol::None);
}
#[test]
fn missing_tmux_passthrough_keeps_graphics_off_even_with_positive_outer_replies() {
    let now = Instant::now();
    for rgb in [false, true] {
        let mut d = decoder(Transport::Tmux { passthrough: false, rgb }, now);
        let queries = d.negotiation.queries("shared");
        assert_eq!(queries, b"\x1b[?u");
        d.feed_at(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;4c\x1b_Gi=32;OK\x1b\\\x1bP1+r5463\x1b\\\x1b[?0u", false, now);
        assert_eq!(d.negotiation.detected.graphics, Protocol::None);
        assert!(!d.negotiation.detected.shared_memory);
        assert_eq!(d.negotiation.detected.color, if rgb { Color::Rgb } else { Color::Indexed });
        assert_eq!(d.negotiation.detected.keyboard, Keyboard::Kitty { previous_flags: 0 });
        println!("tmux-off query={queries:?} selected={:?}", d.negotiation.detected);
    }
}
#[test]
fn tmux_queries_and_fragmented_wrapped_replies_are_decoded() {
    let now = Instant::now();
    let transport = Transport::Tmux { passthrough: true, rgb: true };
    let mut d = decoder(transport, now);
    let query = d.negotiation.queries("shared");
    assert!(query.starts_with(b"\x1bPtmux;\x1b\x1b_Gi=31"));
    assert!(query.windows(4).any(|p| p == b"\x1b[?u"));
    assert!(query.ends_with(b"\x1bPtmux;\x1b\x1b[c\x1b\\"));
    let mut response = Vec::new();
    transport.write_packet(&mut response, b"\x1b_Gi=31;OK\x1b\\").unwrap();
    for byte in &response { assert!(d.feed_at(&[*byte], false, now).is_empty()); }
    assert_eq!(d.negotiation.detected.graphics, Protocol::Kitty);
    assert!(!d.negotiation.detected.shared_memory);
    println!("tmux-on query={query:?} response={response:?} selected={:?}", d.negotiation.detected);
}
#[test]
fn deadlines_and_partial_controls_release_the_event_loop() {
    let now = Instant::now();
    let mut d = decoder(Transport::Direct, now);
    assert_eq!(d.negotiation.wait_ms(now, 1000), 500);
    assert_eq!(d.negotiation.wait_ms(now + Duration::from_micros(499_400), 1000), 1);
    d.feed_at(b"\x1b[?", false, now);
    assert!(d.feed_at(b"", true, now + QUERY_TIME).is_empty());
    assert_eq!(d.feed_at(b"\x1b[Aj", false, now + QUERY_TIME), [Key::named("Up", ""), Key::character('j', "")]);
    d.feed_at(b"\x1b_Gi=31;", false, now);
    d.feed_at(b"", true, now + QUERY_TIME);
    assert_eq!(d.feed_at(b"OK\x1b\\q", false, now + QUERY_TIME), [Key::character('q', "")]);
    assert_eq!(d.negotiation.wait_ms(now + QUERY_TIME, 1000), 1000);
}
