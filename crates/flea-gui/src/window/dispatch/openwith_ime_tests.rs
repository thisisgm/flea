//! The Open With search composes through the window's shared entry: a composition shows only in the field, the query and list follow the commit, and it never launches.
use super::{answer, deliver, opened, quit, sent};
use crate::{
    g2::{
        op1::dialog_editor::Slot,
        op3::{
            openwith::Stop,
            shell::{AppRow, Block},
        },
    },
    state::Shell,
};
use gpui::{Context, Entity, EntityInputHandler, TestAppContext, VisualTestContext, Window};

fn input<R>(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    run: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    let out = cx.update(|window, cx| shell.update(cx, |shell, cx| run(shell, window, cx)));
    cx.run_until_parked();
    out
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn mark(shell: &Entity<Shell>, cx: &mut VisualTestContext, text: &str) {
    let end = text.encode_utf16().count();
    input(shell, cx, |s, w, cx| {
        s.replace_and_mark_text_in_range(None, text, Some(end..end), w, cx)
    });
    draw(cx);
}

// The search line as the card holds it, and the application labels its list shows.
fn listed(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (String, Vec<String>) {
    let catalog = crate::g2::op3::tests::english();
    shell.read_with(cx, |shell, _| {
        let open = shell.open_with.as_ref().expect("an open card");
        let labels = open
            .card
            .card(&catalog)
            .body
            .iter()
            .find_map(|block| match block {
                Block::Apps(rows, _) => Some(
                    rows.iter()
                        .filter_map(|row| match row {
                            AppRow::App { label, .. } => Some(label.clone()),
                            AppRow::Eyebrow { .. } | AppRow::Empty { .. } => None,
                        })
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        (open.card.query.clone(), labels)
    })
}

fn both() -> Vec<String> {
    vec!["Neovim".into(), "btop++".into()]
}

// The card with its applications listed and the search painted, every earlier request read.
fn ready(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, u64) {
    let (shell, cx, id) = opened(cx);
    answer(&shell, cx, id);
    sent();
    draw(cx);
    (shell, cx, id)
}

// Sample input: {"c":"menuaction","op":"snapshot","id":2}; the replacement holds it before its catalogue.
fn catalogue(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let snapshot = sent()
        .iter()
        .filter_map(|line| flea::jsondoc::parse(line).ok())
        .find(|doc| doc.get("op").and_then(flea::jsondoc::Json::as_str) == Some("snapshot"))
        .and_then(|doc| doc.get("id").and_then(flea::jsondoc::Json::as_f64))
        .expect("the replacement card snapshots its row") as u64;
    assert_eq!(
        snapshot,
        shell.read_with(cx, |s, _| s.open_with.as_ref().unwrap().id)
    );
    deliver(
        shell,
        cx,
        &format!(
            r#"{{"t":"menuaction","id":{snapshot},"op":"snapshot","ok":true,"kinds":[33188]}}"#
        ),
    );
    assert_eq!(
        sent(),
        [format!(
            r#"{{"c":"menuaction","op":"applications","id":{snapshot},"installed":true}}"#
        )],
        "the replacement asks for its catalogue after holding its snapshot"
    );
    answer(shell, cx, snapshot);
}

#[gpui::test]
fn i50_openwith_catalogue_refresh_keeps_preedit_unfiltered(cx: &mut TestAppContext) {
    let (shell, cx, id) = ready(cx);
    assert!(cx.debug_bounds("op3-openwith-scrim").is_some());
    mark(&shell, cx, "bt");
    answer(&shell, cx, id);
    assert_eq!(listed(&shell, cx), (String::new(), both()));
    input(&shell, cx, |s, w, cx| {
        assert_eq!(s.marked_text_range(w, cx), Some(0..2));
        s.replace_text_in_range(None, "bt", w, cx);
    });
    assert_eq!(listed(&shell, cx), ("bt".into(), vec!["btop++".into()]));
    quit(&shell, cx);
}

#[gpui::test]
fn i50_openwith_unmark_commits_and_filters_the_query(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready(cx);
    mark(&shell, cx, "bt");
    assert_eq!(listed(&shell, cx), (String::new(), both()));
    input(&shell, cx, |s, w, cx| s.unmark_text(w, cx));
    assert_eq!(listed(&shell, cx), ("bt".into(), vec!["btop++".into()]));
    input(&shell, cx, |s, w, cx| {
        assert!(s.marked_text_range(w, cx).is_none())
    });
    quit(&shell, cx);
}

#[gpui::test]
fn i50_openwith_search_composition_filters_on_commit_and_never_launches(cx: &mut TestAppContext) {
    let (shell, cx, id) = ready(cx);
    let mounted = shell.read_with(cx, |s, _| {
        (
            s.rename.ime.owner,
            s.rename.ime.drawn.borrow().line.registered.is_some(),
        )
    });
    assert_eq!(
        mounted,
        (Some(Slot::OpenWithSearch), true),
        "the search registers the platform input"
    );
    mark(&shell, cx, "bt");
    assert_eq!(
        listed(&shell, cx),
        (String::new(), both()),
        "a composition shows in the field but does not filter"
    );
    input(&shell, cx, |s, w, cx| {
        assert_eq!(s.marked_text_range(w, cx), Some(0..2));
        let field = s.rename.ime.drawn.borrow().bounds.expect("search painted");
        let caret = s.bounds_for_range(2..2, field, w, cx).unwrap();
        assert!(caret.left() > field.left() && caret.left() <= field.right());
        assert_eq!(caret.top(), field.top());
    });
    cx.simulate_keystrokes("enter");
    assert!(sent().is_empty(), "candidate Enter launches nothing");
    assert!(shell.read_with(cx, |s, _| s.open_with.is_some()));
    input(&shell, cx, |s, w, cx| {
        s.replace_text_in_range(None, "bt", w, cx)
    });
    assert_eq!(listed(&shell, cx), ("bt".into(), vec!["btop++".into()]));
    cx.simulate_keystrokes("enter");
    assert_eq!(
        sent(),
        [format!(
            r#"{{"c":"menuaction","op":"openWith","id":{id},"application":"btop.desktop","always":false}}"#
        )],
        "a committed search opens the application it leaves under the cursor"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn i50_openwith_search_escape_drops_the_composition_before_the_card(cx: &mut TestAppContext) {
    let (shell, cx, id) = ready(cx);
    cx.simulate_input("n");
    mark(&shell, cx, "ね");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        listed(&shell, cx).0,
        "n",
        "first Escape drops only the composition"
    );
    assert!(sent().is_empty());
    cx.simulate_keystrokes("escape");
    assert!(shell.read_with(cx, |s, _| s.open_with.is_none()));
    assert_eq!(
        sent(),
        [format!(r#"{{"c":"menuaction","op":"close","id":{id}}}"#)]
    );
    input(&shell, cx, |s, w, cx| {
        s.replace_text_in_range(None, "late", w, cx);
        assert!(s.selected_text_range(false, w, cx).is_none());
        assert!(s.rename.ime.entry.session().is_none());
    });
    quit(&shell, cx);
}

#[gpui::test]
fn i50_openwith_search_typed_text_and_caret_keys_stay_in_the_field(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready(cx);
    cx.simulate_input("bxt");
    assert_eq!(listed(&shell, cx), ("bxt".into(), Vec::new()));
    cx.simulate_keystrokes("left backspace");
    assert_eq!(listed(&shell, cx), ("bt".into(), vec!["btop++".into()]));
    input(&shell, cx, |s, w, cx| {
        s.replace_text_in_range(None, "\u{1b}", w, cx);
        s.replace_and_mark_text_in_range(None, "\u{7}", None, w, cx);
    });
    assert_eq!(
        listed(&shell, cx).0,
        "bt",
        "control text from the input method never enters the search"
    );
    cx.simulate_keystrokes("home delete");
    assert_eq!(listed(&shell, cx), ("t".into(), vec!["btop++".into()]));
    cx.simulate_keystrokes("ctrl-a backspace");
    assert_eq!(listed(&shell, cx), (String::new(), both()));
    cx.simulate_keystrokes("down");
    let (focus, cursor) = shell.read_with(cx, |s, _| {
        let card = &s.open_with.as_ref().unwrap().card;
        (card.focus, card.cursor)
    });
    assert_eq!(
        (focus, cursor),
        (Stop::Search, 1),
        "Down still walks the list"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn i50_openwith_tab_commits_the_composition_and_filters_it(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready(cx);
    mark(&shell, cx, "bt");
    cx.simulate_keystrokes("tab");
    assert_eq!(listed(&shell, cx), ("bt".into(), vec!["btop++".into()]));
    assert_eq!(
        shell.read_with(cx, |s, _| s.open_with.as_ref().unwrap().card.focus),
        Stop::List
    );
    input(&shell, cx, |s, w, cx| {
        s.replace_text_in_range(None, "late", w, cx);
        assert!(s.marked_text_range(w, cx).is_none());
    });
    assert_eq!(
        listed(&shell, cx).0,
        "bt",
        "a late callback misses the blurred search"
    );
    quit(&shell, cx);
}

#[gpui::test]
fn i50_openwith_replaced_card_refuses_the_old_sessions_callback(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready(cx);
    mark(&shell, cx, "ね");
    let before = shell.read_with(cx, |s, _| s.rename.ime.entry.session());
    input(&shell, cx, |s, w, cx| {
        s.open_open_with(0, "/files/mystery.xyz".into(), cx);
        s.replace_text_in_range(None, "late", w, cx);
        s.replace_and_mark_text_in_range(None, "late", None, w, cx);
        assert!(s.selected_text_range(false, w, cx).is_none());
        assert_ne!(s.rename.ime.entry.session(), before);
    });
    assert_eq!(listed(&shell, cx).0, "");
    draw(cx);
    catalogue(&shell, cx);
    draw(cx);
    assert!(cx.debug_bounds("op3-openwith-scrim").is_some());
    cx.simulate_input("n");
    assert_eq!(
        listed(&shell, cx).0,
        "n",
        "the new card's search takes text"
    );
    quit(&shell, cx);
}
