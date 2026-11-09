use crate::tui::{keymap::Map, model::{Deletion, Tab}, render, render::frame_tests::model, theme::{Depth, Theme}};
use std::time::Duration;

const PALETTE: &str = "background='#1c120d'\nforeground='#c6b395'\naccent='#d28a41'\nmuted='#68544b'";

// One cell's or one erase's background as its SGR parameters, "48;2;28;18;13", or None for the terminal default.
type Background = Option<String>;
struct Replay { cells: Vec<(usize, usize, char, Background)>, erases: Vec<(String, Background)>, last: Background }

// Sample input: "\x1b[48;5;234m" yields "48;5;234".
fn parameters(sgr: &str) -> String { sgr.trim_start_matches("\x1b[").trim_end_matches('m').to_owned() }
// Sample input: "0;38;2;1;2;3;48;5;17" leaves the background at "48;5;17".
fn apply(arguments: &str, background: &mut Background) {
    let codes: Vec<&str> = arguments.split(';').collect();
    let mut index = 0;
    while index < codes.len() {
        let code = codes[index].parse::<u32>().unwrap_or(0);
        let end = (index + match code { 38 | 48 if codes.get(index + 1) == Some(&"5") => 3, 38 | 48 => 5, _ => 1 }).min(codes.len());
        match code { 0 | 49 => *background = None, 40..=48 | 100..=107 => *background = Some(codes[index..end].join(";")), _ => {} }
        index = end;
    }
}
// Replays cursor moves, SGR and erases the way a terminal applies them, from the terminal's reset state.
// Sample input: "\x1b[2;3H\x1b[48;5;17mA\x1b[K" writes "A" at x 2, y 1 on background "48;5;17", then erases to the line end.
fn replay(stream: &str) -> Replay {
    let mut out = Replay { cells: Vec::new(), erases: Vec::new(), last: None };
    let (mut x, mut y) = (0, 0);
    let mut chars = stream.chars();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.cells.push((x, y, c, out.last.clone()));
            x += crate::tui::cells::width(c);
            continue;
        }
        assert_eq!(chars.next(), Some('['), "the frame writes only CSI sequences");
        let mut arguments = String::new();
        let mut last = ' ';
        for code in chars.by_ref() {
            if ('@'..='~').contains(&code) { last = code; break; }
            arguments.push(code);
        }
        match last {
            'H' => {
                let mut numbers = arguments.split(';').map(|s| s.parse::<usize>().unwrap_or(1).saturating_sub(1));
                y = numbers.next().unwrap_or(0);
                x = numbers.next().unwrap_or(0);
            }
            'J' | 'K' | 'X' | '@' | 'L' | 'M' | 'P' => out.erases.push((format!("{arguments}{last}"), out.last.clone())),
            'm' => apply(&arguments, &mut out.last),
            _ => {}
        }
    }
    out
}
// The stream render::draw writes for these frames in order, each followed by its reset.
fn drawn(frames: &[String]) -> String { frames.iter().map(|frame| format!("{frame}\x1b[0m")).collect() }
// The plates a frame may paint behind its cells; a scrimmed frame's plates are the scrim's own, mixed halfway to the ground.
fn washes(theme: &Theme, scrimmed: bool) -> Vec<String> {
    let lit = [&theme.selected, &theme.cursor, &theme.marked, &theme.tab, &theme.match_fill, &theme.wash].map(|wash| parameters(wash));
    let scrim = if scrimmed { theme.dim.as_deref().map(|dim| [&dim.selected, &dim.cursor, &dim.marked, &dim.tab, &dim.match_fill, &dim.wash].map(|wash| parameters(wash))) } else { None };
    lit.into_iter().chain(scrim.into_iter().flatten()).collect()
}
// Fails on the first cell outside a wash that carries an explicit background and on any erase under one; returns the washed cells.
fn assert_ground(case: &str, theme: &Theme, stream: &str, scrimmed: bool) -> Vec<String> {
    let washes = washes(theme, scrimmed);
    let replay = replay(stream);
    assert!(replay.cells.len() > 40, "{case}: the frame drew no cells");
    for (x, y, c, background) in &replay.cells {
        if let Some(background) = background {
            assert!(washes.contains(background), "{case}: cell ({x},{y}) {c:?} carries explicit background {background} outside a wash");
        }
    }
    for (erase, background) in &replay.erases {
        assert!(background.is_none(), "{case}: erase CSI {erase} runs with explicit background {background:?}");
    }
    replay.cells.into_iter().filter_map(|(_, _, _, background)| background).collect()
}
fn frame(m: &crate::tui::model::Model, theme: &Theme, columns: usize, lines: usize) -> String {
    render::frame(m, theme, &Map::load(), columns, lines, Duration::ZERO)
}

#[test]
fn tui_ground_is_the_terminal_default_background_in_every_frame_and_erase() {
    for depth in [Depth::Truecolor, Depth::Indexed, Depth::Sixteen] {
        let theme = Theme::with_depth(PALETTE, depth);
        let case = |name: &str| format!("{name} at {depth:?}");
        let mut m = model();
        m.selected.insert(2);
        m.tabs.push(Tab { path: "/fx/v/Docs".into(), cursor: 0, back: Vec::new(), forward: Vec::new() });
        m.tab = 1;
        let washed = assert_ground(&case("listing with marked row, cursor and tabs"), &theme, &drawn(&[frame(&m, &theme, 130, 31)]), false);
        if depth == Depth::Truecolor {
            for wash in [&theme.cursor, &theme.marked, &theme.tab] {
                assert!(washed.contains(&parameters(wash)), "the listing lost its {wash:?} plate");
            }
        }
        m.preview_focus = true;
        let washed = assert_ground(&case("unfocused cursor"), &theme, &drawn(&[frame(&m, &theme, 130, 31)]), false);
        if depth == Depth::Truecolor { assert!(washed.contains(&parameters(&theme.selected)), "the unfocused cursor lost its plate"); }
        let mut m = model();
        m.filter = "o".into();
        let washed = assert_ground(&case("filtered listing"), &theme, &drawn(&[frame(&m, &theme, 130, 31)]), false);
        if depth == Depth::Truecolor { assert!(!washed.contains(&parameters(&theme.match_fill)), "filter matches must keep each row's own background"); }
        let mut m = model();
        m.total = 0; m.rows.clear(); m.path.push("Empty");
        assert_ground(&case("empty folder"), &theme, &drawn(&[frame(&m, &theme, 130, 31)]), false);
        for overlay in ["menu", "sheet", "delete"] {
            let mut m = model();
            match overlay {
                "menu" => m.menu = true,
                "sheet" => m.sheet = true,
                _ => m.deletion = Some(Deletion { token: 1, count: 1, ..Default::default() }),
            }
            assert_ground(&case(overlay), &theme, &drawn(&[frame(&m, &theme, 130, 31)]), overlay != "menu");
        }
        let m = model();
        let sizes = [(130, 31), (11, 4), (40, 12), (130, 4), (130, 31)];
        let frames: Vec<String> = sizes.iter().map(|&(columns, lines)| frame(&m, &theme, columns, lines)).collect();
        assert!(frames.iter().any(|frame| frame.contains("\x1b[2J")), "the resize run never reached the erasing small frame");
        assert_ground(&case("resize redraw"), &theme, &drawn(&frames), false);
        // A frame also leaves the default background behind it, so whatever the terminal erases next takes the ground.
        for (frame, (columns, lines)) in frames.iter().zip(sizes) {
            assert_eq!(replay(frame).last, None, "a {columns}x{lines} frame at {depth:?} ends on an explicit background");
        }
    }
}

// NO_COLOR: every frame and overlay draws without one colour code, and the cursor row still reads, as reverse video.
#[test]
fn no_color_frames_carry_no_colour_and_the_cursor_reads_as_reverse_video() {
    assert!(crate::tui::theme::no_color(Some("1".into())) && !crate::tui::theme::no_color(Some("".into())) && !crate::tui::theme::no_color(None));
    let theme = Theme::with_depth(PALETTE, Depth::Mono);
    let mut frames = Vec::new();
    let mut m = model();
    m.selected.insert(2);
    m.tabs.push(Tab { path: "/fx/v/Docs".into(), cursor: 0, back: Vec::new(), forward: Vec::new() });
    frames.push(("listing", frame(&m, &theme, 130, 31)));
    m.preview_focus = true;
    frames.push(("unfocused cursor", frame(&m, &theme, 130, 31)));
    for overlay in ["menu", "sheet", "delete"] {
        let mut m = model();
        match overlay {
            "menu" => m.menu = true,
            "sheet" => m.sheet = true,
            _ => m.deletion = Some(Deletion { token: 1, count: 1, ..Default::default() }),
        }
        frames.push((overlay, frame(&m, &theme, 130, 31)));
    }
    for (case, stream) in &frames {
        let mut codes = Vec::new();
        let mut rest = stream.as_str();
        while let Some(start) = rest.find("\x1b[") {
            rest = &rest[start + 2..];
            let end = rest.find(|c: char| ('@'..='~').contains(&c)).unwrap();
            if rest[end..].starts_with('m') { codes.extend(rest[..end].split(';').filter_map(|code| code.parse::<u32>().ok())); }
            rest = &rest[end..];
        }
        let colour = codes.iter().find(|code| matches!(code, 30..=38 | 40..=48 | 90..=97 | 100..=107));
        assert!(colour.is_none(), "{case}: NO_COLOR frame carries colour code {colour:?}");
        if *case == "listing" || *case == "unfocused cursor" {
            assert!(codes.contains(&7), "{case}: the cursor row lost its reverse video");
            assert!(codes.contains(&4), "{case}: the marked row lost its underline");
        }
    }
}
