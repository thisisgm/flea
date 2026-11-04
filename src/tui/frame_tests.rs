use crate::tui::{model::{Model, Row}, render, keymap::Map, theme::Theme};
use crate::jsondoc::Json;
use std::time::Duration;

// Sample input: "\x1b[31mnote\x1b[0m".
pub fn plain(text: &str) -> String {
    let mut chars = text.chars();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() == Some('[') { for c in chars.by_ref() { if ('@'..='~').contains(&c) { break; } } }
        } else if !matches!(c, '\u{2066}'..='\u{2069}') { out.push(c); }
    }
    out
}
// Sample input: "\x1b[2;3Hnote\x1b[0m" at row 2, column 3.
pub fn grid(text: &str, columns: usize, lines: usize) -> Vec<String> {
    let mut cells = vec![vec![String::from(" "); columns]; lines];
    let (mut x, mut y) = (0, 0);
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.next() == Some('[') {
            let mut args = String::new();
            for code in chars.by_ref() {
                if ('@'..='~').contains(&code) {
                    if code == 'H' {
                        let mut numbers = args.split(';').map(|s| s.parse::<usize>().unwrap_or(1).saturating_sub(1));
                        y = numbers.next().unwrap_or(0); x = numbers.next().unwrap_or(0);
                    } else if code == 'J' && args == "2" { cells.iter_mut().for_each(|row| row.fill(" ".into())); }
                    break;
                }
                args.push(code);
            }
        } else if y < lines && x < columns {
            if matches!(c, '\u{2066}'..='\u{2069}') { continue; }
            let width = crate::tui::cells::width(c);
            if width == 0 { if x > 0 { cells[y][x - 1].push(c); } }
            else {
                cells[y][x] = c.to_string();
                for cell in cells[y].iter_mut().skip(x + 1).take(width - 1) { cell.clear(); }
                x += width;
            }
        }
    }
    cells.into_iter().map(|row| row.concat()).collect()
}
pub fn entry(name: &str, directory: bool, size: usize) -> Row {
    Row::parse(&crate::jsondoc::parse(&format!(r#"{{"n":"{name}","d":{directory},"s":{size},"p":33188,"i":"text-x-generic"}}"#)).unwrap(), &[])
}
pub fn model() -> Model {
    let mut m = Model::new("/fx/v/work".into(), &Json::Null);
    m.i18n = crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/i18n/fixtures/omarchy"),
        language_file: "/nonexistent/flea-frame-language".into(),
        catalogs: Vec::new(),
    });
    m.height = 27; m.columns = 130;
    m.parents = vec![entry("work", true, 0)];
    for (index, (name, directory, size)) in [("A", true, 462), ("Code", true, 216), ("Docs", true, 22100),
        ("Empty", true, 40), ("Locked", true, 60), ("Many", true, 2200), ("Media", true, 2600000),
        ("notes.txt", false, 99), ("todo.md", false, 48)].into_iter().enumerate() {
        m.rows.insert(index, entry(name, directory, size));
        if directory { m.folder_sizes.insert(index, (size, index == 4)); }
    }
    m.total = 9; m.cursor = 7;
    m.preview_path = m.path.join("notes.txt");
    m.preview_body = "Visual gate notes\n\nThis file is the fixture's plain text sample.\nLine four keeps the preview busy.".into();
    m.rows.get_mut(&7).unwrap().kind = "Plain text document".into();
    m.disk_path = m.path.clone(); m.disk_fs = "tmpfs".into(); m.disk_free = 64100000;
    m.hints = crate::tui::keymap::Hints::new(&Map::load(), "default");
    crate::tui::layout::prepare(&mut m);
    m
}
// Turkish key group and Greek print words from a read-only fixture, so heading tests do not depend on which catalogs ship.
pub fn caps_language(code: &str) -> crate::i18n::I18n {
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/tui/fixtures/caps");
    crate::i18n::I18n::load(crate::i18n::Paths {
        omarchy: fixture.clone(),
        language_file: fixture.join(format!("language-{code}")),
        catalogs: vec![fixture.join("catalog")],
    })
}
#[test]
fn t3_frame_matches_reference_columns_names_sizes_and_rules() {
    let m = model();
    let theme = Theme::from_text("");
    let out = render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO);
    let rows = grid(&out, 130, 31);
    assert_eq!(&rows[0][..16], " /fx/v/work     ");
    assert_eq!(rows[0].chars().skip(123).collect::<String>(), "Name ▴ ");
    assert_eq!(rows[1].chars().nth(27), Some('┬'));
    assert_eq!(rows[1].chars().nth(79), Some('┬'));
    assert_eq!(rows[29].chars().nth(27), Some('┴'));
    assert_eq!(rows[2].chars().nth(27), Some('│'));
    let current: String = rows[9].chars().skip(28).take(51).collect();
    assert_eq!(current, format!("▎{} notes.txt{}99 B  ", crate::tui::icons::TEXT, " ".repeat(33)));
    assert_eq!(rows[9].chars().nth(79), Some('│'));
    assert!(rows[2].contains(&format!("{} notes.txt", crate::tui::icons::TEXT)));
    assert!(rows[5].contains("1 Visual gate notes"));
    assert!(rows[30].starts_with(" 9 items"));
    assert!(rows[30].ends_with("tmpfs · 64.1 MB free · ? keys "));
    assert!(!rows.join("").contains("notes.txt/"));
    assert!(out.contains(&theme.cursor));
}
#[test]
fn t3_no_preview_and_no_marks_keep_name_columns() {
    let mut m = model();
    m.preview_visible = false;
    let theme = Theme::from_text("");
    for marks in [true, false] {
        m.force_text_marks = !marks;
        let rows = grid(&render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO), 130, 31);
        assert_eq!(rows[9].chars().skip(31).take(9).collect::<String>(), "notes.txt");
        assert_eq!(rows[2].chars().filter(|c| *c == '│').count(), 1);
        assert_eq!(rows[9].chars().skip(124).take(4).collect::<String>(), "99 B");
    }
}
#[test]
fn t3_every_screen_axis_and_resize_produce_bounded_complete_frames() {
    let m = model(); let theme = Theme::from_text(""); let map = Map::load();
    for columns in [0, 1, 11, 12, 28, 40, 130] {
        for lines in [0, 1, 4, 5, 6, 31] {
            let rows = grid(&render::frame(&m, &theme, &map, columns, lines, Duration::ZERO), columns, lines);
            assert_eq!(rows.len(), lines);
            assert!(rows.iter().all(|row| crate::tui::cells::text_width(row) == columns));
        }
    }
}

#[test]
fn t3_empty_and_locked_states_match_reference_cells_without_preview_side_effects() {
    let mut m = model(); let theme = Theme::from_text(""); let map = Map::load();
    m.total = 0; m.rows.clear(); m.path.push("Empty");
    m.empty_at = std::time::Instant::now() - Duration::from_secs(6);
    let rows = grid(&render::frame(&m, &theme, &map, 130, 31, Duration::ZERO), 130, 31);
    assert_eq!(rows[14].chars().skip(28).take(51).collect::<String>().trim(), crate::tui::icons::FOLDER);
    assert_eq!(rows[15].chars().skip(28).take(51).collect::<String>().trim(), "QUIET IN HERE");
    assert!(rows[30].starts_with(" 0 items"));
    let (mut w, reader) = crate::tui::wire::echo_wire();
    let mut m = model(); m.cursor = 4; m.preview_path = m.path.join("Locked");
    m.preview_children = Some(m.preview_path.clone());
    m.receive(crate::jsondoc::parse(r#"{"t":"peeked","path":"/fx/v/work/Locked","failed":true,"mode":16384,"rows":[]}"#).unwrap(), &mut w).unwrap();
    assert!(m.error.is_empty());
    crate::tui::preview::layout(&mut m);
    let rows = grid(&render::frame(&m, &theme, &map, 130, 31, Duration::ZERO), 130, 31);
    assert_eq!(rows[16].chars().skip(80).collect::<String>().trim(), crate::tui::icons::LOCK);
    assert_eq!(rows[17].chars().skip(80).collect::<String>().trim(), "You don’t have permission to open this folder.");
    m.pending = Some(m.preview_path.clone());
    m.receive(crate::jsondoc::parse(r#"{"t":"error","where":"scan","path":"/fx/v/work/Locked","msg":"permission denied"}"#).unwrap(), &mut w).unwrap();
    let rows = grid(&render::frame(&m, &theme, &map, 130, 31, Duration::ZERO), 130, 31);
    assert!(rows[30].contains("9 items  You don’t have permission to open this folder."), "{}", rows[30]);
    assert!(!rows[30].contains("dismiss"));
    assert_eq!(m.path, std::path::Path::new("/fx/v/work"));
    drop(w); reader.join().unwrap();
}

#[test]
fn caption_loop_paints_three_cycles_and_writes_nothing_between_beats() {
    let mut m = model(); let theme = Theme::from_text(""); let map = Map::load();
    m.total = 0; m.rows.clear(); m.path.push("Empty");
    let start = std::time::Instant::now();
    m.empty_at = start;
    let initial = render::frame_at(&m, &theme, &map, 130, 31, Duration::ZERO, start);
    let mut last = initial.clone();
    let mut terminal = Vec::new();
    let beat = Duration::from_millis(crate::tui::empty::ROTATE_MS as u64);
    let phrases = crate::tui::empty::PHRASES.len();
    let timeout = beat * (phrases as u32 * 2);
    let mut now = start;
    for cycle in 0..3 {
        let mut paints = 0;
        let mut idle_paints = 0;
        let mut wakes = 0;
        for _ in 0..phrases {
            let input_wait = crate::tui::wait::poll_wait_ms(false, now.duration_since(start), timeout);
            let wait = crate::tui::wait::next_wait_ms(&m, (130, 31), now, input_wait);
            assert_eq!(wait, beat.as_millis() as i32);
            let idle = render::frame_at(&m, &theme, &map, 130, 31, Duration::ZERO, now + beat / 2);
            idle_paints += usize::from(render::present(idle, &mut last, &mut terminal).unwrap());
            now += Duration::from_millis(wait as u64);
            wakes += 1;
            let out = render::frame_at(&m, &theme, &map, 130, 31, Duration::ZERO, now);
            let before = terminal.len();
            let painted = render::present(out, &mut last, &mut terminal).unwrap();
            assert!(painted, "terminal paint at {:?}", now.duration_since(start));
            assert_eq!(terminal.len() > before, painted);
            paints += usize::from(painted);
        }
        assert_eq!(paints, phrases);
        assert_eq!(wakes, phrases);
        assert_eq!(idle_paints, 0);
        assert_eq!(last, initial, "each cycle wraps to the first complete frame");
        eprintln!("TUI_FRAME_CAPTION_CYCLE cycle={} wakes={wakes} paints={paints} idle_paints={idle_paints}", cycle + 1);
    }
}
#[test]
fn t3_prepared_text_is_viewport_bounded_and_rest_frames_remain_identical() {
    let mut m = model(); let theme = Theme::from_text(""); let map = Map::load();
    m.preview_body = "A line with a large body\n".repeat(32000);
    crate::tui::layout::prepare(&mut m);
    assert!(m.text_lines.len() <= m.height - 3);
    let reference = render::frame(&m, &theme, &map, 130, 31, Duration::ZERO);
    let mut micros = Vec::new();
    for _ in 0..5 {
        let start = std::time::Instant::now();
        for _ in 0..100 {
            crate::tui::layout::prepare(&mut m);
            assert_eq!(render::frame(&m, &theme, &map, 130, 31, Duration::ZERO), reference);
        }
        micros.push(start.elapsed().as_micros() / 100);
    }
    eprintln!("t3 cached 130x31 frame cost, microseconds/frame: {}..{} (five batches of 100, debug build)", micros.iter().min().unwrap(), micros.iter().max().unwrap());
    m.preview_scroll = 1000;
    crate::tui::layout::prepare(&mut m);
    assert_eq!(m.text_lines[0].0, Some(1001));
}

#[test]
fn review_t3_r1_symlink_name_with_leading_combining_mark_never_panics() {
    let mut row = entry("a", false, 1);
    row.name = "\u{301}a".into();
    row.link = "target".into();
    let output = crate::tui::cells::row(&row, 51, Some("1 B"), true,
        crate::tui::cells::RowStyle::default(), &Theme::from_text(""), "");
    assert!(plain(&output).contains("a → target"));
    let theme = Theme::from_text("");
    for name in ["\u{301}a", "\u{301}", "e\u{301}界", "\x1b界\u{202e}a", "a very long symlink filename"] {
        row.name = name.into();
        row.link = "\u{301}界/target".into();
        for columns in 0..60 {
            for query in ["", "a", "界", "\u{301}"] {
                let output = crate::tui::cells::row(&row, columns, Some("1 B"), true,
                    crate::tui::cells::RowStyle::default(), &theme, query);
                let visible = plain(&output);
                assert_eq!(crate::tui::cells::text_width(&visible), columns, "{name:?} at {columns}");
                assert!(!visible.chars().any(|c| !crate::tui::cells::safe(c)));
                if columns >= 10 { assert!(visible.ends_with("1 B  ")); }
            }
        }
    }
}

#[test]
fn review_t3_r1_folder_preview_scroll_changes_rendered_children() {
    use crate::tui::{actions, input::Key, preview, wire};
    let (mut wire, reader) = wire::echo_wire();
    let mut m = model();
    m.height = 6;
    m.cursor = 0;
    preview::load(&mut m, true);
    preview::request(&mut m, &mut wire).unwrap();
    m.receive(crate::jsondoc::parse(r#"{"t":"peeked","path":"/fx/v/work/A","n":6,"rows":[{"n":"child-1","d":false},{"n":"child-2","d":false},{"n":"child-3","d":false},{"n":"child-4","d":false},{"n":"child-5","d":false},{"n":"child-6","d":false}]}"#).unwrap(), &mut wire).unwrap();
    preview::layout(&mut m);
    crate::tui::layout::prepare(&mut m);
    let theme = Theme::from_text("");
    let before = plain(&crate::tui::layout::preview(&m, 3, 50, 6, &theme));
    m.preview_focus = true;
    actions::key(&mut m, &Key::named("Down", ""), &Map::load(), &mut wire).unwrap();
    preview::layout(&mut m);
    crate::tui::layout::prepare(&mut m);
    let after = plain(&crate::tui::layout::preview(&m, 3, 50, 6, &theme));
    assert_eq!(m.preview_scroll, 1);
    eprintln!("preview_scroll={}, first child before={before:?}, after={after:?}", m.preview_scroll);
    assert_ne!(before, after, "a real preview Down must reveal the next child");
    assert!(before.contains("child-1") && after.contains("child-2"));
    let header = plain(&crate::tui::layout::preview(&m, 0, 50, 6, &theme));
    for _ in 0..10 { wheel(&mut m, 65, &mut wire); }
    assert_eq!(m.preview_scroll, 3);
    assert!(plain(&crate::tui::layout::preview(&m, 5, 50, 6, &theme)).contains("child-6"));
    actions::key(&mut m, &Key::named("Up", ""), &Map::load(), &mut wire).unwrap();
    preview::layout(&mut m);
    assert_eq!(m.preview_scroll, 2);
    assert!(plain(&crate::tui::layout::preview(&m, 3, 50, 6, &theme)).contains("child-3"));
    assert_eq!(plain(&crate::tui::layout::preview(&m, 0, 50, 6, &theme)), header);
    m.height = 9;
    preview::layout(&mut m);
    assert_eq!(m.preview_scroll, 0);
    for y in 0..m.height { assert_eq!(crate::tui::cells::text_width(&plain(&crate::tui::layout::preview(&m, y, 50, m.height, &theme))), 50); }
    wire::finish(wire, reader);
}

#[test]
fn review_t3_r1_archive_backend_facts_are_rendered() {
    use crate::tui::{preview, wire};
    let (mut wire, reader) = wire::echo_wire();
    let mut m = model();
    m.rows.get_mut(&7).unwrap().name = "bundle.zip".into();
    m.rows.get_mut(&7).unwrap().icon = "package-x-generic".into();
    m.rows.get_mut(&7).unwrap().kind = "ZIP archive".into();
    preview::load(&mut m, true);
    preview::request(&mut m, &mut wire).unwrap();
    m.receive(crate::jsondoc::parse(&format!(r#"{{"t":"meta","row":7,"token":{},"entries":3,"unpacked":156}}"#, m.preview_requested)).unwrap(), &mut wire).unwrap();
    preview::layout(&mut m);
    crate::tui::layout::prepare(&mut m);
    // The facts are the archive's own count line under the head now, not two metadata lines.
    assert!(m.preview_metadata.iter().all(|line| !line.starts_with("Entries") && !line.starts_with("Unpacked")));
    let theme = Theme::from_text("");
    let rows = (0..m.height).map(|y| plain(&crate::tui::layout::preview(&m,y,50,m.height,&theme))).collect::<Vec<_>>();
    let body = rows.join("\n");
    eprintln!("archive preview: {body}");
    wire::finish(wire, reader);
    assert_eq!(rows[2].trim_end(), " 3 entries · 156 B unpacked", "backend archive facts disappeared from the rendered preview");
}

fn wheel(m: &mut Model, button: u32, wire: &mut crate::tui::wire::Wire) {
    use crate::tui::{actions, input::{Key, Pointer}, preview};
    let mut key = Key::named("", "");
    key.pointer = Some(Pointer { button, x: 90, y: 6, released: false, motion: false });
    actions::key(m, &key, &Map::load(), wire).unwrap();
    preview::layout(m);
}
#[test]
fn review_t3_r1_archive_failure_reply_is_rendered_below_the_header() {
    use crate::tui::{preview, wire};
    let (mut wire, reader) = wire::echo_wire();
    let mut m = model();
    let row = m.rows.get_mut(&7).unwrap();
    row.name = "broken.zip".into(); row.icon = "package-x-generic".into(); row.kind = "ZIP archive".into();
    preview::load(&mut m, true);
    preview::request(&mut m, &mut wire).unwrap();
    preview::layout(&mut m);
    m.receive(crate::jsondoc::parse(&format!(r#"{{"t":"meta","row":7,"token":{},"afailed":true}}"#, m.preview_requested)).unwrap(), &mut wire).unwrap();
    preview::layout(&mut m);
    let theme = Theme::from_text("");
    let rows: Vec<_> = (0..m.height).map(|y| plain(&crate::tui::layout::preview(&m, y, 50, m.height, &theme))).collect();
    assert!(rows[0].contains("broken.zip"));
    assert!(rows[1].contains("ZIP archive"));
    assert_eq!(rows[2].trim(), "");
    // The can't-show state is centred in the body: the alert mark, then the sentence on the next row.
    let sentence = rows.iter().position(|row| row.trim() == "This archive could not be read.").expect("the failure sentence is drawn");
    assert_eq!(sentence, 3 + (m.height - 3 - 2) / 2 + 1);
    assert_eq!(rows[sentence - 1].trim(), "\u{ea6c}");
    assert!(rows[3..sentence - 1].iter().all(|row| row.trim().is_empty()));
    assert!(rows.iter().all(|row| crate::tui::cells::text_width(row) == 50));
    wire::finish(wire, reader);
}
#[test]
fn review_t3_r1_non_text_bodies_scroll_without_moving_the_header() {
    use crate::tui::{preview, wire};
    let (mut wire, reader) = wire::echo_wire();
    let theme = Theme::from_text("");
    for selected in [false, true] {
        let mut m = model(); m.height = 4; m.preview_body.clear(); m.preview_width = 0;
        if selected {
            m.selected.extend([0, 1, 2]);
            for i in 0..3 { m.selected_rows.insert(i, m.rows[&i].clone()); }
        } else {
            m.preview_metadata = vec!["Entries · 3".into(), "Unpacked · 156 B".into(), "This archive could not be read.".into()];
        }
        preview::layout(&mut m);
        let header = plain(&crate::tui::layout::preview(&m, 0, 50, 4, &theme));
        let before = plain(&crate::tui::layout::preview(&m, 3, 50, 4, &theme));
        wheel(&mut m, 65, &mut wire);
        assert_eq!(m.preview_scroll, 1);
        let after = plain(&crate::tui::layout::preview(&m, 3, 50, 4, &theme));
        assert_ne!(before, after);
        for _ in 0..5 { wheel(&mut m, 65, &mut wire); }
        assert_eq!(m.preview_scroll, 2);
        let last = plain(&crate::tui::layout::preview(&m, 3, 50, 4, &theme));
        assert!(last.contains(if selected { "Docs" } else { "This archive could not be read." }));
        assert_eq!(plain(&crate::tui::layout::preview(&m, 0, 50, 4, &theme)), header);
        for _ in 0..5 { wheel(&mut m, 64, &mut wire); }
        assert_eq!(m.preview_scroll, 0);
        assert_eq!(plain(&crate::tui::layout::preview(&m, 3, 50, 4, &theme)), before);
        m.height = 3;
        preview::layout(&mut m);
        assert_eq!(m.preview_scroll, 0);
    }
    wire::finish(wire, reader);
}
// Re-runs one test alone in a child with no controlling terminal, stdin from /dev/null and only the variables it sets; false inside that child.
pub fn isolated_test(name: &str, extra: &[(&str, &str)]) -> bool {
    if std::env::var_os("FLEA_TEST_ISOLATED_CHILD").is_some() { return false; }
    let mut command = std::process::Command::new("setsid");
    command.arg("-w").arg(std::env::current_exe().unwrap()).args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env_clear().env("PATH", "/usr/bin:/bin").env("LC_ALL", "C.UTF-8").env("FLEA_TEST_ISOLATED_CHILD", "1")
        .stdin(std::process::Stdio::null());
    for (key, value) in extra { command.env(key, value); }
    let output = command.output().unwrap();
    println!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "isolated test {name}: {}", output.status);
    assert!(String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed;"), "isolated test {name} ran no test");
    true
}

#[path = "frame_isolate_join_tests.rs"]
mod isolate_join;
// tui-long: while the list scrolls a heavy thumb in the frame ink rides the listing's separator, rows 18 to 26 for 54 to 80 of 80.
#[test]
fn the_scroll_thumb_rides_the_listing_separator_in_the_frame_ink() {
    let mut m = model();
    m.total = 80; m.top = 53; m.cursor = 79; m.scrolled_at = Some(std::time::Instant::now());
    let theme = Theme::from_text("");
    let out = render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO);
    let rows = grid(&out, 130, 31);
    let separator = |y: usize| rows[y].chars().nth(79).unwrap();
    let inside = |y: usize| rows[y].chars().nth(78).unwrap();
    for y in 2..2 + 27 {
        assert_eq!(separator(y), if (20..29).contains(&y) { '┃' } else { '│' }, "row {y}");
        assert_eq!(inside(y), ' ', "row {y}");
    }
    assert!(out.contains(&format!("{}┃", theme.frame)));
    m.preview_visible = false;
    let rows = grid(&render::frame(&m, &theme, &Map::load(), 130, 31, Duration::ZERO), 130, 31);
    for (y, row) in rows.iter().enumerate().take(2 + 27).skip(2) {
        assert_eq!(row.chars().nth(129) == Some('┃'), (20..29).contains(&y), "row {y} with no preview takes the last cell");
    }
}
