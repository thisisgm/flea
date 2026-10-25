//! A real pointer drag through the mounted List pane: lift, travel, drop, then op1's transfer line on the pane backend.
use crate::{backend::Backend, list::bench, list::pane::Held, state::tests::shell_with, state::Shell};
use flea::protocol::RowFacts;
use gpui::{
    point, px, size, AppContext, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext, WindowHandle,
};
use std::path::PathBuf;
use std::rc::Rc;

struct Root(PathBuf);

impl Root {
    // A fresh mktemp -d root holding its marker before any other write.
    fn new(name: &str) -> Self {
        let template = std::env::temp_dir().join(format!("flea-dg2-{name}.XXXXXXXX"));
        let made = std::process::Command::new("mktemp").arg("-d").arg(&template).output().expect("mktemp runs");
        assert!(made.status.success(), "mktemp -d failed");
        let root = PathBuf::from(String::from_utf8(made.stdout).expect("utf-8 path").trim_end());
        assert!(root.is_absolute() && root.starts_with(std::env::temp_dir()) && root != std::env::temp_dir());
        std::fs::write(root.join(".flea-test-root"), b"").unwrap();
        std::fs::create_dir_all(root.join("src/alpha")).unwrap();
        std::fs::write(root.join("src/b.txt"), b"x").unwrap();
        Self(root)
    }

    fn src(&self) -> String {
        self.0.join("src").to_string_lossy().into_owned()
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let owned = self.0.is_absolute()
            && !self.0.as_os_str().is_empty()
            && self.0.starts_with(std::env::temp_dir())
            && self.0 != std::env::temp_dir()
            && self.0.join(".flea-test-root").is_file();
        if owned {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn row(name: &str, dir: bool) -> RowFacts {
    let mut facts = bench::fixture("uniform").remove(0);
    facts.name = name.into();
    facts.dir = dir;
    facts.link = None;
    facts.mode = if dir { 0o40755 } else { 0o100644 };
    facts
}

fn open(cx: &mut TestAppContext, folder: String) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    open_with(cx, folder, 0)
}

// The fixture rows plus extra plain files, enough of them to give the pane a scroll lane.
fn open_with(cx: &mut TestAppContext, folder: String, extra: usize) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    let (backend, lines) = Backend::test_operation_channel();
    let handle = cx
        .open_window(size(px(1100.), px(700.)), move |_, cx| {
            let mut shell = shell_with(backend, cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.path = folder;
            let mut rows = vec![row("alpha", true), row("b.txt", false)];
            rows.extend((0..extra).map(|n| row(&format!("f{n:03}.txt"), false)));
            host.pane.held = Rc::new(Held { listing: 2, total: rows.len(), start: 0, rows });
            host.list.borrow_mut().typeface = bench::bench_typeface();
            shell
        });
    (handle, lines)
}

fn frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

// The centre of a painted row, found through the pane's own hit test.
fn centre(cx: &mut TestAppContext, handle: WindowHandle<Shell>, index: usize) -> Point<Pixels> {
    handle
        .update(cx, |shell, _, _| {
            let input = &shell.panes[0].as_ref().unwrap().input;
            let bounds = input.frame.expect("a painted pane").bounds;
            let x = bounds.left() + px(60.);
            let hits: Vec<f32> = (0..f32::from(bounds.size.height) as i32)
                .map(|dy| dy as f32)
                .filter(|dy| input.hit(point(x, bounds.top() + px(*dy))) == Some(index))
                .collect();
            assert!(!hits.is_empty(), "row {index} is not painted");
            point(x, bounds.top() + px((hits[0] + hits[hits.len() - 1]) / 2.))
        })
        .unwrap()
}

fn drain(lines: &mut impl FnMut() -> Option<String>) -> Vec<String> {
    std::iter::from_fn(lines).collect()
}

// op1 asks the backend for collisions first; an empty answer releases the transfer line itself.
fn transfer(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>, lines: &mut impl FnMut() -> Option<String>) -> String {
    let sent = drain(lines);
    let probe = sent.iter().find(|line| line.contains(r#""c":"collisions""#));
    assert!(probe.is_some(), "no collisions probe among {sent:?}");
    handle
        .update(vcx, |shell, window, cx| {
            let line = r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#;
            shell.receive(0, crate::backend::Event::Operations(line.into()), window, cx);
        })
        .unwrap();
    vcx.run_until_parked();
    let after = drain(lines);
    after
        .into_iter()
        .chain(sent)
        .find(|line| line.contains(r#""c":"transfer""#))
        .expect("a transfer after the empty collisions answer")
}

fn drag(vcx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    vcx.simulate_mouse_move(from, None, Modifiers::default());
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    let mid = point(from.x + px(3.), from.y + (to.y - from.y) / 2.);
    vcx.simulate_mouse_move(mid, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
}

#[test]
fn m9_dg2_mounted_file_dropped_on_a_folder_row_moves_into_it() {
    let root = Root::new("move");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    drag(&mut vcx, file, folder);
    vcx.run_until_parked();
    let transfer = transfer(&mut vcx, handle, &mut lines);
    assert!(transfer.contains(r#""op":"move""#), "{transfer}");
    assert!(transfer.contains(&format!("{}/b.txt", root.src())), "{transfer}");
    assert!(transfer.contains(&format!(r#""dest":"{}/alpha""#, root.src())), "{transfer}");
}

#[test]
fn m9_dg2_mounted_ctrl_at_the_drop_copies() {
    let root = Root::new("ctrl");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    let ctrl = Modifiers { control: true, ..Modifiers::default() };
    vcx.simulate_mouse_move(file, None, Modifiers::default());
    vcx.simulate_mouse_down(file, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(point(file.x + px(3.), file.y - px(4.)), Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_move(folder, Some(MouseButton::Left), ctrl);
    vcx.simulate_mouse_up(folder, MouseButton::Left, ctrl);
    vcx.run_until_parked();
    let transfer = transfer(&mut vcx, handle, &mut lines);
    assert!(transfer.contains(r#""op":"copy""#), "{transfer}");
}

#[test]
fn m9_dg2_mounted_travel_under_the_slop_sends_nothing() {
    let root = Root::new("tap");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    drag(&mut vcx, file, point(file.x + px(4.), file.y + px(3.)));
    vcx.run_until_parked();
    assert!(drain(&mut lines).iter().all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn m9_dg2_mounted_drop_on_its_own_folder_says_already_here() {
    let root = Root::new("same");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let below = point(file.x + px(200.), file.y + px(120.));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    drag(&mut vcx, file, below);
    vcx.run_until_parked();
    assert!(drain(&mut lines).iter().all(|line| !line.contains("transfer") && !line.contains("collisions")));
    let status = handle
        .update(&mut vcx, |shell, _, _| shell.panes[0].as_ref().unwrap().status.to_string())
        .unwrap();
    assert!(status == "Already in this folder." || status == "drag.already_here", "{status}");
}

#[test]
fn m9_dg2_mounted_escape_ends_the_drag_and_nothing_moves() {
    let root = Root::new("esc");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    handle.update(&mut vcx, |shell, window, cx| window.focus(&shell.focus, cx)).unwrap();
    vcx.simulate_mouse_move(file, None, Modifiers::default());
    vcx.simulate_mouse_down(file, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(folder, Some(MouseButton::Left), Modifiers::default());
    assert!(vcx.update(|_, cx| cx.has_active_drag()), "the drag started");
    vcx.simulate_keystrokes("escape");
    assert!(!vcx.update(|_, cx| cx.has_active_drag()), "Escape ended the drag");
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(drain(&mut lines).iter().all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn m9_dg2_mounted_a_blank_press_never_lifts_an_earlier_press() {
    let root = Root::new("stale");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let blank = point(file.x, file.y + px(200.));
    // A press a drop elsewhere left behind, as a drop on the other pane leaves it.
    handle
        .update(&mut cx, |shell, _, _| {
            let host = shell.panes[0].as_mut().unwrap();
            host.input.press = Some(crate::input::Press {
                row: 1,
                listing: host.pane.held.listing,
                name: Some("b.txt".into()),
                event: gpui::MouseDownEvent {
                    position: file,
                    button: MouseButton::Left,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                },
                dragged: true,
            });
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    drag(&mut vcx, blank, folder);
    vcx.run_until_parked();
    assert!(drain(&mut lines).iter().all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

fn dragging(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|_, cx| cx.has_active_drag())
}

#[test]
fn m9_dg2_mounted_a_blank_press_drags_a_marquee_not_files() {
    let root = Root::new("blank");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let blank = point(folder.x, folder.y + px(240.));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(blank, None, Modifiers::default());
    vcx.simulate_mouse_down(blank, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(folder, Some(MouseButton::Left), Modifiers::default());
    assert!(!dragging(&mut vcx), "a blank press starts no file drag");
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(drain(&mut lines).iter().all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn m9_dg2_mounted_a_middle_button_row_drag_lifts_nothing() {
    let root = Root::new("middle");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, root.src());
    frame(&mut cx, handle);
    let file = centre(&mut cx, handle, 1);
    let folder = centre(&mut cx, handle, 0);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(file, None, Modifiers::default());
    vcx.simulate_mouse_down(file, MouseButton::Middle, Modifiers::default());
    vcx.simulate_mouse_move(folder, Some(MouseButton::Middle), Modifiers::default());
    assert!(!dragging(&mut vcx), "only a left press lifts");
    vcx.simulate_mouse_up(folder, MouseButton::Middle, Modifiers::default());
    vcx.run_until_parked();
    assert!(drain(&mut lines).iter().all(|line| !line.contains("transfer") && !line.contains("collisions")));
}

#[test]
fn m9_dg2_mounted_a_scroll_lane_drag_lifts_nothing() {
    let root = Root::new("lane");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open_with(&mut cx, root.src(), 300);
    frame(&mut cx, handle);
    let folder = centre(&mut cx, handle, 0);
    let lane = handle
        .update(&mut cx, |shell, _, _| {
            let frame = shell.panes[0].as_ref().unwrap().input.frame.expect("a painted pane");
            assert!(frame.max_scroll() > 0.5, "the fixture scrolls");
            let y = frame.bounds.center().y;
            (0..400)
                .map(|dx| point(frame.bounds.right() - px(1. + dx as f32), y))
                .find(|at| frame.in_lane(*at))
                .expect("a scroll lane")
        })
        .unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    vcx.simulate_mouse_move(lane, None, Modifiers::default());
    vcx.simulate_mouse_down(lane, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(point(lane.x, lane.y + px(80.)), Some(MouseButton::Left), Modifiers::default());
    assert!(!dragging(&mut vcx), "a lane press scrolls, it never lifts");
    vcx.simulate_mouse_move(folder, Some(MouseButton::Left), Modifiers::default());
    vcx.simulate_mouse_up(folder, MouseButton::Left, Modifiers::default());
    vcx.run_until_parked();
    assert!(drain(&mut lines).iter().all(|line| !line.contains("transfer") && !line.contains("collisions")));
}
