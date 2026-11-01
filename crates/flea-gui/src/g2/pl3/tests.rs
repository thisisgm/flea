//! The Trash pane as a real entity in a real window: the wire, the strip's Empty Trash… states, the card, the menu.
use super::board::{HOVER, PRESS};
use super::ink::Ink;
use super::pane::{Button, TrashPane};
use super::seams::{At, Card, Outside};
use crate::list::bench::real_cx;
use crate::list::Palette;
use crate::palette::Theme;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_pl3::host::Item;
use gpui::{px, size, TestAppContext, WindowHandle};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Call {
    Send(String),
    Card(Option<Card>),
    Menu(Vec<&'static str>, At),
    Status(String, bool),
    Result(String, bool),
    Count(usize),
    Back,
    Escape,
    Rail,
    Action(String),
}

pub(super) struct Recorder(pub Rc<RefCell<Vec<Call>>>);

impl Outside for Recorder {
    fn send(&mut self, line: String) {
        self.0.borrow_mut().push(Call::Send(line));
    }
    fn card(&mut self, card: Option<Card>) {
        self.0.borrow_mut().push(Call::Card(card));
    }
    fn menu(&mut self, items: Vec<Item>, at: At) {
        let ids = items
            .iter()
            .filter_map(|item| match item {
                Item::Entry(entry) => Some(entry.id),
                Item::Separator => None,
            })
            .collect();
        self.0.borrow_mut().push(Call::Menu(ids, at));
    }
    fn status(&mut self, text: String, error: bool) {
        self.0.borrow_mut().push(Call::Status(text, error));
    }
    fn result(&mut self, headline: String, _: String, error: bool) {
        self.0.borrow_mut().push(Call::Result(headline, error));
    }
    // A showing status message takes one Escape, as StatusBar.escapePressed dismisses it.
    fn escape_taken(&mut self) -> bool {
        let mut calls = self.0.borrow_mut();
        let showing = calls
            .iter()
            .rev()
            .take_while(|call| **call != Call::Escape)
            .any(|call| matches!(call, Call::Status(..)));
        if showing {
            calls.push(Call::Escape);
        }
        showing
    }
    fn count(&mut self, total: usize) {
        self.0.borrow_mut().push(Call::Count(total));
    }
    fn focus_rail(&mut self) {
        self.0.borrow_mut().push(Call::Rail);
    }
    fn back(&mut self) {
        self.0.borrow_mut().push(Call::Back);
    }
    fn action(&mut self, name: String) {
        self.0.borrow_mut().push(Call::Action(name));
    }
}

pub(super) fn english() -> Arc<I18n> {
    let none = std::path::PathBuf::from("/nonexistent/flea-pl3");
    Arc::new(I18n::load(Paths {
        omarchy: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../src/i18n/fixtures/omarchy"),
        language_file: none,
        catalogs: Vec::new(),
    }))
}

pub(super) fn ink() -> Ink {
    // The window's own text style is unavailable here, so the theme roles stand in, as Palette::inherited does.
    // Each role blends the theme foreground at its own opacity, so a role drawn in another's place changes the ink.
    let theme = Theme::default();
    let foreground = theme.foreground;
    let palette = Palette {
        background: theme.background,
        foreground,
        muted: foreground.opacity(0.65),
        error: foreground.opacity(0.95),
        error_on_surface: foreground.opacity(0.96),
        metadata: foreground.opacity(0.62),
        metadata_frame: foreground.opacity(0.5),
        accent: foreground.opacity(0.9),
        accent_frame: foreground.opacity(0.85),
        symlink: foreground.opacity(0.8),
        executable: foreground.opacity(0.75),
        marked: foreground.opacity(0.35),
        muted_frame: foreground.opacity(0.45),
        tags: theme.tag_menu_inks,
    };
    Ink {
        muted_frame: foreground.opacity(0.42),
        ..Ink::interim(&palette, px(8.))
    }
}

#[test]
fn the_ink_fixture_gives_each_drawn_role_its_own_ink() {
    // The surface and the error ink are the background and the foreground by design, so they are not checked here.
    let ink = ink();
    let roles = [
        ink.background,
        ink.foreground,
        ink.muted,
        ink.muted_frame,
        ink.accent,
        ink.accent_frame,
        ink.mark_fill,
    ];
    for (at, role) in roles.iter().enumerate() {
        assert!(
            roles[at + 1..].iter().all(|other| other != role),
            "role {at} shares its ink with a later role"
        );
    }
}

pub(super) fn open(cx: &mut TestAppContext) -> (WindowHandle<TrashPane>, Rc<RefCell<Vec<Call>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let recorder = Recorder(calls.clone());
    let window = cx.open_window(size(px(908.), px(646.)), move |window, cx| {
        // A pane-only window takes its rem size as the text size; the boards draw at 14.
        window.set_rem_size(px(14.));
        let mut pane = TrashPane::new(
            english(),
            Box::new(recorder),
            ink(),
            "default",
            "/home/gm".into(),
            cx,
        );
        pane.monitor_start = None;
        pane.resize(908., 646., cx);
        pane
    });
    (window, calls)
}

pub(super) fn row(name: &str, image: Option<&str>) -> String {
    let thumbnail = image.map_or("null".to_string(), |path| {
        format!(r#"{{"path":"{path}","provider":"l1:1","identity":"b-{name}","mtime":5}}"#)
    });
    format!(
        r#"{{"uri":"trash:///{name}","original":"/fx/v/work/{name}","size":13,"deleted":"2025-06-14T09:30:00","directory":false,"icon":"text-x-generic","identity":"id-{name}","bulkSelected":false,"storageClass":"","thumbnail":{thumbnail}}}"#
    )
}

pub(super) fn sent(calls: &Rc<RefCell<Vec<Call>>>) -> Vec<String> {
    calls
        .borrow()
        .iter()
        .filter_map(|call| match call {
            Call::Send(line) => Some(line.clone()),
            _ => None,
        })
        .collect()
}

fn last_id(calls: &Rc<RefCell<Vec<Call>>>, op: &str) -> u64 {
    let line = sent(calls)
        .into_iter()
        .rev()
        .find(|line| line.contains(&format!(r#""op":"{op}""#)))
        .expect("the op was sent");
    let text = &line[line.rfind(r#""id":"#).unwrap() + 5..];
    text.trim_end_matches('}').parse().unwrap()
}

/// Answers the last request of `op` with these fields.
pub(super) fn answer(
    cx: &mut TestAppContext,
    window: WindowHandle<TrashPane>,
    calls: &Rc<RefCell<Vec<Call>>>,
    op: &str,
    fields: &str,
) {
    let id = last_id(calls, op);
    let text = format!(r#"{{"t":"trashbrowse","id":{id},"op":"{op}","ok":true{fields}}}"#);
    window
        .update(cx, |pane, _, cx| pane.line(&text, cx))
        .unwrap();
}

pub(super) fn listed(rows: &[String]) -> String {
    format!(
        r#","total":{},"start":0,"rows":[{}],"present":[]"#,
        rows.len(),
        rows.join(",")
    )
}

pub(super) fn opened(
    cx: &mut TestAppContext,
    rows: &[String],
) -> (WindowHandle<TrashPane>, Rc<RefCell<Vec<Call>>>) {
    let (window, calls) = open(cx);
    window.update(cx, |pane, _, cx| pane.open("", cx)).unwrap();
    answer(cx, window, &calls, "list", &listed(rows));
    answer(
        cx,
        window,
        &calls,
        "summary",
        r#","bytes":13,"partial":false"#,
    );
    (window, calls)
}

#[test]
fn trash_pane_lists_lands_and_counts() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("old-draft.txt", None)]);
    window
        .update(&mut cx, |pane, _, _| {
            let paint = pane.paint.borrow();
            let line = paint.line(0).expect("the row is held");
            assert_eq!(
                (
                    line.name.as_str(),
                    line.location.as_str(),
                    line.deleted.as_str()
                ),
                ("old-draft.txt", "/fx/v/work", "2025-06-14 09:30"),
                "trash board: name, Original Location and Deleted cells"
            );
            assert!(
                line.marked && paint.cursor == 0 && paint.edge,
                "the cursor lands marked and draws the rung with its edge"
            );
            assert_eq!(
                flea::ui_logic_pl3::words::strip_count(
                    &pane.catalog,
                    pane.view().total,
                    pane.view().bytes
                ),
                "1 item · 13 B",
                "the strip count"
            );
            assert_eq!(
                pane.empty_line(),
                None,
                "a listing Trash draws no empty line"
            );
        })
        .unwrap();
    assert!(
        calls.borrow().contains(&Call::Count(1)),
        "the rail hears the count"
    );
    let (window, _) = opened(&mut cx, &[]);
    window
        .update(&mut cx, |pane, _, _| {
            assert_eq!(pane.empty_line(), Some(("Trash is empty".into(), false)))
        })
        .unwrap();
}

#[test]
fn empty_trash_button_has_every_state() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("old-draft.txt", None)]);
    let ink = ink();
    window
        .update(&mut cx, |pane, _, cx| {
            let rest = pane.button_look();
            assert_eq!(
                (rest.frame, rest.wash, rest.label, rest.opacity),
                (ink.muted_frame, None, ink.foreground, 1.),
                "trash: rest"
            );
            pane.button = Button {
                hover: true,
                pressed: false,
            };
            assert_eq!(
                pane.button_look().wash,
                Some(ink.foreground.opacity(HOVER)),
                "trash-strip-hover: the 8% wash, label still foreground"
            );
            pane.button = Button {
                hover: true,
                pressed: true,
            };
            assert_eq!(
                pane.button_look().wash,
                Some(ink.foreground.opacity(PRESS)),
                "pressed: Buttons.js 14%"
            );
            pane.button = Button::default();
            pane.focus_strip(true, cx);
            let focus = pane.button_look();
            assert_eq!(
                (focus.frame, focus.wash),
                (ink.foreground, None),
                "trash-strip-focus: the foreground ring, no wash"
            );
            assert!(
                !pane.paint.borrow().edge && pane.paint.borrow().line(0).unwrap().marked,
                "the row keeps its plate and drops the edge"
            );
            pane.action("activate", false, cx);
        })
        .unwrap();
    assert!(
        sent(&calls)
            .last()
            .unwrap()
            .contains(r#""emptyTrash":true"#),
        "Return on the focused button reviews Empty Trash"
    );
    answer(
        &mut cx,
        window,
        &calls,
        "prepare",
        r#","token":4,"all":true,"count":1,"bytes":13,"partial":false"#,
    );
    let card = calls
        .borrow()
        .iter()
        .rev()
        .find_map(|call| match call {
            Call::Card(Some(card)) => Some(card.clone()),
            _ => None,
        })
        .expect("op1 gets one card");
    assert_eq!(
        (
            card.title.as_str(),
            card.body.as_str(),
            card.confirm.as_str(),
            card.cancel.as_str(),
            card.token
        ),
        (
            "Empty Trash?",
            "1 item, 13 B. This deletes it from disk. This cannot be undone.",
            "Empty Trash",
            "Cancel",
            4
        )
    );
    window
        .update(&mut cx, |pane, _, cx| {
            pane.card_answered(Some(4), cx);
            let busy = pane.button_look();
            assert_eq!(
                (busy.opacity, busy.label, busy.frame),
                (0.55, ink.foreground, ink.muted_frame),
                "trash-emptying: dimmed in place, label foreground"
            );
            assert_eq!(
                flea::ui_logic_pl3::words::strip_count(
                    &pane.catalog,
                    pane.view().total,
                    pane.view().bytes
                ),
                "1 item · 13 B",
                "the strip keeps its count while it runs"
            );
            pane.empty_pressed(cx);
        })
        .unwrap();
    assert!(
        sent(&calls).last().unwrap().contains(r#""op":"delete""#),
        "a press while emptying sends nothing more"
    );
    assert_eq!(
        calls
            .borrow()
            .iter()
            .filter(|call| matches!(call, Call::Card(Some(_))))
            .count(),
        1,
        "no second confirmation"
    );
    answer(
        &mut cx,
        window,
        &calls,
        "delete",
        r#","done":1,"failed":0,"failures":[]"#,
    );
    answer(&mut cx, window, &calls, "list", &listed(&[]));
    window
        .update(&mut cx, |pane, _, _| {
            assert_eq!(
                pane.button_look().opacity,
                0.55,
                "an empty Trash keeps Empty Trash… dimmed in place"
            );
            assert!(!pane.empty_available());
        })
        .unwrap();
    assert!(calls
        .borrow()
        .iter()
        .any(|call| matches!(call, Call::Result(text, false) if text == "Deleted 1 of 1 item")));
}

#[test]
fn trash_menu_opens_three_under_the_row_and_drops_the_edge() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("old-draft.txt", None)]);
    window
        .update(&mut cx, |pane, _, cx| pane.action("menu", false, cx))
        .unwrap();
    assert_eq!(
        calls.borrow().last(),
        Some(&Call::Menu(
            vec!["restoreTrashSelection"],
            At { x: 14., y: 88. }
        )),
        "trash-menu: Restore alone at 14, 88 in the pane, Delete permanently hidden by default"
    );
    window
        .update(&mut cx, |pane, _, cx| {
            assert!(
                !pane.paint.borrow().edge,
                "the row keeps its plate and drops the edge while its menu holds the keyboard"
            );
            pane.menu_action(Some("restoreTrashSelection"), cx);
            assert!(
                pane.paint.borrow().edge,
                "the edge returns with the keyboard"
            );
        })
        .unwrap();
    assert!(sent(&calls).last().unwrap().contains(r#""op":"restore""#));
    window
        .update(&mut cx, |pane, _, cx| {
            pane.right_click(None, 300., 400., cx)
        })
        .unwrap();
    assert!(
        matches!(calls.borrow().last(), Some(Call::Menu(ids, At { x, y })) if ids.len() == 1 && *x == 300. && *y == 400.),
        "a right press on the ground opens the marked rows' menu where it was"
    );
    let (window, calls) = opened(&mut cx, &[]);
    window
        .update(&mut cx, |pane, _, cx| pane.action("menu", false, cx))
        .unwrap();
    assert!(
        matches!(calls.borrow().last(), Some(Call::Menu(ids, _)) if *ids == ["open", "restoreAll", "emptyTrash"]),
        "rail-menu-trash rows on an empty Trash"
    );
}

#[test]
fn trash_pane_keys_route_and_back_closes() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("a.txt", None), row("b.txt", None)]);
    window
        .update(&mut cx, |pane, _, cx| {
            pane.action("settings", false, cx);
            pane.action("search", false, cx);
            pane.action("focusNext", false, cx);
            pane.action("trashArm", false, cx);
            pane.action("back", false, cx);
        })
        .unwrap();
    let calls = calls.borrow();
    assert!(
        calls.contains(&Call::Action("settings".into()))
            && !calls.contains(&Call::Action("search".into())),
        "window keys forward, listing keys stay"
    );
    assert!(calls.contains(&Call::Rail), "Tab hops to the rail");
    assert!(
        calls.contains(&Call::Status(
            "Press d again to review permanent deletion, or Delete on its own.".into(),
            false
        )),
        "the view's own dd prompt"
    );
    assert!(
        calls.contains(&Call::Back) && sent_of(&calls).last().unwrap().contains(r#""op":"cancel""#),
        "back closes and cancels the selection"
    );
}

fn sent_of(calls: &[Call]) -> Vec<String> {
    calls
        .iter()
        .filter_map(|call| match call {
            Call::Send(line) => Some(line.clone()),
            _ => None,
        })
        .collect()
}

#[path = "thumbnail_tests.rs"]
mod thumbnail_tests;
