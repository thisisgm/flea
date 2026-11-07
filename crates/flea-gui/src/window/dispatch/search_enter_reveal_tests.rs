//! GM's recorded nested FLAC result must reveal without opening an application or a preview.
use super::{
    search_open_tests::answer,
    test_shell::{asked, land, open_in, row, sent},
};
use crate::{columns::geometry::Slot, state::Shell, window::startup::View};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::cell::RefCell;

thread_local! {
    static OPENED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

fn record(argv: &[String]) -> crate::g2::op3::lane::Exit {
    OPENED.with(|opened| opened.borrow_mut().push(argv.to_vec()));
    (Some(0), String::new())
}

fn ready<'a>(cx: &'a mut TestAppContext, view: &str) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = open_in(cx, "default", view);
    land(&shell, cx, "/files", vec![row("Music", true)], 1);
    OPENED.with(|opened| opened.borrow_mut().clear());
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.opener[0].launch = record;
            window.focus(&shell.focus, cx);
        })
    });
    (shell, cx)
}

fn no_open(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    cx.run_until_parked();
    OPENED.with(|opened| assert!(opened.borrow().is_empty(), "Enter sent an open request"));
    shell.read_with(cx, |shell, cx| {
        assert!(
            !shell.opener[0].waiting(),
            "Enter queued an open request against the backend"
        );
        assert!(!shell.quick_look_open(cx), "Enter created a preview card");
        assert!(shell.overlay.is_none(), "Enter created an opener card");
    });
}

fn landed(shell: &Entity<Shell>, cx: &mut VisualTestContext, path: &str, listing: usize) {
    let rows = (0..80)
        .map(|at| row(&format!("track-{at:02}.flac"), false))
        .collect();
    land(shell, cx, path, rows, listing);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 60);
        assert_eq!(host.pane.held.row(60).unwrap().name, "track-60.flac");
        assert!(
            !host.search.owner.active(),
            "search is closed after landing"
        );
        assert!(!host.filter.owner.up(), "filter is closed after landing");
        let visible = match shell.view {
            View::Grid => host
                .grid
                .as_ref()
                .unwrap()
                .state
                .borrow()
                .drawn_row(60)
                .is_some(),
            View::Columns => host
                .columns
                .as_ref()
                .unwrap()
                .painters
                .get(Slot::Active)
                .unwrap()
                .borrow()
                .visible()
                .any(|at| at == 60),
            _ => host
                .list
                .borrow()
                .visible()
                .iter()
                .any(|row| row.index == 60),
        };
        assert!(visible, "the selected file is painted in {:?}", shell.view);
    });
    no_open(shell, cx);
}

fn search(cx: &mut TestAppContext, view: &str) {
    let (shell, cx) = ready(cx, view);
    cx.simulate_keystrokes("f s");
    let mut track = row("Music/Album/track-60.flac", false);
    track.icon = "audio-x-generic".into();
    answer(&shell, cx, 1, vec![row("Music/Album", true), track]);
    cx.simulate_keystrokes("end");
    shell.read_with(cx, |shell, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        assert_eq!(pane.cursor, 1, "Enter must target the file result");
        assert_eq!(
            pane.held.row(pane.cursor).unwrap().name,
            "Music/Album/track-60.flac"
        );
    });
    sent();
    cx.simulate_keystrokes("enter");
    no_open(&shell, cx);
    assert_eq!(asked(&shell, cx), "/files/Music/Album");
    assert!(sent().iter().any(|line| line.contains("\"searchclose\"")));
    landed(&shell, cx, "/files/Music/Album", 30);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn filter(cx: &mut TestAppContext, view: &str, multiple: bool) {
    let (shell, cx) = ready(cx, view);
    let mut track = row("track-60.flac", false);
    track.icon = "audio-x-generic".into();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell
                .step(
                    0,
                    crate::window::navigate::Step::Open("/files/Music/Album".into()),
                    window,
                    cx,
                )
                .unwrap();
        })
    });
    land(
        &shell,
        cx,
        "/files/Music/Album",
        vec![row("Other", true), track],
        2,
    );
    cx.simulate_keystrokes("/ t");
    cx.update(|window, cx| shell.update(cx, |shell, cx| {
        let host = shell.panes[0].as_ref().unwrap();
        let asked = host.filter.owner.asked().unwrap();
        assert_eq!(asked.query, "t", "fixture types the query through real keys");
        let id = asked.id;
        let listing = host.pane.held.listing;
        let rows = if multiple { "[0,1]" } else { "[1]" };
        let line = format!(r#"{{"t":"filtered","id":{id},"query":"t","n":2,"rows":{rows},"ms":1,"listing":{listing}}}"#);
        shell.receive(0, crate::backend::Backend::test_reply_line(&line), window, cx);
    }));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.filter.owner.drawn(host.pane.held.listing).is_some(),
            "the filter answer landed"
        );
        assert!(
            host.filter.owner.asked().is_none(),
            "no filter reply is outstanding"
        );
    });
    if multiple {
        cx.simulate_keystrokes("end");
    }
    shell.read_with(cx, |shell, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        assert_eq!(pane.held.row(pane.cursor).unwrap().name, "track-60.flac");
    });
    cx.simulate_keystrokes("enter");
    no_open(&shell, cx);
    assert_eq!(asked(&shell, cx), "/files/Music/Album");
    shell.read_with(cx, |shell, _| {
        assert!(!shell.panes[0].as_ref().unwrap().filter.owner.up())
    });
    landed(&shell, cx, "/files/Music/Album", 30);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn search_enter_reveal_list_audio(cx: &mut TestAppContext) {
    search(cx, "list");
}
#[gpui::test]
fn search_enter_reveal_columns_audio(cx: &mut TestAppContext) {
    search(cx, "columns");
}
#[gpui::test]
fn search_enter_reveal_grid_audio(cx: &mut TestAppContext) {
    search(cx, "grid");
}
#[gpui::test]
fn search_enter_reveal_filter_list(cx: &mut TestAppContext) {
    filter(cx, "list", false);
}
#[gpui::test]
fn search_enter_reveal_filter_columns(cx: &mut TestAppContext) {
    filter(cx, "columns", false);
}
#[gpui::test]
fn search_enter_reveal_filter_grid(cx: &mut TestAppContext) {
    filter(cx, "grid", false);
}

#[gpui::test]
fn search_enter_reveal_standing_filter_list(cx: &mut TestAppContext) {
    filter(cx, "list", true);
}
#[gpui::test]
fn search_enter_reveal_standing_filter_columns(cx: &mut TestAppContext) {
    filter(cx, "columns", true);
}
#[gpui::test]
fn search_enter_reveal_standing_filter_grid(cx: &mut TestAppContext) {
    filter(cx, "grid", true);
}

#[gpui::test]
fn search_enter_reveal_standing_filter_keeps_rail_enter(cx: &mut TestAppContext) {
    let (shell, cx) = ready(cx, "list");
    cx.simulate_keystrokes("/ t enter tab");
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.read_with(cx, |shell, cx| {
            assert!(shell.panes[0].as_ref().unwrap().filter.owner.up());
            assert!(!shell.panes[0].as_ref().unwrap().filter.owner.typing());
            assert!(shell.dual_focus.rail.contains_focused(window, cx));
            assert_eq!(shell.keyboard.context, flea::keys::Context::Rail);
        });
    });
    cx.simulate_keystrokes("home enter");
    assert_eq!(asked(&shell, cx), std::env::var("HOME").unwrap());
    no_open(&shell, cx);
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
