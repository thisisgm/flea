//! Recent through the running window: the rail row's Return and click, the history listing, Back, refusals and readers.
use super::test_shell::*;
use crate::{ipc::server, navigation::folder_state::Order, state::Shell, window::navigate::Step};
use flea::{
    ipc::wire,
    ui_logic_pl4::{
        order::{Heading, RailSection},
        rail::RailEntry,
    },
};
use gpui::{point, px, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

const RECENT_REFUSAL: &str = "Recent cannot take a paste. Open a folder first";
const HISTORY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/g2/pl4/recent-fixture.xbel"
);

pub(super) fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

pub(super) fn read(shell: &Entity<Shell>, cx: &mut VisualTestContext, name: &str) -> String {
    let (answer, receiver) = std::sync::mpsc::sync_channel(1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.ipc(
                server::Event::Call {
                    request: wire::Request::Call {
                        target: "flea".into(),
                        function: name.into(),
                        arguments: vec![],
                    },
                    answer,
                },
                window,
                cx,
            )
        })
    });
    let reply = receiver.try_recv().unwrap().reply;
    assert_eq!(reply.code, 0, "{name}: {}", reply.stderr);
    reply.stdout.trim().to_owned()
}

// The path bar's own text, which reads Recent while the history stands.
fn bar(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| shell.path_labels[0].to_string())
}

pub(super) fn rows(shell: &Entity<Shell>, cx: &mut VisualTestContext, entries: Vec<RailEntry>) {
    shell.update(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .update(cx, |producer, cx| {
                producer.replace_sections(
                    vec![RailSection {
                        heading: Heading::Key("places.heading.places"),
                        rows: entries,
                    }],
                    cx,
                );
                producer.key_action("cursorFirst", cx);
            });
    });
    frame(cx);
}

fn click(shell: &Entity<Shell>, cx: &mut VisualTestContext, button: MouseButton) {
    let at = shell.read_with(cx, |shell, cx| {
        let laid = shell.rail.as_ref().unwrap().places.read(cx).laid.borrow();
        let row = laid.row(0).expect("the live rail painted its row");
        point(px(row.rect.x + 45.), px(row.rect.y + row.rect.height / 2.))
    });
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, button, Modifiers::none());
    frame(cx);
}

fn step(shell: &Entity<Shell>, cx: &mut VisualTestContext, step: Step) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let _ = shell.step(0, step, window, cx);
        })
    });
    frame(cx);
}

#[gpui::test]
fn railrecent_click_opens_history_and_live_readers(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("A", true)], 1);
    rows(&shell, cx, vec![flea::ui_logic_pl4::order::recent_entry()]);
    sent();
    click(&shell, cx, MouseButton::Left);
    assert_eq!(read(&shell, cx, "recentMode"), "results");
    assert_eq!(read(&shell, cx, "recentFrom"), "/files");
    assert_eq!(bar(&shell, cx), "Recent", "the path bar names the history");
    assert!(sent()
        .iter()
        .any(|line| line.contains("\"c\":\"listpaths\"")));
    land(&shell, cx, "/", vec![row("tmp/alpha.txt", false)], 2);
    assert_eq!(read(&shell, cx, "path"), "/");
    assert_eq!(read(&shell, cx, "total"), "1");
    assert_eq!(
        bar(&shell, cx),
        "Recent",
        "the landed history keeps its name"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_return_opens_history_and_back_restores_the_folder_and_its_sort(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("A", true)], 1);
    rows(&shell, cx, vec![flea::ui_logic_pl4::order::recent_entry()]);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().nav.order = Order {
            key: "size",
            desc: false,
        };
    });
    cx.simulate_keystrokes("tab enter");
    frame(cx);
    assert_eq!(read(&shell, cx, "recentMode"), "results");
    land(&shell, cx, "/", vec![], 2);
    cx.simulate_keystrokes("escape");
    frame(cx);
    assert_eq!(
        asked(&shell, cx),
        "/files",
        "Back returns to the folder Recent was opened from"
    );
    assert_eq!(read(&shell, cx, "recentMode"), "");
    assert_eq!(read(&shell, cx, "recentFrom"), "");
    land(&shell, cx, "/files", vec![row("A", true)], 3);
    assert_eq!(
        bar(&shell, cx),
        "/files",
        "the folder's own name returns once it lands"
    );
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().nav.order.key, "size");
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_listpaths_carries_the_history_newest_first_without_its_refused_links(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("A", true)], 1);
    sent();
    shell.update(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .update(cx, |producer, cx| {
                producer.read_recent(HISTORY.into(), cx);
            });
    });
    frame(cx);
    cx.run_until_parked();
    frame(cx);
    assert_eq!(read(&shell, cx, "recentMode"), "results");
    let listed = sent();
    assert!(
        listed
            .iter()
            .any(|line| line.contains("\"c\":\"listpaths\"")
                && line.contains("\"paths\":[\"/newer.txt\",\"/older.txt\"]")),
        "the history's file paths, newest first: {listed:?}"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_paste_verbs_are_refused_with_the_v0312_sentence(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("A", true)], 1);
    rows(&shell, cx, vec![flea::ui_logic_pl4::order::recent_entry()]);
    cx.simulate_keystrokes("tab enter");
    frame(cx);
    land(&shell, cx, "/", vec![row("tmp/alpha.txt", false)], 2);
    cx.simulate_keystrokes("ctrl-v");
    frame(cx);
    assert_eq!(read(&shell, cx, "lastMessage"), RECENT_REFUSAL, "ctrl-v");
    cx.simulate_keystrokes("shift-p");
    frame(cx);
    assert_eq!(
        read(&shell, cx, "contextMenuVisible"),
        "false",
        "P opens no paste menu"
    );
    assert_eq!(read(&shell, cx, "lastMessage"), RECENT_REFUSAL, "shift-p");
    assert_eq!(
        read(&shell, cx, "recentMode"),
        "results",
        "a refused paste leaves Recent standing"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_a_late_reply_after_navigating_away_is_dropped(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("A", true)], 1);
    rows(&shell, cx, vec![flea::ui_logic_pl4::order::recent_entry()]);
    shell.update(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .update(cx, |producer, cx| producer.activate(0, cx));
    });
    step(&shell, cx, Step::Open("/work".into()));
    cx.run_until_parked();
    frame(cx);
    assert_eq!(
        read(&shell, cx, "recentMode"),
        "",
        "the history never replaces /work"
    );
    assert_eq!(asked(&shell, cx), "/work");
    land(&shell, cx, "/work", vec![], 2);
    assert_eq!(bar(&shell, cx), "/work");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_enter_on_a_folder_row_leaves_recent_for_it(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("A", true)], 1);
    rows(&shell, cx, vec![flea::ui_logic_pl4::order::recent_entry()]);
    cx.simulate_keystrokes("tab enter");
    frame(cx);
    land(&shell, cx, "/", vec![row("tmp", true)], 2);
    cx.simulate_keystrokes("enter");
    frame(cx);
    assert_eq!(asked(&shell, cx), "/tmp");
    assert_eq!(read(&shell, cx, "recentMode"), "");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn railrecent_reveal_opens_the_rows_folder_with_it_selected(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("A", true)], 1);
    rows(&shell, cx, vec![flea::ui_logic_pl4::order::recent_entry()]);
    cx.simulate_keystrokes("tab enter");
    frame(cx);
    land(&shell, cx, "/", vec![row("tmp/alpha.txt", false)], 2);
    cx.simulate_keystrokes("o");
    frame(cx);
    assert_eq!(asked(&shell, cx), "/tmp");
    assert_eq!(read(&shell, cx, "recentMode"), "");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
