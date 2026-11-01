//! A Columns file opens on a double click and renames on a slow second click (EY-319, EY-146), as Finder and v0.3.12 do.
use super::host;
use crate::{
    columns::geometry::{Slot, NAME_X},
    state::Shell,
};
use gpui::{
    point, px, Modifiers, MouseButton, MouseDownEvent, Pixels, Point, TestAppContext,
    VisualTestContext, WindowHandle,
};
use std::{cell::RefCell, time::Duration};

thread_local! {
    static OPENED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}
fn record(argv: &[String]) -> crate::g2::op3::lane::Exit {
    OPENED.with(|paths| paths.borrow_mut().push(argv.to_vec()));
    (Some(0), String::new())
}

type Lines = Box<dyn FnMut() -> Option<String>>;

/// Columns on /srv/a/b/c with a folder and a file, the opener recorded and the cursor on the folder.
fn columns(cx: &mut TestAppContext) -> (WindowHandle<Shell>, Point<Pixels>, Lines) {
    OPENED.with(|paths| paths.borrow_mut().clear());
    let (handle, _requests) = host::open(cx, "/srv/a/b/c");
    let (backend, lines) = crate::backend::Backend::test_lines();
    host::change(cx, handle, |shell, window, cx| {
        shell.opener[0].launch = record;
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = std::rc::Rc::new(crate::list::pane::Held {
            listing: 3,
            total: 2,
            start: 0,
            rows: vec![host::row("d01", true), host::row("notes.txt", false)],
        });
        host.changed(&shell.locale);
        host::settle(shell, window, cx);
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
    });
    host::frame(cx, handle);
    let at = handle
        .update(cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Active).unwrap().borrow();
            let drawn = paint.drawn.as_ref().unwrap();
            let y = drawn.row_height * 1.5 - drawn.scroll;
            point(
                drawn.bounds.left() + px(NAME_X + 4.),
                drawn.bounds.top() + px(y),
            )
        })
        .unwrap();
    (handle, at, Box::new(lines))
}

fn tap(cx: &mut VisualTestContext, at: Point<Pixels>, count: usize) {
    cx.simulate_event(MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: count,
        ..Default::default()
    });
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn wait(cx: &mut VisualTestContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn editing(handle: WindowHandle<Shell>, cx: &mut VisualTestContext) -> Option<String> {
    handle
        .update(cx, |shell, _, _| {
            let edit = shell.rename.edit.as_ref()?;
            shell.renaming_row(0)?;
            Some(edit.entry.text[edit.entry.selected.clone()].to_owned())
        })
        .unwrap()
}

fn opened() -> Vec<Vec<String>> {
    OPENED.with(|paths| paths.borrow().clone())
}

const OPEN: [&str; 2] = ["--open", "/srv/a/b/c/notes.txt"];
/// The double-click interval the slow rename waits out (rename_slow_policy.rs).
const INTERVAL: u64 = crate::input::slow::DOUBLE_CLICK_MS;
/// Half again the interval: the next tap is a fresh one, and an armed rename has fired.
const PAST: u64 = INTERVAL * 3 / 2;
/// A quarter of the interval: an armed rename is still pending.
const INSIDE: u64 = INTERVAL / 4;

#[test]
fn ey319_columns_double_click_on_a_file_opens_it_and_never_renames() {
    let mut cx = host::context();
    let (handle, at, _lines) = columns(&mut cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    tap(&mut vcx, at, 1);
    tap(&mut vcx, at, 2);
    wait(&mut vcx, PAST);
    assert_eq!(
        editing(handle, &mut vcx),
        None,
        "a double click never renames"
    );
    let opened = opened();
    assert_eq!(opened.len(), 1, "the second tap opens the file once");
    assert_eq!(&opened[0][1..], OPEN);
}

#[test]
fn ey146_columns_slow_second_click_on_the_selected_name_renames() {
    let mut cx = host::context();
    let (handle, at, _lines) = columns(&mut cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    tap(&mut vcx, at, 1);
    wait(&mut vcx, PAST);
    assert_eq!(
        editing(handle, &mut vcx),
        None,
        "the first tap only selects"
    );
    tap(&mut vcx, at, 1);
    wait(&mut vcx, INSIDE);
    assert_eq!(
        editing(handle, &mut vcx),
        None,
        "the rename waits out the interval"
    );
    wait(&mut vcx, INTERVAL);
    assert_eq!(editing(handle, &mut vcx).as_deref(), Some("notes"));
    assert!(opened().is_empty(), "a slow click never opens");
}

#[test]
fn ey319_columns_double_click_cancels_a_pending_slow_rename_and_opens() {
    let mut cx = host::context();
    let (handle, at, _lines) = columns(&mut cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    tap(&mut vcx, at, 1);
    wait(&mut vcx, PAST);
    tap(&mut vcx, at, 1);
    wait(&mut vcx, INSIDE);
    let armed = handle
        .update(&mut vcx, |shell, _, _| {
            shell
                .rename
                .slow
                .as_ref()
                .is_some_and(|slow| slow.pending())
        })
        .unwrap();
    assert!(armed, "the slow second tap armed its rename");
    tap(&mut vcx, at, 2);
    wait(&mut vcx, PAST);
    assert_eq!(
        editing(handle, &mut vcx),
        None,
        "the double click cancelled the rename"
    );
    let opened = opened();
    assert_eq!(opened.len(), 1, "the double click opens the file");
    assert_eq!(&opened[0][1..], OPEN);
}

fn tap_with(cx: &mut VisualTestContext, at: Point<Pixels>, modifiers: Modifiers) {
    cx.simulate_event(MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: 1,
        modifiers,
        ..Default::default()
    });
    cx.simulate_mouse_up(at, MouseButton::Left, modifiers);
    cx.run_until_parked();
}

fn selected(handle: WindowHandle<Shell>, cx: &mut VisualTestContext) -> Vec<usize> {
    handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().selected.clone()
        })
        .unwrap()
}

#[test]
fn ey315_columns_modified_tap_on_the_blank_area_keeps_the_selection() {
    let mut cx = host::context();
    let (handle, file, _lines) = columns(&mut cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let row = handle
        .update(&mut vcx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Active).unwrap().borrow();
            paint.drawn.as_ref().unwrap().row_height
        })
        .unwrap();
    let folder = point(file.x, file.y - px(row));
    let blank = point(file.x, file.y + px(row * 3.));
    tap(&mut vcx, file, 1);
    wait(&mut vcx, PAST);
    tap_with(&mut vcx, folder, Modifiers::control());
    assert_eq!(
        selected(handle, &mut vcx),
        [0, 1],
        "the Ctrl tap marks the folder too"
    );
    for modifiers in [Modifiers::shift(), Modifiers::control()] {
        wait(&mut vcx, PAST);
        tap_with(&mut vcx, blank, modifiers);
        assert_eq!(
            selected(handle, &mut vcx),
            [0, 1],
            "a {modifiers:?} tap below the rows keeps the selection"
        );
    }
    wait(&mut vcx, PAST);
    tap_with(&mut vcx, blank, Modifiers::none());
    assert!(
        selected(handle, &mut vcx).is_empty(),
        "a plain tap below the rows still clears"
    );
}
