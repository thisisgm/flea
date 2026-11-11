//! Find's and the filter's typing lines open the rename field's text menu on a right press, through the window's own pointer dispatch.
use crate::{
    backend::Backend,
    g2::context_menus::menu::{Face, Menu},
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{
    point, AppContext, ClipboardItem, Context, Modifiers, MouseButton, Pixels, PlatformWindow as _,
    Point, TestAppContext, VisualTestContext, Window, WindowHandle,
};

type Handle = WindowHandle<Root>;

#[derive(Clone, Copy, Debug)]
enum Line {
    Find,
    Filter,
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    let out = handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap();
    cx.run_until_parked();
    out
}

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn typing(
    cx: &mut TestAppContext,
    line: Line,
    text: &str,
) -> (Handle, impl FnMut() -> Option<String>) {
    let (backend, lines) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    frame(cx, handle);
    edit(cx, handle, |shell, window, cx| {
        let opened = match line {
            Line::Filter => shell.filter_action("filter", window, cx),
            Line::Find => shell.search_action("search", window, cx),
        };
        assert!(opened, "{line:?} must open");
    });
    frame(cx, handle);
    let mut input = cx
        .test_window(handle.into())
        .take_input_handler()
        .expect("the typing line registers platform input");
    input.replace_text_in_range(None, text);
    cx.test_window(handle.into()).set_input_handler(input);
    frame(cx, handle);
    (handle, lines)
}

fn text(cx: &mut TestAppContext, handle: Handle, line: Line) -> String {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        match line {
            Line::Find => host.search.owner.entry.borrow().text.clone(),
            Line::Filter => host.filter.owner.entry.borrow().text.clone(),
        }
    })
}

// Past the typed text, at the far end of the room the typing line owns.
fn inside(cx: &mut TestAppContext, handle: Handle, line: Line) -> Point<Pixels> {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let bounds = match line {
            Line::Find => host.search.strip.borrow().edit.bounds.get(),
            Line::Filter => host.filter.strip.borrow().query.room(),
        };
        let room = bounds.expect("the typing line is drawn");
        point(room.right() - gpui::px(4.), room.center().y)
    })
}

/// The middle of the open card's row whose id is `id`.
pub(crate) fn row_point(menu: &Menu, id: &str) -> Point<Pixels> {
    let card = menu.card.as_ref().expect("the menu card is open");
    let index = card
        .entries
        .iter()
        .position(|entry| entry.id.as_ref() == id)
        .expect("the card holds the row");
    let x = card.frame.x + card.frame.w / 2.;
    let rows: Vec<f32> = (0..card.frame.h as i32)
        .map(|dy| card.frame.y + dy as f32)
        .filter(|y| card.row_at(x, *y) == Some(index))
        .collect();
    let y = (rows.first().unwrap() + rows.last().unwrap()) / 2.;
    point(gpui::px(x), gpui::px(y))
}

/// The open card's rows by id, separators as empty ids.
pub(crate) fn row_ids(menu: &Menu) -> Vec<String> {
    menu.card
        .as_ref()
        .map(|card| card.entries.iter().map(|e| e.id.to_string()).collect())
        .unwrap_or_default()
}

pub(crate) const ROWS: [&str; 7] = [
    "edit:undo",
    "",
    "edit:cut",
    "edit:copy",
    "edit:paste",
    "",
    "edit:selectAll",
];

fn click(cx: &mut TestAppContext, handle: Handle, at: Point<Pixels>, button: MouseButton) {
    let mut window = VisualTestContext::from_window(handle.into(), cx);
    window.simulate_mouse_move(at, None, Modifiers::none());
    window.simulate_mouse_down(at, button, Modifiers::none());
    window.simulate_mouse_up(at, button, Modifiers::none());
    window.run_until_parked();
    frame(cx, handle);
}

#[gpui::test]
fn a_right_press_on_find_opens_the_text_menu(cx: &mut TestAppContext) {
    opens_on(cx, Line::Find);
}

#[gpui::test]
fn a_right_press_on_the_filter_opens_the_text_menu(cx: &mut TestAppContext) {
    opens_on(cx, Line::Filter);
}

fn opens_on(cx: &mut TestAppContext, line: Line) {
    let (handle, _lines) = typing(cx, line, "abc");
    cx.write_to_clipboard(ClipboardItem::new_string("XY".into()));
    let at = inside(cx, handle, line);
    click(cx, handle, at, MouseButton::Right);
    let (face, ids) = edit(cx, handle, |shell, _, _| {
        let menu = shell.menu.menu.borrow();
        (menu.face, row_ids(&menu))
    });
    assert_eq!(face, Face::Text, "TEXT_MENU_FACE {line:?}");
    assert_eq!(ids, ROWS, "TEXT_MENU_ROWS {line:?}");
    // Paste lands at the caret through the line's own commit, and the menu closes.
    let paste = edit(cx, handle, |shell, _, _| {
        row_point(&shell.menu.menu.borrow(), "edit:paste")
    });
    click(cx, handle, paste, MouseButton::Left);
    assert!(!edit(cx, handle, |shell, _, _| shell.menu_opened()));
    assert_eq!(text(cx, handle, line), "abcXY", "PASTE_ROW {line:?}");
    // Undo is live after the paste and takes it back.
    click(cx, handle, at, MouseButton::Right);
    let undo = edit(cx, handle, |shell, _, _| {
        let menu = shell.menu.menu.borrow();
        assert!(!menu.card.as_ref().unwrap().entries[0].disabled);
        row_point(&menu, "edit:undo")
    });
    click(cx, handle, undo, MouseButton::Left);
    assert_eq!(text(cx, handle, line), "abc", "UNDO_ROW {line:?}");
    // The line still types a real keystroke after the card closes, through the window's key dispatch.
    VisualTestContext::from_window(handle.into(), cx).simulate_input("d");
    frame(cx, handle);
    assert_eq!(text(cx, handle, line), "abcd", "TYPES_AFTER {line:?}");
}
