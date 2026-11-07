//! Recent's Escape and reveal stand behind a filter or a search that owns the key: the owner answers first.
use super::{
    railrecent_tests::{frame, read, rows},
    search_open_tests::line,
    test_shell::{asked, land, open_root, row, sent},
    Order,
};
use crate::{backend::Event as BackendEvent, state::Shell, window::input::ActionEvent};
use flea::{
    keys::Context as Keys,
    protocol::{Reply, RowFacts},
    ui_logic_pl4::order::recent_entry,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

// The filter line's and the search's open state on the focused pane.
fn owners(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (bool, bool) {
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.filter.owner.up(), host.search.owner.active())
    })
}

// Recent open over /files with its history listing landed.
fn open_recent(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    land(shell, cx, "/files", vec![row("A", true)], 1);
    rows(shell, cx, vec![recent_entry()]);
    cx.simulate_keystrokes("tab enter");
    frame(cx);
    land(
        shell,
        cx,
        "/",
        vec![row("tmp/alpha.txt", false), row("tmp/beta.txt", false)],
        2,
    );
}

// One search generation over the folder the pane stands on: its listed line, its rows and the searched line that ends the walk.
fn answer(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    folder: &str,
    generation: usize,
    rows: Vec<RowFacts>,
) {
    let total = rows.len();
    line(
        shell,
        cx,
        &format!(r#"{{"t":"listed","path":"{folder}","n":{total},"gen":{generation}}}"#),
    );
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let reply = Reply::Rows {
                start: 0,
                listing: 10 + generation,
                rows,
            };
            shell.receive(0, BackendEvent::Session { generation, reply }, window, cx)
        })
    });
    line(
        shell,
        cx,
        &format!(
            r#"{{"t":"searched","n":{total},"scanned":9,"ms":1,"cancelled":false,"gen":{generation},"walking":false}}"#
        ),
    );
    cx.run_until_parked();
}

// The listing family's key at the dispatch point, which the key path reaches once the filter and search owners decline it.
fn dispatch(shell: &Entity<Shell>, cx: &mut VisualTestContext, action: &'static str) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.dispatch(
                Order::Action(ActionEvent {
                    action,
                    context: Keys::Listing,
                    seat: Default::default(),
                    pair_armed: false,
                }),
                window,
                cx,
            )
        })
    });
    frame(cx);
}

#[gpui::test]
fn railrecent_escape_clears_a_typing_filter_and_keeps_recent(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    open_recent(&shell, cx);
    cx.simulate_keystrokes("/ a");
    frame(cx);
    assert_eq!(
        owners(&shell, cx),
        (true, false),
        "the filter line stands over Recent"
    );
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert_eq!(
        owners(&shell, cx),
        (false, false),
        "Escape clears the filter"
    );
    assert_eq!(
        read(&shell, cx, "recentMode"),
        "results",
        "Escape leaves Recent standing"
    );
    assert_eq!(read(&shell, cx, "recentFrom"), "/files");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_escape_clears_a_committed_filter_and_keeps_recent(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    open_recent(&shell, cx);
    cx.simulate_keystrokes("/ a enter");
    frame(cx);
    assert_eq!(
        owners(&shell, cx),
        (true, false),
        "the committed filter stands over Recent"
    );
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert_eq!(
        owners(&shell, cx),
        (false, false),
        "Escape clears the committed filter"
    );
    assert_eq!(
        read(&shell, cx, "recentMode"),
        "results",
        "Escape leaves Recent standing"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_escape_ends_a_standing_search_and_keeps_recent(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    open_recent(&shell, cx);
    cx.simulate_keystrokes("f s");
    answer(&shell, cx, "/", 1, vec![row("notes.txt", false)]);
    frame(cx);
    assert_eq!(
        owners(&shell, cx),
        (false, true),
        "the search stands over Recent"
    );
    sent();
    cx.simulate_keystrokes("escape");
    frame(cx);
    let exit = sent();
    assert!(
        exit.iter().any(|line| line.contains("\"c\":\"listpaths\"")),
        "the search's exit lists the history again: {exit:?}"
    );
    assert!(
        !exit.iter().any(|line| line.contains("\"c\":\"list\"")),
        "the exit never lists the folder under Recent: {exit:?}"
    );
    assert_eq!(
        read(&shell, cx, "recentMode"),
        "results",
        "Escape leaves Recent standing"
    );
    assert_eq!(read(&shell, cx, "recentFrom"), "/files");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_o_on_a_search_result_reveals_that_result_and_leaves_recent(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    open_recent(&shell, cx);
    cx.simulate_keystrokes("f s");
    answer(&shell, cx, "/", 1, vec![row("tmp/alpha.txt", false)]);
    // The committed results a tag place opens; Enter on the line now goes to its row.
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.search_update(
                0,
                None,
                |search, host| search.key(crate::g2::sr1::StripKey::Enter, host),
                window,
                cx,
            )
        })
    });
    frame(cx);
    assert_eq!(
        owners(&shell, cx),
        (false, true),
        "the committed results hold the keys"
    );
    sent();
    cx.simulate_keystrokes("o");
    frame(cx);
    let reveal = sent();
    assert!(
        reveal
            .iter()
            .any(|line| line.contains("\"c\":\"list\",\"path\":\"/tmp\"")),
        "the result's own folder is listed: {reveal:?}"
    );
    assert!(
        !reveal
            .iter()
            .any(|line| line.contains("\"c\":\"listpaths\"")),
        "{reveal:?}"
    );
    assert_eq!(
        read(&shell, cx, "recentMode"),
        "",
        "reveal leaves Recent for the folder"
    );
    assert_eq!(asked(&shell, cx), "/tmp", "the result's own folder opens");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_dispatched_escape_yields_to_a_standing_filter(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    open_recent(&shell, cx);
    cx.simulate_keystrokes("/ a enter");
    frame(cx);
    assert_eq!(
        owners(&shell, cx),
        (true, false),
        "the committed filter stands over Recent"
    );
    dispatch(&shell, cx, "escape");
    assert_eq!(
        owners(&shell, cx),
        (true, false),
        "the filter owns Escape, Recent does not climb"
    );
    assert_eq!(
        read(&shell, cx, "recentMode"),
        "results",
        "Recent stays open"
    );
    assert_eq!(read(&shell, cx, "recentFrom"), "/files");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_dispatched_escape_yields_to_a_standing_search(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    open_recent(&shell, cx);
    cx.simulate_keystrokes("f s");
    answer(&shell, cx, "/", 1, vec![row("notes.txt", false)]);
    cx.simulate_keystrokes("enter");
    frame(cx);
    assert_eq!(
        owners(&shell, cx),
        (false, true),
        "the search stands over Recent"
    );
    dispatch(&shell, cx, "escape");
    assert_eq!(
        owners(&shell, cx),
        (false, true),
        "the search owns Escape, Recent does not leave"
    );
    assert_eq!(
        read(&shell, cx, "recentMode"),
        "results",
        "Recent stays open"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_dispatched_reveal_does_not_join_a_name_onto_the_folder_under_a_search(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root(cx, "default");
    open_recent(&shell, cx);
    cx.simulate_keystrokes("f s");
    answer(&shell, cx, "/", 1, vec![row("notes.txt", false)]);
    cx.simulate_keystrokes("enter");
    frame(cx);
    dispatch(&shell, cx, "reveal");
    assert_eq!(
        read(&shell, cx, "recentMode"),
        "results",
        "Recent's reveal does not run under a search"
    );
    shell.read_with(cx, |shell, _| {
        let pending = shell.panes[0].as_ref().unwrap().nav.pending_select.clone();
        assert_ne!(
            pending.as_deref(),
            Some("/notes.txt"),
            "no name joined onto the folder"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
