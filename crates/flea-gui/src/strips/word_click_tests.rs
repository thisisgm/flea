//! A double-click in the filter and Find queries selects the word under the pointer, a third click the whole line.
use crate::{
    backend::Backend,
    path::entry::Entry,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{
    AppContext, Bounds, Context, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    PlatformWindow, TestAppContext, VisualTestContext, Window, WindowHandle,
};
use std::{cell::RefCell, rc::Rc};

type Handle = WindowHandle<Root>;
// The query line and the band its last frame drew.
type Query = (Rc<RefCell<Entry>>, Option<Bounds<Pixels>>);

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap()
}

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

// Opens the query `action` names, types "alpha beta" through the platform handler and returns the line and its drawn band.
fn typed(
    cx: &mut TestAppContext,
    action: &'static str,
    line: fn(&Shell) -> Query,
) -> (Handle, Rc<RefCell<Entry>>, Bounds<Pixels>) {
    let (backend, _read) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    frame(cx, handle);
    edit(cx, handle, |shell, window, cx| match action {
        "filter" => assert!(shell.filter_action("filter", window, cx)),
        _ => assert!(shell.search_action("search", window, cx)),
    });
    frame(cx, handle);
    let mut native = cx.test_window(handle.into());
    let mut input = native
        .take_input_handler()
        .expect("the query registers platform input");
    input.replace_text_in_range(None, "alpha beta");
    native.set_input_handler(input);
    cx.run_until_parked();
    frame(cx, handle);
    let (entry, band) = edit(cx, handle, |shell, _, _| line(shell));
    assert_eq!(entry.borrow().text, "alpha beta");
    (handle, entry, band.expect("the query drew its band"))
}

fn presses(
    cx: &mut TestAppContext,
    handle: Handle,
    entry: &RefCell<Entry>,
    band: Bounds<Pixels>,
) -> Vec<String> {
    let at = gpui::point(band.left() + gpui::px(6.), band.center().y);
    (1..=3)
        .map(|clicks| {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            visual.simulate_event(MouseDownEvent {
                button: MouseButton::Left,
                position: at,
                modifiers: Modifiers::none(),
                click_count: clicks,
                first_mouse: false,
            });
            visual.simulate_event(MouseUpEvent {
                button: MouseButton::Left,
                position: at,
                modifiers: Modifiers::none(),
                click_count: clicks,
            });
            frame(cx, handle);
            let entry = entry.borrow();
            entry.text[entry.selected.clone()].to_owned()
        })
        .collect()
}

#[gpui::test]
fn a_double_click_in_the_filter_query_selects_one_word(cx: &mut TestAppContext) {
    let (handle, entry, band) = typed(cx, "filter", |shell| {
        let filter = &shell.panes[0].as_ref().unwrap().filter;
        (
            filter.owner.entry.clone(),
            filter.strip.borrow().query.bounds.get(),
        )
    });
    let selected = presses(cx, handle, &entry, band);
    assert_eq!(
        selected[1], "alpha",
        "a double-click selects the word under the pointer"
    );
    assert_eq!(
        selected[2], "alpha beta",
        "a third click selects the whole query"
    );
}

#[gpui::test]
fn a_double_click_in_the_find_query_selects_one_word(cx: &mut TestAppContext) {
    let (handle, entry, band) = typed(cx, "search", |shell| {
        let search = &shell.panes[0].as_ref().unwrap().search;
        (
            search.owner.entry.clone(),
            search.strip.borrow().edit.input_bounds.get(),
        )
    });
    let selected = presses(cx, handle, &entry, band);
    assert_eq!(
        selected[1], "alpha",
        "a double-click selects the word under the pointer"
    );
    assert_eq!(
        selected[2], "alpha beta",
        "a third click selects the whole query"
    );
}
