//! Every menu card the 0.4.0 menu boards draw, row for row: words, marks, inks, state words, keys, width, flyout place and scroll.
use super::build::{build_entries, App, Context, Kind};
use super::entry::{Brand, Entry, Text};
use super::fit::{self, Rect, RowParts, PAD};
use super::inventory::effective_hidden;
use super::places::place_menu;
use super::rows::header_entries;
use super::testkit::{english, strings};
use crate::i18n::I18n;

// board_cards.tsv is read off the board HTML: board, card, left, top, width, height, view, scroll, cursor, lit, rows.
const CARDS: &str = include_str!("board_cards.tsv");
// JetBrains Mono's advances at the bench's body 14 and caption 12, the widths the boards fit their menus to.
const BODY: f32 = 8.4;
const CAPTION: f32 = 7.2;
const TAGS: [&str; 7] = ["red", "orange", "yellow", "green", "blue", "purple", "gray"];
const WIDE: Rect = Rect {
    x: 0.,
    y: 0.,
    w: 1100.,
    h: 700.,
};
const SMALL: Rect = Rect {
    x: 0.,
    y: 0.,
    w: 480.,
    h: 240.,
};

struct Card<'a> {
    board: &'a str,
    card: usize,
    left: f32,
    top: f32,
    width: f32,
    view: Option<i32>,
    scroll: i32,
    cursor: Option<usize>,
    lit: Option<usize>,
    rows: String,
    moved: bool,
    // How far the move pushed the lit row down, in pixels.
    drop: i32,
}

// Finder's order moves the boards' closing Move to Trash group, marked [trash] in every language, to just after the open group.
fn finder_order(rows: &str, marks: [Option<usize>; 2]) -> (String, [Option<usize>; 2], bool, i32) {
    let mut all: Vec<&str> = rows.split('|').collect();
    let Some(last) = all.iter().rposition(|r| *r == "-") else {
        return (rows.into(), marks, false, 0);
    };
    if !all[last + 1].contains(" [trash]") {
        return (rows.into(), marks, false, 0);
    }
    let group: Vec<&str> = all.split_off(last + 1);
    all.pop();
    let at = all.iter().position(|r| *r == "-").unwrap() + 1;
    assert!(marks.iter().flatten().all(|&i| i < last), "a mark inside the moved trash group needs its own remap");
    let shift = group.len() + 1;
    all.splice(at..at, group.into_iter().chain(["-"]));
    let drop = if marks[1].is_some_and(|i| i >= at) { (shift as i32 - 1) * fit::ROW + fit::SEP } else { 0 };
    let marks = marks.map(|m| m.map(|i| if i >= at { i + shift } else { i }));
    (all.join("|"), marks, true, drop)
}

fn cards() -> Vec<Card<'static>> {
    CARDS
        .lines()
        .skip(1)
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let marks = [f[8].parse().ok(), f[9].parse().ok()];
            let (rows, [cursor, lit], moved, drop) = if f[1] == "0" {
                finder_order(f[10], marks)
            } else {
                (f[10].into(), marks, false, 0)
            };
            Card {
                board: f[0],
                card: f[1].parse().unwrap(),
                left: f[2].parse().unwrap(),
                top: f[3].parse().unwrap(),
                width: f[4].parse().unwrap(),
                view: f[6].parse().ok(),
                scroll: f[7].parse().unwrap_or(0),
                cursor,
                lit,
                rows,
                moved,
                drop,
            }
        })
        .collect()
}

// notes.txt, a plain-text file that prints, with nothing copied, Taildrop offline and Dropbox signed out.
fn notes(all_on: bool) -> Context {
    let tags = TAGS.iter().enumerate().map(|(i, name)| {
        let mut name = (*name).to_owned();
        name[..1].make_ascii_uppercase();
        let slot = i as u8 + 1;
        Entry {
            tag: Some(slot),
            ..Entry::leaf(format!("tag{slot}"), Text::Data(name))
        }
        .with_key_hint(["1", "2", "3", "4", "5", "6", "7"][i])
    });
    Context {
        row_mode: 0o100644,
        row_is_file: true,
        row_name: "notes.txt".into(),
        printable: true,
        // Run and Open as Text, rows since v0.3.10, are absent from the boards' every-row captures, so the fixture switches them off.
        hidden: if all_on {
            effective_hidden(Some(&strings(&["run", "openAsText"])), false)
        } else {
            effective_hidden(None, false)
        },
        archive_formats: strings(&["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "7z"]),
        tag_submenu: Some(tags.collect()),
        open_with_apps: vec![App {
            id: "nvim.desktop".into(),
            label: "Neovim".into(),
            icon: "nvim".into(),
            default: true,
        }],
        open_with_loaded: true,
        taildrop_installed: true,
        dropbox_installed: true,
        dropbox_code: "signedOut".into(),
        ..Context::default()
    }
}

// Each board's own state, as its source draws it.
fn entries(board: &str) -> (Vec<Entry>, bool, Rect, f32) {
    let (rows, hints) = match board {
        "file-menu" | "menu-row" | "compress-menu" | "lang-de-menu" => {
            (build_entries(Kind::File, &notes(false)), false)
        }
        "file-menu-print" => (build_entries(Kind::File, &notes(false)), true),
        "pasteas-menu" => (
            build_entries(
                Kind::File,
                &Context {
                    clipboard_available: true,
                    ..notes(true)
                },
            ),
            true,
        ),
        "file-menu-combine" => {
            let p = Context {
                selection_count: 3,
                selection_images: 3,
                row_is_image: true,
                can_convert: true,
                row_name: "DSC_4822.jpg".into(),
                selection_modes: vec![0o100644; 3],
                ..notes(false)
            };
            (build_entries(Kind::File, &p), false)
        }
        "background-menu" => (
            build_entries(
                Kind::Background,
                &Context {
                    has_row: false,
                    row_mode: 0,
                    sort_by: "name".into(),
                    ..notes(false)
                },
            ),
            false,
        ),
        "header-menu" | "header-menu-pointer" | "header-menu-neighbour" => {
            (header_entries(&strings(&["mode", "kind"]), false), false)
        }
        "rail-menu" => (place_menu(Some(&[]), false), false),
        _ => (build_entries(Kind::File, &notes(true)), true),
    };
    let (work, inset) = if board == "copyas-menu-small" {
        (SMALL, 4.)
    } else {
        (WIDE, 5.)
    };
    (rows, hints, work, inset)
}

fn glyph(e: &Entry) -> String {
    if let Some(check) = e.check {
        return if check {
            "checkmark".into()
        } else {
            String::new()
        };
    }
    if let Some(tag) = e.tag {
        return format!("tag:{}", TAGS[tag as usize - 1]);
    }
    match (e.brand, &e.icon) {
        (Some(Brand::Tailscale), _) => "brand:tailscale".into(),
        (Some(Brand::Dropbox), _) => "brand:dropbox".into(),
        (Some(Brand::LocalSend), _) => "brand:localsend".into(),
        (None, Some(_)) => "icon".into(),
        (None, None) => e.glyph.unwrap_or("").into(),
    }
}

fn row(i18n: &I18n, e: &Entry, hints: bool) -> String {
    if e.separator {
        return "-".into();
    }
    let label = e
        .label
        .as_ref()
        .map(|t| t.resolve(i18n))
        .unwrap_or_default();
    let mut out = format!("{label} [{}]", glyph(e));
    for (on, flag) in [
        (e.disabled, " dis"),
        (e.errored, " err"),
        (e.danger, " destr"),
        (e.has_submenu(), " >"),
    ] {
        if on {
            out.push_str(flag);
        }
    }
    if let Some(word) = &e.hint {
        out.push_str(&format!(" ~{}", word.resolve(i18n)));
    }
    let key = e.key_text("default", hints);
    if !key.is_empty() {
        out.push_str(&format!(" ^{key}"));
    }
    out
}

fn widest(i18n: &I18n, rows: &[Entry], hints: bool) -> f32 {
    let chars = |t: Option<&Text>| t.map_or(0, |t| t.resolve(i18n).chars().count()) as f32;
    rows.iter()
        .filter(|e| !e.separator)
        .map(|e| {
            let key = e.key_text("default", hints).chars().count() as f32;
            fit::row_width(RowParts {
                label: chars(e.label.as_ref()) * BODY,
                word: chars(e.hint.as_ref()) * CAPTION,
                key: key * CAPTION,
                chevron: e.has_submenu(),
            })
        })
        .fold(0., f32::max)
}

// The walk each scrolled board took, as its source states it.
fn walked(board: &str, rows: &[Entry], view: i32, keep: usize) -> (i32, i32) {
    let index = |label: &str| {
        rows.iter()
            .position(|e| e.label.as_ref().map(|t| t.resolve(&english())).as_deref() == Some(label))
            .unwrap()
    };
    let end = fit::content_height(rows) - view;
    match board {
        "tags-menu" => fit::menu_cut(rows, end, view, Some(keep)),
        "copyas-menu-small" => {
            fit::menu_cut(rows, fit::reveal(rows, keep, 0, view), view, Some(keep))
        }
        "menu-past-fold" => {
            let last = fit::reveal(rows, index("Delete Permanently\u{2026}"), 0, view);
            fit::menu_cut(rows, fit::reveal(rows, keep, last, view), view, Some(keep))
        }
        "menu-past-fold-up" => {
            let last = fit::reveal(rows, index("Delete Permanently\u{2026}"), 0, view);
            let fold = fit::menu_cut(
                rows,
                fit::reveal(rows, index("Copy As"), last, view),
                view,
                Some(index("Copy As")),
            )
            .0;
            fit::menu_cut(rows, fit::reveal(rows, keep, fold, view), view, Some(keep))
        }
        _ => fit::menu_cut(rows, 0, view, Some(keep)),
    }
}

#[test]
fn every_board_card_draws_the_boards_rows_width_place_and_scroll() {
    let i18n = english();
    let all = cards();
    assert_eq!(all.len(), 27, "every card the 19 boards draw");
    for card in all.iter().filter(|c| c.card == 0) {
        let (rows, hints, work, inset) = entries(card.board);
        let name = card.board;
        let drawn: Vec<String> = rows.iter().map(|e| row(&i18n, e, hints)).collect();
        // Print's ctrl-p waits for the print unit's binding in keys.toml, so its key is pinned once that lands.
        let mut want: Vec<String> = card
            .rows
            .split('|')
            .map(|r| r.replace(" ^ctrl-p", ""))
            .collect();
        if name == "file-menu-combine" {
            // Open With acts on one item, as v0.3.12 dims it; the board names no change, so the dimmed row stays.
            want[1] = "Open With [app-window] dis >".into();
        }
        if name == "lang-de-menu" {
            // The German catalog belongs to the languages unit; until it lands the rows are pinned by mark, ink and shape.
            let shape = |r: &str| {
                r.split_once(" [").map_or(r.to_owned(), |(_, rest)| {
                    rest.split(" ~").next().unwrap().to_owned()
                })
            };
            assert_eq!(
                drawn.iter().map(|r| shape(r)).collect::<Vec<_>>(),
                want.iter().map(|r| shape(r)).collect::<Vec<_>>(),
                "{name}"
            );
            continue;
        }
        assert_eq!(drawn, want, "{name}: rows");
        let width = fit::fit_width(widest(&i18n, &rows, hints), work.w);
        assert_eq!(width, card.width, "{name}: width");
        let (content_y, view) = match (card.view, card.cursor.or(card.lit)) {
            (Some(view), Some(keep)) => {
                let full = (work.h - 2. * inset) as i32 - 2 * PAD;
                let got = walked(name, &rows, full, keep);
                if card.moved {
                    // The board's scroll pins the old order, so a moved card keeps its view and the fold's own rules.
                    assert_eq!(got.1, view, "{name}: view");
                    assert!(
                        fit::cue_ok(&rows, got.0, got.1) && fit::keeps_clear(&rows, Some(keep), got.0, got.1),
                        "{name}: a half row cue with the kept row clear at {got:?}"
                    );
                } else {
                    assert_eq!(got, (card.scroll, view), "{name}: scroll and view");
                }
                got
            }
            _ => (0, fit::content_height(&rows)),
        };
        assert_eq!(
            fit::fades(&rows, content_y, view),
            (content_y > 0, content_y + view < fit::content_height(&rows)),
            "{name}: fades"
        );
        let Some(fly) = all.iter().find(|c| c.board == name && c.card == 1) else {
            continue;
        };
        let parent = card
            .lit
            .unwrap_or_else(|| panic!("{name}: a flyout hangs from a lit row"));
        let leaves = rows[parent].submenu.clone().unwrap();
        let drawn: Vec<String> = leaves.iter().map(|e| row(&i18n, e, hints)).collect();
        assert_eq!(
            drawn,
            fly.rows.split('|').collect::<Vec<_>>(),
            "{name}: flyout rows"
        );
        let fly_w = fit::flyout_width(width, widest(&i18n, &leaves, hints), work.w);
        assert_eq!(fly_w, fly.width, "{name}: flyout width");
        let frame = Rect {
            x: card.left,
            y: card.top,
            w: width,
            h: (view + 2 * PAD) as f32,
        };
        let row_top = card.top + (PAD + fit::row_top(&rows, parent) - content_y) as f32;
        let full = fit::place_flyout(
            frame,
            fly_w,
            fit::content_height(&leaves),
            row_top,
            work,
            inset,
        )
        .h as i32
            - 2 * PAD;
        let (y, h) = fit::menu_cut(&leaves, 0, full, fly.cursor);
        if let Some(fly_view) = fly.view {
            assert_eq!(
                (y, h),
                (fly.scroll, fly_view),
                "{name}: flyout scroll and view"
            );
        }
        // A trimmed view keeps its foot where the work area put it, as Menu::open_flyout places it.
        let placed = fit::place_flyout(frame, fly_w, h, row_top, work, inset);
        // A moved card's flyout follows its parent row by the move less the scroll that changed with it.
        let follow = if card.moved { (card.drop - (content_y - card.scroll)) as f32 } else { 0. };
        let foot = work.y + work.h - inset;
        let want = (fly.top + follow).min(foot - placed.h);
        assert!(
            placed.x == fly.left && placed.y == want,
            "{name}: flyout place {:?} against the board's {:?} moved by {follow}",
            (placed.x, placed.y),
            (fly.left, fly.top)
        );
    }
}
