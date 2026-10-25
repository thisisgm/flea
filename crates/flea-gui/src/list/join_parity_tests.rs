//! v0.3.12 list parity: Dual drops Mode and Kind (Columns.dualSet), and Kind paints the reply's kind names.
use super::{frame, open};
use crate::{
    list::{bench, columns::Column, header, pane::Held},
    window::{pane::PaneHost, startup::View},
};

// Switches every optional column on, as a person who shows Mode and Kind in the List would.
fn every_column(cx: &mut gpui::TestAppContext, handle: gpui::WindowHandle<crate::state::Shell>) {
    handle
        .update(cx, |shell, _, _| {
            let patch = flea::jsondoc::parse(r#"{"columns":["name","mode","size","date","kind"]}"#)
                .unwrap();
            shell.launch.settings.data =
                flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
        })
        .unwrap();
}

#[test]
fn m9_dual_panes_never_draw_mode_or_kind() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    every_column(&mut cx, handle);
    frame(&mut cx, handle);
    let shown = |cx: &mut gpui::TestAppContext, side: usize| {
        handle
            .update(cx, |shell, _, _| {
                let list = shell.panes[side].as_ref().unwrap().list.borrow();
                [Column::Mode, Column::Kind, Column::Size, Column::Date]
                    .map(|column| list.drawn.layout.shown(column))
            })
            .unwrap()
    };
    assert_eq!(
        shown(&mut cx, 0),
        [true; 4],
        "the List shows every switched column"
    );
    handle
        .update(&mut cx, |shell, _, _| {
            let right = PaneHost::new("/right".into(), None, 620., &shell.locale);
            right.list.borrow_mut().typeface = bench::bench_typeface();
            shell.panes[1] = Some(right);
            shell.view = View::Dual;
        })
        .unwrap();
    handle
        .update(&mut cx, |shell, _, cx| {
            assert!(shell.list_header(0, cx).is_some());
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            assert!(
                !list.view.switched.on(Column::Mode) && !list.view.switched.on(Column::Kind),
                "the header must suppress Dual columns before the body renders"
            );
        })
        .unwrap();
    frame(&mut cx, handle);
    let [mode, kind, ..] = shown(&mut cx, 0);
    assert!(!mode && !kind, "a Dual pane drew Mode or Kind");
    handle
        .update(&mut cx, |shell, _, _| {
            let list = shell.panes[0].as_ref().unwrap().list.borrow();
            assert_eq!(list.header.drawn.layout, list.drawn.layout);
            assert!(!list.view.switched.on(Column::Mode) && !list.view.switched.on(Column::Kind));
        })
        .unwrap();
}

#[test]
fn m9_kind_cells_paint_and_autofit_the_replys_kind_names() {
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    every_column(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, _| {
            let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
            let mut rows = held.rows.clone();
            rows[0].kind = "Plain Text Document".into();
            *held = std::rc::Rc::new(Held {
                listing: held.listing,
                total: held.total,
                start: held.start,
                rows,
            });
        })
        .unwrap();
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let list = host.list.borrow();
            assert!(list.drawn.layout.shown(Column::Kind));
            let width = |index: usize| {
                list.visible()
                    .iter()
                    .find(|shown| shown.index == index)
                    .and_then(|shown| shown.prepared.as_ref())
                    .map(|prepared| f32::from(prepared.kind.line().width))
            };
            assert!(
                width(0).is_some_and(|width| width > 0.),
                "the Kind cell drew nothing for kinds[k]"
            );
            assert_eq!(
                width(1),
                Some(0.),
                "a row naming no kind keeps an empty cell"
            );
            assert_eq!(
                header::cell_text(Column::Kind, &host.pane.held, 0, &list.view).as_deref(),
                Some("Plain Text Document"),
                "autofit measures the Kind name"
            );
        })
        .unwrap();
}

#[test]
fn m9_a_held_press_holds_the_watched_reread_until_release() {
    let mut cx = bench::real_cx();
    let control = open(&mut cx);
    control
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let now = host.pane.now();
            assert!(host.nav.changed(&host.pane, "/files", now));
            assert!(
                host.nav.fire_watch(&mut host.pane, &[], now + 400),
                "the unheld control must reread"
            );
            assert_eq!(host.nav.counters.lists, 1);
        })
        .unwrap();
    let handle = open(&mut cx);
    frame(&mut cx, handle);
    let at = handle
        .update(&mut cx, |shell, _, _| {
            let frame = shell.panes[0].as_ref().unwrap().input.frame.unwrap();
            gpui::point(
                frame.bounds.left() + gpui::px(40.),
                frame.bounds.top() + gpui::px(frame.row * 1.5),
            )
        })
        .unwrap();
    let held = |cx: &mut gpui::TestAppContext| {
        handle
            .update(cx, |shell, _, _| {
                shell.panes[0].as_ref().unwrap().nav.gate.pointer_held
            })
            .unwrap()
    };
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    assert!(
        held(&mut cx),
        "a press on the rows did not hold the re-read"
    );
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let now = host.pane.now();
            let path = host.pane.path().to_owned();
            host.nav.changed(&host.pane, &path, now);
            assert!(
                !host.nav.fire_watch(&mut host.pane, &[], now + 400),
                "a watched re-read ran under a held press"
            );
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    assert!(!held(&mut cx), "the release kept the hold");
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            let due = host
                .nav
                .watch
                .due
                .expect("release must rearm the held debt");
            assert!(host.nav.fire_watch(&mut host.pane, &[], due));
            assert_eq!(host.nav.watch.rereads, 1);
            assert_eq!(host.nav.counters.lists, 1);
            assert!(!host.nav.fire_watch(&mut host.pane, &[], due + 400));
        })
        .unwrap();
}

#[test]
fn m9_lang_marks_setting_reaches_the_list_rows() {
    use crate::row::marks::Mark;
    let mut cx = bench::real_cx();
    let handle = open(&mut cx);
    handle
        .update(&mut cx, |shell, _, _| {
            let held = &mut shell.panes[0].as_mut().unwrap().pane.held;
            let mut rows = held.rows.clone();
            rows[0].name = "main.rs".into();
            rows[0].dir = false;
            rows[0].mode = 0o100644;
            *held = std::rc::Rc::new(Held {
                listing: held.listing,
                total: held.total,
                start: held.start,
                rows,
            });
        })
        .unwrap();
    for (on, language) in [(true, true), (false, false)] {
        handle
            .update(&mut cx, |shell, _, _| {
                let patch = flea::jsondoc::parse(&format!(r#"{{"langMarks":{on}}}"#)).unwrap();
                shell.launch.settings.data =
                    flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
            })
            .unwrap();
        frame(&mut cx, handle);
        let mark = handle
            .update(&mut cx, |shell, _, _| {
                let list = shell.panes[0].as_ref().unwrap().list.borrow();
                list.visible()
                    .iter()
                    .find(|shown| shown.index == 0)
                    .and_then(|shown| shown.prepared.as_ref())
                    .map(|prepared| prepared.mark)
            })
            .unwrap();
        assert_eq!(
            mark.map(|mark| matches!(mark, Mark::Language(_))),
            Some(language),
            "langMarks {on} drew {mark:?} for main.rs"
        );
    }
}
