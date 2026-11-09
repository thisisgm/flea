use super::arabic;
use crate::{
    backend::Event as BackendEvent,
    window::dispatch::{
        rename_tests::{draw, ready_root, receive},
        test_shell::{land, open_root_at, row},
    },
};
use flea::ui_logic_open_with_properties::bulkrename::Stop;
use gpui::{point, px, Modifiers, PlatformWindow as _, TestAppContext};

#[gpui::test]
fn i50_rtl_the_real_rename_field_keeps_carets_and_selection_in_text_order(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready_root(cx, "default");
    arabic(&shell, cx);
    cx.simulate_keystrokes("f2");
    draw(cx);
    let frame = shell.read_with(cx, |shell, _| {
        shell.rename.edit.as_ref().unwrap().line.frame.unwrap()
    });
    let width = cx.update(|window, _| window.viewport_size().width);
    let handle = cx.windows().into_iter().next().unwrap();
    let mut platform = cx.test_window(handle);
    let mut input = platform
        .take_input_handler()
        .expect("the production rename input handler");
    let first = input.bounds_for_range(0..0).unwrap();
    let last = input.bounds_for_range(9..9).unwrap();
    assert!(
        first.left() < last.left(),
        "the filename's caret order stays left to right"
    );
    let physical_left = width - frame.right();
    assert!(first.left() >= physical_left && last.right() <= physical_left + frame.size.width);
    assert_eq!(input.selected_text_range(true).unwrap().range, 0..5);
    platform.set_input_handler(input);
    cx.simulate_click(
        point(physical_left + px(2.), frame.center().y),
        Modifiers::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .rename
            .edit
            .as_ref()
            .unwrap()
            .entry
            .cursor()),
        0
    );
    cx.simulate_keystrokes("shift-right");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .rename
            .edit
            .as_ref()
            .unwrap()
            .entry
            .selected
            .clone()),
        0..1
    );
    cx.simulate_keystrokes("left");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .rename
            .edit
            .as_ref()
            .unwrap()
            .entry
            .cursor()),
        0
    );
}

#[gpui::test]
fn i50_rtl_the_production_bulk_rename_handler_swaps_button_navigation(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready_root(cx, "default");
    arabic(&shell, cx);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![1, 2];
        host.input.selection.changed(&host.selected);
    });
    cx.simulate_keystrokes("f2");
    assert_eq!(shell.read_with(cx, |shell, _| shell.rename.paths), Some(0));
    receive(
        &shell,
        cx,
        BackendEvent::Paths(vec!["/files/beta".into(), "/files/notes.txt".into()]),
    );
    shell.update(cx, |shell, cx| {
        shell.rename.bulk.as_mut().unwrap().card.focus = Stop::Cancel;
        cx.notify();
    });
    draw(cx);
    cx.simulate_keystrokes("left");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .rename
            .bulk
            .as_ref()
            .unwrap()
            .card
            .focus),
        Stop::Rename
    );
    cx.simulate_keystrokes("right");
    assert_eq!(
        shell.read_with(cx, |shell, _| shell
            .rename
            .bulk
            .as_ref()
            .unwrap()
            .card
            .focus),
        Stop::Cancel
    );
}

#[gpui::test]
fn i50_rtl_the_real_path_field_keeps_carets_in_text_order(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_at(cx, "/files/beta");
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 1);
    arabic(&shell, cx);
    cx.simulate_keystrokes("ctrl-l");
    draw(cx);
    let path = shell.read_with(cx, |shell, cx| {
        shell.chrome.view.as_ref().unwrap().read(cx).path.clone()
    });
    let (frame, end) = path.read_with(cx, |view, _| {
        (view.paint.borrow().frame.unwrap(), view.entry.text.len())
    });
    assert!(end > 1, "the opened path is in the field");
    let width = cx.update(|window, _| window.viewport_size().width);
    let handle = cx.windows().into_iter().next().unwrap();
    let mut platform = cx.test_window(handle);
    let mut input = platform
        .take_input_handler()
        .expect("the production path input handler");
    let first = input.bounds_for_range(0..0).unwrap();
    let last = input.bounds_for_range(end..end).unwrap();
    platform.set_input_handler(input);
    assert!(
        first.left() < last.left(),
        "the path's caret order stays left to right: {first:?} {last:?}"
    );
    let physical_left = width - frame.right();
    assert!(first.left() >= physical_left && last.right() <= physical_left + frame.size.width);
}
