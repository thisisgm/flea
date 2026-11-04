//! Mounted Columns gestures use the same selection, rename and transfer owners as List.
use super::host;
use crate::{backend::Backend, columns::geometry::Slot, state::Shell};
use gpui::{point, px, Modifiers, MouseButton, Pixels, Point, VisualTestContext, WindowHandle};
use std::{path::PathBuf, rc::Rc, time::Duration};

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let made = std::process::Command::new("mktemp")
            .args(["-d", "/tmp/flea-columns-gesture.XXXXXXXX"])
            .output()
            .unwrap();
        assert!(made.status.success());
        let path = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
        assert!(
            path.is_absolute() && path.starts_with("/tmp") && path != std::path::Path::new("/tmp")
        );
        std::fs::write(path.join(".flea-test-root"), b"").unwrap();
        std::fs::create_dir(path.join("Empty")).unwrap();
        std::fs::create_dir(path.join("Kinds")).unwrap();
        std::fs::write(path.join("Kinds/item-01.txt"), b"one").unwrap();
        std::fs::write(path.join("Kinds/item-02.txt"), b"two").unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if self.0.is_absolute()
            && !self.0.as_os_str().is_empty()
            && self.0.starts_with("/tmp")
            && self.0 != std::path::Path::new("/tmp")
            && self.0.join(".flea-test-root").is_file()
        {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

fn open(
    cx: &mut gpui::TestAppContext,
    root: &Root,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    let path = root.0.join("Kinds").to_string_lossy().into_owned();
    let (handle, _requests) = host::open(cx, &path);
    let (backend, lines) = Backend::test_lines();
    host::change(cx, handle, |shell, window, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.held = Rc::new(crate::list::pane::Held {
            listing: 3,
            total: 2,
            start: 0,
            rows: vec![
                host::row("item-01.txt", false),
                host::row("item-02.txt", false),
            ],
        });
        host.selected = vec![1];
        host.pane.cursor = 1;
        host.changed(&shell.locale);
        host::peek(
            shell,
            root.0.to_str().unwrap(),
            &[("Empty", true), ("Kinds", true)],
            false,
            window,
            cx,
        );
        host::settle(shell, window, cx);
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
    });
    host::frame(cx, handle);
    (handle, lines)
}

fn at(
    cx: &mut gpui::TestAppContext,
    handle: WindowHandle<Shell>,
    slot: Slot,
    row: usize,
    x: f32,
) -> Point<Pixels> {
    handle
        .update(cx, |shell, _, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns.painters.get(slot).unwrap().borrow();
            let drawn = paint.drawn.as_ref().unwrap();
            point(
                drawn.bounds.left() + px(x),
                drawn.bounds.top() + px(row as f32 * 31. + 15. - drawn.scroll),
            )
        })
        .unwrap()
}

fn click(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
}

#[test]
fn ey146_columns_second_slow_name_click_opens_stem_rename() {
    let root = Root::new();
    let mut cx = host::context();
    let (handle, _lines) = open(&mut cx, &root);
    let name = at(&mut cx, handle, Slot::Active, 0, 50.);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    click(&mut vcx, name);
    vcx.executor().advance_clock(Duration::from_secs(1));
    host::change(&mut vcx, handle, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "renameEditorLive", &[], window, cx)
                .stdout
                .trim(),
            "false"
        );
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0]);
    });
    click(&mut vcx, name);
    vcx.executor().advance_clock(Duration::from_millis(401));
    vcx.run_until_parked();
    host::frame(&mut vcx, handle);
    host::change(&mut vcx, handle, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "renameEditorLive", &[], window, cx)
                .stdout
                .trim(),
            "true"
        );
        assert_eq!(
            host::call(shell, "renameEditorText", &[], window, cx)
                .stdout
                .trim(),
            "item-01.txt"
        );
        let state = host::call(shell, "renameState", &[], window, cx).stdout;
        assert!(state.contains(r#""selectedText":"item-01""#), "{state}");
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0]);
    });
}

#[test]
fn ey147_columns_drag_to_parent_folder_sends_one_move() {
    drops(Modifiers::default(), "move");
}

#[test]
fn ey147_columns_ctrl_drop_to_parent_folder_copies() {
    drops(
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
        "copy",
    );
}

fn drops(modifiers: Modifiers, verb: &str) {
    let root = Root::new();
    let mut cx = host::context();
    let (handle, mut lines) = open(&mut cx, &root);
    let file = at(&mut cx, handle, Slot::Active, 0, 50.);
    let folder = at(&mut cx, handle, Slot::Parent, 0, 50.);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(file, None, Modifiers::default());
    vcx.simulate_mouse_down(file, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(file.x + px(15.), file.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_move(folder, Some(MouseButton::Left), modifiers);
    vcx.simulate_mouse_up(folder, MouseButton::Left, modifiers);
    vcx.run_until_parked();
    let sent: Vec<_> = std::iter::from_fn(&mut lines).collect();
    let probe = sent
        .iter()
        .find(|line| line.contains(r#""c":"collisions""#))
        .expect("a drop probes the destination");
    let probe = flea::jsondoc::parse(probe).unwrap();
    let id = probe
        .get("id")
        .and_then(flea::jsondoc::Json::as_f64)
        .unwrap();
    host::change(&mut vcx, handle, |shell, window, cx| {
        shell.receive(
            0,
            crate::backend::Event::Operations(format!(
                r#"{{"t":"collisions","id":{id},"total":0,"names":[],"mergeable":false}}"#
            )),
            window,
            cx,
        );
    });
    vcx.run_until_parked();
    let after: Vec<_> = std::iter::from_fn(&mut lines).collect();
    let transfers: Vec<_> = after
        .iter()
        .filter(|line| line.contains(r#""c":"transfer""#))
        .collect();
    assert_eq!(transfers.len(), 1, "{after:?}");
    let transfer = flea::jsondoc::parse(transfers[0]).unwrap();
    assert_eq!(
        transfer.get("op").and_then(flea::jsondoc::Json::as_str),
        Some(verb)
    );
    assert_eq!(
        transfer.get("dest").and_then(flea::jsondoc::Json::as_str),
        root.0.join("Empty").to_str()
    );
    assert!(transfers[0].contains(root.0.join("Kinds/item-01.txt").to_str().unwrap()));
    assert!(!transfers[0].contains("item-02.txt"));
}

#[test]
fn ey146_columns_click_past_name_disarms_slow_rename() {
    let root = Root::new();
    let mut cx = host::context();
    let (handle, _lines) = open(&mut cx, &root);
    let name = at(&mut cx, handle, Slot::Active, 0, 50.);
    let past = at(&mut cx, handle, Slot::Active, 0, 250.);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    click(&mut vcx, name);
    vcx.executor().advance_clock(Duration::from_secs(1));
    click(&mut vcx, past);
    vcx.executor().advance_clock(Duration::from_secs(1));
    click(&mut vcx, name);
    vcx.executor().advance_clock(Duration::from_secs(1));
    vcx.run_until_parked();
    host::change(&mut vcx, handle, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "renameEditorLive", &[], window, cx)
                .stdout
                .trim(),
            "false"
        );
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0]);
    });
}

#[test]
fn ey147_columns_escape_cancels_drag_without_transfer_or_rename() {
    let root = Root::new();
    let mut cx = host::context();
    let (handle, mut lines) = open(&mut cx, &root);
    let file = at(&mut cx, handle, Slot::Active, 0, 50.);
    let folder = at(&mut cx, handle, Slot::Parent, 0, 50.);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(file, None, Modifiers::default());
    vcx.simulate_mouse_down(file, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(file.x + px(15.), file.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    assert!(vcx.update(|_, cx| cx.has_active_drag()));
    vcx.simulate_keystrokes("escape");
    assert!(!vcx.update(|_, cx| cx.has_active_drag()));
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.executor().advance_clock(Duration::from_secs(1));
    vcx.run_until_parked();
    let sent: Vec<_> = std::iter::from_fn(&mut lines).collect();
    assert!(
        sent.iter()
            .all(|line| !line.contains("collisions") && !line.contains("transfer")),
        "{sent:?}"
    );
    host::change(&mut vcx, handle, |shell, window, cx| {
        assert_eq!(
            host::call(shell, "renameEditorLive", &[], window, cx)
                .stdout
                .trim(),
            "false"
        );
    });
}

#[path = "columns_peek_drag_tests.rs"]
mod peek_drag;
