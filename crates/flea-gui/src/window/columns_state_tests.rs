use super::host;
use crate::backend::Event;
use flea::protocol::Reply;
use gpui::VisualTestContext;

#[test]
fn u09_live_columns_empty_hero_uses_the_whole_pane_or_clear_free_area() {
    for (width, count) in [(800., 2), (1400., 3), (2100., 4), (2700., 5)] {
        let mut cx = host::context();
        let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", width, count);
        host::change(&mut cx, window, |shell, window, cx| {
            shell.receive(
                0,
                Event::Reply(Reply::Listed {
                    path: "/srv/a/b/c".into(),
                    total: 0,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                Event::Reply(Reply::Rows {
                    start: 0,
                    listing: 4,
                    rows: Vec::new(),
                }),
                window,
                cx,
            );
            host::settle(shell, window, cx);
        });
        host::frame(&mut cx, window);
        let (pane, active) = window
            .update(&mut cx, |shell, window, _| {
                (
                    shell.geometry(window).panes[0],
                    shell.panes[0]
                        .as_ref()
                        .unwrap()
                        .columns
                        .as_ref()
                        .unwrap()
                        .area
                        .layout
                        .active_x(),
                )
            })
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), &cx);
        let slot = visual
            .debug_bounds("columns-state-slot")
            .expect("the actual state producer mounted");
        let mark = visual
            .debug_bounds("fb1-hero-mark")
            .expect("the hero mark painted");
        let start = if count <= 3 { 0. } else { active };
        assert_eq!(f32::from(slot.origin.x), pane.x + start);
        assert_eq!(f32::from(slot.size.width), pane.width - start);
        assert!((f32::from(mark.center().x - slot.center().x)).abs() < 1.);
        if count == 3 {
            assert!(
                f32::from(slot.size.width) > pane.width / 2.,
                "the hero was confined to the active column"
            );
        }
    }
}

#[test]
fn a_new_columns_limit_applies_to_the_open_columns_pane_at_once() {
    let mut cx = host::context();
    let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 2700., 5);
    host::change(&mut cx, window, host::settle);
    host::frame(&mut cx, window);
    for limit in [2, 4, 3] {
        host::change(&mut cx, window, |shell, window, cx| {
            let patch = flea::jsondoc::parse(&format!(r#"{{"columnsLimit":{limit}}}"#)).unwrap();
            shell.change_settings(patch, window, cx);
            host::settle(shell, window, cx);
        });
        host::frame(&mut cx, window);
        host::change(&mut cx, window, |shell, window, cx| {
            assert_eq!(
                host::call(shell, "columnCount", &[], window, cx)
                    .stdout
                    .trim(),
                limit.to_string(),
                "Settings set the limit to {limit}"
            );
        });
    }
}

#[test]
fn m9_columns_limit_changed_in_settings_relays_the_drawn_columns_live() {
    let mut cx = host::context();
    let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 2700., 5);
    host::frame(&mut cx, window);
    let count = |cx: &mut gpui::TestAppContext| {
        window
            .update(cx, |shell, _, _| {
                let area = &shell.panes[0]
                    .as_ref()
                    .unwrap()
                    .columns
                    .as_ref()
                    .unwrap()
                    .area;
                (area.limit, area.count())
            })
            .unwrap()
    };
    assert_eq!(count(&mut cx), (Some(5.), 5));
    for (limit, expected) in [(2, 2), (4, 4)] {
        host::change(&mut cx, window, |shell, window, cx| {
            let patch = flea::jsondoc::parse(&format!(r#"{{"columnsLimit":{limit}}}"#)).unwrap();
            shell.change_settings(patch, window, cx);
        });
        host::frame(&mut cx, window);
        assert_eq!(
            count(&mut cx),
            (Some(limit as f64), expected),
            "the Settings limit re-lays the open Columns view without a relaunch"
        );
    }
}

#[test]
fn m9_columns_rows_wear_language_marks_while_lang_marks_is_on() {
    use crate::{columns::geometry::Slot, row::marks::Mark};
    let mut cx = host::context();
    let (window, _requests) = host::open(&mut cx, "/srv/a/b/c");
    host::change(&mut cx, window, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[("c", true)], false, window, cx);
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("main.rs", false)],
            false,
            window,
            cx,
        );
    });
    for (patch, language) in [(None, true), (Some(false), false), (Some(true), true)] {
        if let Some(on) = patch {
            host::change(&mut cx, window, |shell, window, cx| {
                let patch = flea::jsondoc::parse(&format!(r#"{{"langMarks":{on}}}"#)).unwrap();
                shell.change_settings(patch, window, cx);
            });
        }
        host::frame(&mut cx, window);
        let mark = window
            .update(&mut cx, |shell, _, _| {
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                let painter = columns.painters.get(Slot::Child).unwrap().borrow();
                painter.prepared(0).unwrap().mark
            })
            .unwrap();
        assert_eq!(
            matches!(mark, Mark::Language(_)),
            language,
            "langMarks {patch:?} drew {mark:?} for main.rs"
        );
    }
}

// EY-373: cols4-hero-empty.png draws four 592 px columns at 2560x1400 with the hero on the free area right of them;
// the default three-column limit keeps cols-insideempty.png's whole-pane centre, which also clears the columns there.
#[test]
fn ey373_columns_empty_hero_at_2560_follows_the_board_for_its_column_count() {
    for count in [3, 4] {
        let mut cx = host::context();
        let (window, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 2560., count);
        host::change(&mut cx, window, |shell, window, cx| {
            shell.receive(
                0,
                Event::Reply(Reply::Listed {
                    path: "/srv/a/b/c".into(),
                    total: 0,
                }),
                window,
                cx,
            );
            shell.receive(
                0,
                Event::Reply(Reply::Rows {
                    start: 0,
                    listing: 4,
                    rows: Vec::new(),
                }),
                window,
                cx,
            );
            host::settle(shell, window, cx);
        });
        host::frame(&mut cx, window);
        let (pane, layout) = window
            .update(&mut cx, |shell, window, _| {
                let host = shell.panes[0].as_ref().unwrap();
                (
                    shell.geometry(window).panes[0],
                    host.columns.as_ref().unwrap().area.layout.clone(),
                )
            })
            .unwrap();
        assert_eq!(layout.count, count);
        let mut visual = VisualTestContext::from_window(window.into(), &cx);
        let slot = visual
            .debug_bounds("columns-state-slot")
            .expect("the state producer mounted");
        let mark = visual
            .debug_bounds("fb1-hero-mark")
            .expect("the hero mark painted");
        let start = if count == 4 {
            assert_eq!(
                layout.column_width, 592.,
                "the board's columns are 592 wide"
            );
            layout.active_x()
        } else {
            0.
        };
        assert_eq!(f32::from(slot.origin.x), pane.x + start);
        assert_eq!(f32::from(slot.size.width), pane.width - start);
        assert!((f32::from(mark.center().x - slot.center().x)).abs() < 1.);
        assert!(
            f32::from(mark.origin.x) > pane.x + layout.active_x(),
            "the hero stands clear of every drawn column"
        );
    }
}
