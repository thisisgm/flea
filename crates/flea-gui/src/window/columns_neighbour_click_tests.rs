//! Every visible column accepts real window pointer dispatch, including the child of the focused folder.
use super::host;
use crate::{columns::geometry::Slot, state::Shell};
use gpui::{
    point, px, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    WindowHandle,
};
use std::cell::RefCell;

thread_local! {
    static OPENED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

fn record(argv: &[String]) -> crate::g2::open_with_properties::lane::Exit {
    OPENED.with(|paths| paths.borrow_mut().push(argv.to_vec()));
    (Some(0), String::new())
}

type Requests = Box<dyn FnMut() -> Option<(usize, usize)>>;

fn child(cx: &mut TestAppContext) -> (WindowHandle<Shell>, Point<Pixels>, Requests) {
    let (handle, requests) = host::open(cx, "/srv/a/b/c");
    host::change(cx, handle, |shell, window, cx| {
        host::peek(
            shell,
            "/srv/a/b/c/d01",
            &[("inside", true), ("notes.txt", false)],
            false,
            window,
            cx,
        );
        host::settle(shell, window, cx);
    });
    host::frame(cx, handle);
    let at = handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let columns = host.columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Child).unwrap().borrow();
            let drawn = paint.drawn.as_ref().unwrap();
            assert!(drawn.view().enabled, "the child rows are ready");
            assert_eq!(drawn.view().path, "/srv/a/b/c/d01");
            assert_eq!(host.pane.place(), "/srv/a/b/c");
            point(
                drawn.bounds.left() + px(60.),
                drawn.bounds.top() + px(drawn.row_height / 2. - drawn.scroll),
            )
        })
        .unwrap();
    (handle, at, Box::new(requests))
}

#[test]
fn columns_click_child_folder_responds_without_clicking_its_parent() {
    let mut cx = host::context();
    let (handle, at, _requests) = child(&mut cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    host::change(&mut vcx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.place(),
            "/srv/a/b/c/d01/inside",
            "a visible child folder takes the first click"
        );
    });
}

#[test]
fn columns_click_child_right_click_opens_its_row_menu() {
    let mut cx = host::context();
    let (handle, at, _requests) = child(&mut cx);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    vcx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    host::change(&mut vcx, handle, |shell, _, _| {
        let captured = shell.menu.captured.as_ref().expect("the child row's menu");
        assert_eq!(captured.path, "/srv/a/b/c/d01");
        assert_eq!(captured.target.as_ref().unwrap().name, "inside");
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.place(), "/srv/a/b/c");
    });
}

#[test]
fn columns_click_child_file_selects_its_folder_and_exact_row() {
    let mut cx = host::context();
    let (handle, folder, _requests) = child(&mut cx);
    let at = point(folder.x, folder.y + px(crate::pane::ROW_HEIGHT));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    host::change(&mut vcx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.place(), "/srv/a/b/c/d01");
        assert_eq!(
            host.nav.pending_select.as_deref(),
            Some("/srv/a/b/c/d01/notes.txt"),
            "the clicked file becomes the new listing's selected row"
        );
    });
}

#[test]
fn columns_click_child_file_double_click_opens_it_before_the_listing_lands() {
    let mut cx = host::context();
    let (handle, folder, _requests) = child(&mut cx);
    OPENED.with(|paths| paths.borrow_mut().clear());
    host::change(&mut cx, handle, |shell, _, _| {
        shell.opener[0].launch = record
    });
    let at = point(folder.x, folder.y + px(crate::pane::ROW_HEIGHT));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    for click_count in 1..=2 {
        vcx.simulate_event(gpui::MouseDownEvent {
            position: at,
            button: MouseButton::Left,
            click_count,
            ..Default::default()
        });
        vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        vcx.run_until_parked();
        if click_count == 1 {
            assert!(OPENED.with(|paths| paths.borrow().is_empty()));
        }
    }
    OPENED.with(|paths| {
        let paths = paths.borrow();
        assert_eq!(paths.len(), 1);
        assert_eq!(&paths[0][1..], ["--open", "/srv/a/b/c/d01/notes.txt"]);
    });
}

#[test]
fn columns_click_child_file_drag_lifts_the_pressed_file() {
    let mut cx = host::context();
    let (handle, folder, _requests) = child(&mut cx);
    let at = point(folder.x, folder.y + px(crate::pane::ROW_HEIGHT));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    host::change(&mut vcx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.pane.place(),
            "/srv/a/b/c",
            "a held press must not navigate"
        );
        assert!(host.nav.pending_select.is_none());
    });
    vcx.simulate_mouse_move(
        point(at.x + px(15.), at.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    vcx.run_until_parked();
    host::change(&mut vcx, handle, |shell, window, cx| {
        let Some(gpui::ExternalDragPayload::Mime(payload)) = shell.file_drag_payload(window, cx)
        else {
            panic!("the child file starts a drag");
        };
        assert_eq!(
            payload
                .data
                .get("text/uri-list")
                .map(|bytes| bytes.to_vec()),
            Some(b"file:///srv/a/b/c/d01/notes.txt\r\n".to_vec())
        );
    });
}

#[test]
fn columns_click_scrolled_child_file_lands_in_the_active_column() {
    let mut cx = host::context();
    let (handle, _requests) = host::open(&mut cx, "/srv/a/b/c");
    let names: Vec<_> = (0..60)
        .map(|index| format!("item-{index:02}.txt"))
        .collect();
    let rows: Vec<_> = names.iter().map(|name| (name.as_str(), false)).collect();
    host::change(&mut cx, handle, |shell, window, cx| {
        OPENED.with(|paths| paths.borrow_mut().clear());
        shell.opener[0].launch = record;
        host::peek(shell, "/srv/a/b/c/d01", &rows, false, window, cx);
        host::settle(shell, window, cx);
        shell.panes[0]
            .as_mut()
            .unwrap()
            .columns
            .as_mut()
            .unwrap()
            .area
            .scroll_by(Slot::Child, 620.);
    });
    host::frame(&mut cx, handle);
    let at = handle
        .update(&mut cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(Slot::Child).unwrap().borrow();
            let drawn = paint.drawn.as_ref().unwrap();
            assert!(drawn.scroll > 0.);
            point(
                drawn.bounds.left() + px(60.),
                drawn.bounds.top() + px(30.5 * drawn.row_height - drawn.scroll),
            )
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    host::change(&mut vcx, handle, |shell, window, cx| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.place(),
            "/srv/a/b/c/d01"
        );
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/srv/a/b/c/d01".into(),
                total: 60,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                listing: 4,
                rows: names.iter().map(|name| host::row(name, false)).collect(),
            },
        ] {
            shell.receive(0, crate::backend::Event::Reply(reply), window, cx);
        }
        host::settle(shell, window, cx);
    });
    host::advance(&mut vcx, handle, std::time::Duration::from_secs(1));
    host::frame(&mut vcx, handle);
    host::change(&mut vcx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.path, "/srv/a/b/c/d01");
        assert_eq!(host.pane.cursor, 30);
        assert_eq!(host.selected, [30]);
        let columns = host.columns.as_ref().unwrap();
        let paint = columns.painters.get(Slot::Active).unwrap().borrow();
        let drawn = paint.drawn.as_ref().unwrap();
        assert_eq!(drawn.view().path, "/srv/a/b/c/d01");
        assert!(
            drawn.scroll > 0.,
            "the selected row is beyond the first page"
        );
        let top = 30. * drawn.row_height - drawn.scroll;
        assert!(top >= 0. && top + drawn.row_height <= f32::from(drawn.bounds.size.height));
    });
    vcx.simulate_event(gpui::MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    vcx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
    OPENED.with(|paths| {
        let paths = paths.borrow();
        assert_eq!(paths.len(), 1, "the double click survives column promotion");
        assert_eq!(&paths[0][1..], ["--open", "/srv/a/b/c/d01/item-30.txt"]);
    });
}
