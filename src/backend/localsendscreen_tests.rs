use super::*;
use crate::backend::localsendtext::{parse_peers, strip_ansi};

// Captured from localsend-cli 1.18.2: the lit row drawn whole, the other two as changed cells, blanks stepped over.
const PANEL: &str = "\u{1b}[?1049h\u{1b}[?25l\u{1b}[5;1H\u{2502}\u{1b}[1m\u{1b}[38;5;2;49mDiscovered\u{1b}[22m\u{1b}[39;49m\
\u{1b}[6;2H\u{1b}[38;5;15;48;5;8m  [1] Peer D (10.77.0.4)                    \
\u{1b}[7;4H\u{1b}[39;49m[2]\u{1b}[7;8HPeer\u{1b}[7;13HC\u{1b}[7;15H(10.77.0.3)\
\u{1b}[8;4H[3]\u{1b}[8;8HPeer\u{1b}[8;13HB\u{1b}[8;15H(10.77.0.2)\u{1b}[39m\u{1b}[49m\u{1b}[59m\u{1b}[0m\u{1b}[?25l";

fn screen_of(chunks: &[&[u8]]) -> Screen {
    let mut screen = Screen::new(40, 120);
    for chunk in chunks {
        screen.feed(chunk);
    }
    screen
}

#[test]
fn a_row_drawn_as_changed_cells_reads_back_as_the_device_it_names() {
    let screen = screen_of(&[PANEL.as_bytes()]);
    let peers = parse_peers(&screen.visible());
    let named: Vec<(usize, &str, &str)> = peers.iter().map(|p| (p.index, p.name.as_str(), p.address.as_str())).collect();
    assert_eq!(named, vec![(1, "Peer D", "10.77.0.4"), (2, "Peer C", "10.77.0.3"), (3, "Peer B", "10.77.0.2")]);
    // The transcript the 0.4.0 bridge read, escapes stripped, has only the chosen row in a readable form.
    assert_eq!(parse_peers(&strip_ansi(PANEL)).len(), 1);
}

#[test]
fn the_chosen_row_is_the_lit_one_and_follows_a_key() {
    let mut screen = screen_of(&[PANEL.as_bytes()]);
    assert!(screen.chosen().is_some_and(|row| row.contains("[1] Peer D")));
    // Down: the old row redrawn plain, the next drawn on the background.
    screen.feed("\u{1b}[6;2H\u{1b}[39;49m  [1] Peer D (10.77.0.4)\u{1b}[7;2H\u{1b}[38;5;15;48;5;8m  [2] Peer C (10.77.0.3)    \u{1b}[0m".as_bytes());
    assert!(screen.chosen().is_some_and(|row| row.contains("[2] Peer C")));
    assert_eq!(Screen::new(4, 20).chosen(), None);
}

#[test]
fn a_character_split_between_two_reads_arrives_whole() {
    let row = "\u{1b}[?1049h\u{1b}[3;3H[1] P\u{e4}st\u{eb} B\u{fc}ro \u{6771}\u{4eac} (10.0.0.9)".as_bytes();
    // Cut inside the two-byte a-umlaut and inside the three-byte first ideograph.
    let cut_a = row.iter().position(|b| *b == 0xc3).unwrap() + 1;
    let cut_b = row.iter().position(|b| *b == 0xe6).unwrap() + 2;
    let screen = screen_of(&[&row[..cut_a], &row[cut_a..cut_b], &row[cut_b..]]);
    let peers = parse_peers(&screen.visible());
    assert_eq!(peers[0].name, "P\u{e4}st\u{eb} B\u{fc}ro \u{6771}\u{4eac}");
    assert_eq!(peers[0].address, "10.0.0.9");
}

#[test]
fn a_wide_name_keeps_the_cells_after_it_where_the_cli_put_them() {
    // Each ideograph takes two cells, so the address the CLI placed by column lands after the name.
    let screen = screen_of(&["\u{1b}[2;1H[1] \u{6771}\u{4eac}\u{1b}[2;10H(10.0.0.8)".as_bytes()]);
    let peers = parse_peers(&screen.visible());
    assert_eq!((peers[0].name.as_str(), peers[0].address.as_str()), ("\u{6771}\u{4eac}", "10.0.0.8"));
    // Emoji are wide inside and outside the emoji blocks: a rocket, a green circle and a high voltage sign.
    for wide in ["\u{1f680}", "\u{1f7e2}", "\u{26a1}"] {
        let screen = screen_of(&[format!("\u{1b}[2;1H[1] Peer {}\u{1b}[2;13HX\u{1b}[2;15H(10.0.0.7)", wide).as_bytes()]);
        assert_eq!(parse_peers(&screen.visible())[0].name, format!("Peer {} X", wide));
    }
    // The emoji blocks also hold narrow symbols: the hammer and wrench and the desktop computer take one cell.
    for narrow in ["\u{1f6e0}", "\u{1f5a5}"] {
        let screen = screen_of(&[format!("\u{1b}[2;1H[1] Box {}\u{1b}[2;11HX\u{1b}[2;13H(10.0.0.6)", narrow).as_bytes()]);
        assert_eq!(parse_peers(&screen.visible())[0].name, format!("Box {} X", narrow));
    }
}

// A cursor move the size of usize is clamped at the screen's edge rather than overflowing the reader.
#[test]
fn a_huge_cursor_move_stops_at_the_edge() {
    let screen = screen_of(&[b"\x1b[2;3H\x1b[18446744073709551615C\x1b[18446744073709551615BX"]);
    assert!(screen.visible().lines().last().is_some_and(|row| row.len() == 120 && row.ends_with('X')), "{}", screen.visible());
}

// Captured: the progress line redraws by ESC[1G ESC[2K with no carriage return, so a transcript joins it to the Sent line.
const FINISH: &[u8] = b"\x1b[38;5;13mS\x1b[39m Peer B [####################-] 30.9 MB / 31.5 MB [12.7 MB/s] [ETA: 0s]\
\x1b[1G\x1b[2K\x1b[38;5;13mS\x1b[39m Peer B [#####################] 31.5 MB / 31.5 MB [9.5 MB/s] [ETA: --]\
\x1b[1G\x1b[2K\x1b[38;5;13mS\x1b[39m Peer B: Sent 1 file (31.5 MB, took 2s)\r\n\x1b[38;5;13mS\x1b[39m Peer B [####";

#[test]
fn a_status_line_redrawn_in_place_leaves_its_last_words_on_a_line_of_their_own() {
    let screen = screen_of(&[FINISH]);
    assert!(screen.log().lines().any(|l| l == "S Peer B: Sent 1 file (31.5 MB, took 2s)"), "{}", screen.log());
    assert!(screen.log().lines().any(|l| l.starts_with("S Peer B [#####################] 31.5 MB / 31.5 MB")));
    // The 0.4.0 transcript: the Sent words never start a line of their own.
    let joined = strip_ansi(std::str::from_utf8(FINISH).unwrap());
    assert!(!joined.lines().any(|l| l.starts_with("S Peer B: Sent")));
}

// An hour of progress redraws keeps the log bounded, cut at a line, and the newest line whole.
#[test]
fn a_long_transfer_keeps_only_the_recent_log() {
    let mut screen = Screen::new(40, 120);
    for i in 0..20_000 {
        screen.feed(format!("\x1b[1G\x1b[2KS P\u{e4}er [###] {i} MB / 20000 MB [1 MB/s] [ETA: 1s]").as_bytes());
    }
    screen.feed(b"\x1b[1G\x1b[2KS P\xc3\xa4er: Sent 1 file (20000 MB, took 1h)\r\n");
    assert!(screen.log().len() <= LOG_CAP, "{}", screen.log().len());
    assert!(screen.log().starts_with("S P\u{e4}er [###] "));
    assert!(screen.log().lines().any(|l| l == "S P\u{e4}er: Sent 1 file (20000 MB, took 1h)"));
}

#[test]
fn closing_the_panel_shows_the_screen_under_it_and_erases_clear_cells() {
    let mut screen = screen_of(&[b"D [1] Peer B (10.77.0.2)\r\n", PANEL.as_bytes()]);
    assert_eq!(parse_peers(&screen.visible()).len(), 3);
    screen.feed(b"\x1b[?1049l");
    let peers = parse_peers(&screen.visible());
    assert_eq!(peers.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Peer B"]);
    screen.feed(b"\x1b[1;1H\x1b[2K");
    assert!(parse_peers(&screen.visible()).is_empty());
}
