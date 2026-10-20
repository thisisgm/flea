use super::*;
use crate::tui::{action_key_tests::drain, actions, input::Pointer, model::Deletion, render::{self, frame_tests::{grid, model}}, wire};
use std::time::Duration;

// A palette whose roles all differ, so an ink assertion cannot pass on a shared fallback colour.
pub(crate) const PALETTE: &str = "foreground = \"#a9b1d6\"\nbackground = \"#1a1b26\"\naccent = \"#7aa2f7\"\nurgent = \"#f7768e\"\n";
pub(crate) type Ink = (String, String, bool);

// Each cell's foreground, background and bold, read from a frame's cursor moves and SGR codes.
// Sample input: "\x1b[2;5H\x1b[1;38;2;169;177;214mA" puts a bold A at x 4, y 1.
pub(crate) fn inks(text: &str, columns: usize, lines: usize) -> Vec<Vec<Ink>> {
    let mut cells = vec![vec![Ink::default(); columns]; lines];
    let (mut x, mut y, mut ink) = (0, 0, Ink::default());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.next() == Some('[') {
            let mut args = String::new();
            for code in chars.by_ref() {
                if ('@'..='~').contains(&code) {
                    if code == 'H' {
                        let mut numbers = args.split(';').map(|s| s.parse::<usize>().unwrap_or(1).saturating_sub(1));
                        y = numbers.next().unwrap_or(0); x = numbers.next().unwrap_or(0);
                    } else if code == 'm' { sgr(&args, &mut ink); }
                    break;
                }
                args.push(code);
            }
        } else if y < lines && x < columns {
            let width = crate::tui::cells::width(c);
            if width > 0 { cells[y][x] = ink.clone(); x += width; }
        }
    }
    cells
}
// Sample input: "1;38;2;169;177;214" sets bold and the truecolour foreground.
fn sgr(args: &str, ink: &mut Ink) {
    let parts: Vec<&str> = args.split(';').collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "" | "0" => *ink = Ink::default(),
            "1" => ink.2 = true,
            "22" => ink.2 = false,
            "38" | "48" => {
                let n = if parts.get(i + 1) == Some(&"2") { 5 } else { 3 };
                let value = parts[i + 1..(i + n).min(parts.len())].join(";");
                if parts[i] == "38" { ink.0 = value } else { ink.1 = value }
                i += n - 1;
            }
            _ => {}
        }
        i += 1;
    }
}
// The colour a theme role sets, in the form inks() records it.
pub(crate) fn role(code: &str) -> String {
    let mut ink = Ink::default();
    sgr(code.trim_start_matches("\x1b[").trim_end_matches('m'), &mut ink);
    if ink.0.is_empty() { ink.1 } else { ink.0 }
}
pub(crate) fn at(rows: &[String], y: usize, x: usize, n: usize) -> String { rows[y].chars().skip(x).take(n).collect() }
// A pointer event at a 1-based terminal cell.
pub(crate) fn pointer(button: u32, x: usize, y: usize, motion: bool) -> Key {
    Key { name: String::new(), text: String::new(), mods: String::new(), pointer: Some(Pointer { button, x, y, released: false, motion }) }
}
pub(crate) fn screen(m: &Model, theme: &Theme) -> (Vec<String>, Vec<Vec<Ink>>, String) {
    let lines = m.height + crate::tui::layout::CHROME_ROWS;
    let out = render::frame(m, theme, &Map::load(), m.columns, lines, Duration::ZERO);
    (grid(&out, m.columns, lines), inks(&out, m.columns, lines), out)
}
pub(crate) fn sent<'a>(values: &'a [Json], command: &str, op: &str) -> Vec<&'a Json> {
    values.iter().filter(|v| text(v, "c") == command && (op.is_empty() || text(v, "op") == op)).collect()
}
pub(super) fn deleting(count: usize, token: usize) -> Model {
    let mut m = model();
    m.deletion = Some(Deletion { token, count, ..Default::default() });
    m
}
pub(super) fn press(m: &mut Model, key: Key, w: &mut Wire) { actions::key(m, &key, &Map::load(), w).unwrap(); }

#[test]
fn delete_card_matches_its_board_and_cancel_stays_default_as_focus_moves() {
    let theme = Theme::from_text(PALETTE);
    let mut m = deleting(1, 1);
    let (rows, ink, _) = screen(&m, &theme);
    assert_eq!([at(&rows, 12, 35, 1), at(&rows, 12, 94, 1), at(&rows, 18, 35, 1), at(&rows, 18, 94, 1)], ["┌", "┐", "└", "┘"]);
    assert_eq!(ink[12][35].0, role(&theme.frame));
    assert_eq!(at(&rows, 13, 38, 26), "Delete 1 item permanently?");
    assert!(ink[13][38].2, "the title is bold");
    assert!((64..87).all(|x| !ink[13][x].2), "the padding after the title is plain");
    assert_eq!((at(&rows, 13, 89, 3), ink[13][89].0.clone()), ("esc".into(), role(&theme.muted)));
    assert_eq!((at(&rows, 14, 37, 56), ink[14][38].0.clone()), (format!(" {} ", "─".repeat(54)), role(&theme.rule)));
    assert_eq!(at(&rows, 15, 38, 54), "This item is deleted from disk. This cannot be undone.");
    assert_eq!(at(&rows, 17, 70, 22), "[ Cancel ]  [ Delete ]");
    assert_eq!(ink[15][60].1, role(&theme.background), "the card draws on the ground");
    let brackets = |ink: &[Vec<Ink>], x: usize| (ink[17][x].0.clone(), ink[17][x + 9].0.clone());
    assert_eq!(brackets(&ink, 70), (role(&theme.foreground), role(&theme.foreground)), "focused Cancel");
    assert_eq!(brackets(&ink, 82), (role(&theme.frame), role(&theme.frame)), "unfocused Delete");
    assert_eq!((ink[17][74].1.clone(), ink[17][86].1.clone()), (role(&theme.wash), role(&theme.background)), "the default keeps the wash");
    let (mut w, reader) = wire::echo_wire();
    press(&mut m, Key::named("Tab", ""), &mut w);
    assert!(m.deletion.as_ref().unwrap().destructive);
    let (_, ink, _) = screen(&m, &theme);
    assert_eq!(brackets(&ink, 70), (role(&theme.accent), role(&theme.accent)), "Cancel keeps the accent of the default");
    assert_eq!(brackets(&ink, 82), (role(&theme.foreground), role(&theme.foreground)), "focused Delete");
    assert_eq!((ink[17][74].1.clone(), ink[17][86].1.clone()), (role(&theme.wash), role(&theme.background)));
    assert!(drain(&mut w).is_empty(), "moving focus sends nothing");
    wire::finish(w, reader);
}

#[test]
fn enter_and_space_press_the_focused_button_and_an_unchecked_delete_never_runs() {
    for activator in ["Return", "Space"] {
        let (mut w, reader) = wire::echo_wire();
        let mut m = deleting(3, 9);
        press(&mut m, Key::named(activator, ""), &mut w);
        let values = drain(&mut w);
        assert!(m.deletion.is_none() && sent(&values, "menuaction", "delete").is_empty());
        assert_eq!(sent(&values, "menuaction", "close").len(), 1, "{activator} on Cancel closes");
        let mut m = deleting(3, 9);
        press(&mut m, Key::named("Tab", ""), &mut w);
        press(&mut m, Key::named(activator, ""), &mut w);
        let values = drain(&mut w);
        let delete = sent(&values, "menuaction", "delete");
        assert_eq!(delete.len(), 1, "{activator} on Delete deletes");
        assert_eq!(count(delete[0], "token"), 9);
        assert_eq!(m.menu_action, "deleteRunning");
        let mut m = deleting(3, 0);
        for key in [Key::named("Tab", ""), Key::named("Right", ""), Key::character('l', "")] { press(&mut m, key, &mut w); }
        assert!(!m.deletion.as_ref().unwrap().destructive, "Delete cannot take focus before the backend checked the items");
        press(&mut m, Key::named(activator, ""), &mut w);
        assert!(sent(&drain(&mut w), "menuaction", "delete").is_empty());
        wire::finish(w, reader);
    }
}

#[test]
fn pointer_hover_focuses_a_button_and_a_click_presses_it_or_cancels_outside() {
    let (mut w, reader) = wire::echo_wire();
    let mut m = deleting(1, 4);
    let card = delete_card(&m);
    let (cancel, delete) = (card.buttons[0], card.buttons[1]);
    press(&mut m, pointer(0, delete.0 + 1, delete.1 + 1, true), &mut w);
    assert!(m.deletion.as_ref().unwrap().destructive, "hovering Delete focuses it");
    press(&mut m, pointer(0, cancel.0 + cancel.2, cancel.1 + 1, true), &mut w);
    assert!(!m.deletion.as_ref().unwrap().destructive, "the last cell of Cancel is Cancel's");
    press(&mut m, pointer(0, card.left() + 3, card.y + 3, false), &mut w);
    press(&mut m, pointer(65, card.left() + 3, card.y + 3, false), &mut w);
    assert!(m.deletion.is_some() && drain(&mut w).is_empty(), "a click on the body or a wheel step does nothing");
    press(&mut m, pointer(0, delete.0 + 1, delete.1 + 1, false), &mut w);
    assert_eq!(sent(&drain(&mut w), "menuaction", "delete").len(), 1);
    let mut m = deleting(1, 4);
    press(&mut m, pointer(0, 1, 1, false), &mut w);
    assert!(m.deletion.is_none());
    assert_eq!(sent(&drain(&mut w), "menuaction", "close").len(), 1, "a click outside cancels");
    wire::finish(w, reader);
}

#[test]
fn delete_titles_and_bodies_count_one_item_and_many() {
    for (count, title, body) in [(1, "Delete 1 item permanently?", "This item is deleted from disk. This cannot be undone."),
        (3, "Delete 3 items permanently?", "These items are deleted from disk. This cannot be undone.")] {
        let m = deleting(count, 1);
        let (title_text, rows, _) = delete_text(&m);
        assert_eq!((title_text.as_str(), rows.concat().as_str()), (title, body));
    }
    let m = deleting(0, 0);
    assert_eq!(delete_text(&m).1.concat(), "Inspecting selected items…");
}

#[test]
fn delete_buttons_stay_on_screen_and_hit_testable_at_small_sizes() {
    let theme = Theme::from_text(PALETTE);
    for columns in [12, 28, 80] {
        for height in [4, 20] {
            let mut m = deleting(3, 1);
            m.columns = columns;
            m.height = height;
            let card = delete_card(&m);
            let (rows, _, _) = screen(&m, &theme);
            assert_eq!(card.buttons.len(), 2);
            assert_ne!(card.buttons[0], card.buttons[1]);
            for (index, (label, &(x, y, width))) in ["[ Cancel ]", "[ Delete ]"].iter().zip(&card.buttons).enumerate() {
                assert!(x + width <= columns && y < height + 4, "{columns}x{height} {label} at {x},{y}");
                assert_eq!(at(&rows, y, x, width), *label, "{columns}x{height}");
                assert_eq!(card.button(x + 1, y + 1), Some(index));
                assert_eq!(card.button(x + width, y + 1), Some(index));
            }
        }
    }
}

#[test]
fn padded_cards_never_clip_their_text() {
    let theme = Theme::from_text(PALETTE);
    let path = "/fx/v/work/a-long-file-name-with-an-extension-that-wraps.txt";
    for columns in [12, 28, 30, 80] {
        let mut m = deleting(3, 1);
        m.columns = columns;
        let card = delete_card(&m);
        let (rows, _, _) = screen(&m, &theme);
        let body: String = (0..card.shown).map(|i| at(&rows, card.y + 3 + i, card.left(), card.content())).collect();
        assert_eq!(body.replace(' ', ""), "These items are deleted from disk. This cannot be undone.".replace(' ', ""), "{columns} columns");
        let mut m = model();
        m.columns = columns;
        m.sheet = true;
        m.properties = Some(vec![("Path".into(), path.into())]);
        let card = properties_card(&m);
        let (rows, _, _) = screen(&m, &theme);
        let (label, _) = property_rows(&m);
        let value: String = (0..card.shown).map(|i| at(&rows, card.y + 3 + i, card.left() + label, card.content() - label).trim_end().to_owned()).collect();
        assert_eq!(value, path, "{columns} columns");
    }
}

#[test]
fn a_value_still_shows_beside_a_label_wider_than_the_card() {
    let theme = Theme::from_text(PALETTE);
    let mut m = model();
    m.columns = 12;
    m.sheet = true;
    m.properties = Some(vec![("Permissions".into(), "0644".into())]);
    let card = properties_card(&m);
    let (rows, _, _) = screen(&m, &theme);
    let shown: String = (0..card.shown).map(|i| at(&rows, card.y + 3 + i, card.left(), card.content())).collect();
    assert!(shown.contains("0644"), "{shown:?}");
}

#[test]
fn a_card_title_cut_for_its_room_is_plain_text_with_a_bold_ellipsis() {
    let theme = Theme::from_text(PALETTE);
    let m = deleting(1, 1);
    let card = delete_card(&m);
    let room = card.content() - (text_width(&m.i18n.tr("card.esc")) + 2);
    let title = "Delete one item permanently and remove every copy from the disk, ok?";
    let mut out = String::new();
    frame(&mut out, &m, &theme, &card, title);
    let lines = m.height + crate::tui::layout::CHROME_ROWS;
    let (rows, ink) = (grid(&out, m.columns, lines), inks(&out, m.columns, lines));
    let (y, x) = (card.y + 1, card.left());
    let shown: String = title.chars().take(room - 1).collect();
    assert_eq!(at(&rows, y, x, room), format!("{shown}\u{2026}"));
    assert!((x..x + room).all(|c| ink[y][c].2), "the cut title and its ellipsis are bold");
    assert_eq!(at(&rows, y, x + room + 2, 3), "esc");
}
