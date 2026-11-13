//! A link whose target is a folder takes a drop as a folder row does: it lights, is named and receives.
use super::mounted_tests::{centre, drag, drain, frame, transfer, Root};
use crate::{
    backend::Backend, list::bench, list::pane::Held, state::tests::shell_with, state::Shell,
};
use flea::protocol::RowFacts;
use gpui::{
    point, px, size, Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle,
};
use std::os::unix::fs::symlink;
use std::rc::Rc;

pub(super) const ALPHA: usize = 0;
pub(super) const FILE: usize = 1;
pub(super) const TO_ELSE: usize = 2;
pub(super) const TO_SELF: usize = 3;
pub(super) const INTO_ALPHA: usize = 4;
pub(super) const TO_FILE: usize = 5;
pub(super) const DANGLING: usize = 6;
pub(super) const BETA: usize = 7;
pub(super) const UNNAMED: &str = "Move 1 item to a folder · ctrl copies, shift moves";

const FILE_TYPE_MASK: u32 = 0o170000;
const DIRECTORY_TYPE: u32 = 0o040000;

fn row(name: &str, mode: u32, icon: &str, link: Option<&str>) -> RowFacts {
    let mut facts = bench::fixture("uniform").remove(0);
    facts.name = name.into();
    facts.dir = mode & FILE_TYPE_MASK == DIRECTORY_TYPE;
    facts.mode = mode;
    facts.icon = icon.into();
    facts.link = link.map(str::to_owned);
    facts
}

// The mounted root with a folder beside the listed one and five links listed as the backend lists them: d false, the icon following the target.
pub(super) fn open(
    cx: &mut TestAppContext,
    root: &Root,
) -> (WindowHandle<Shell>, impl FnMut() -> Option<String>) {
    let src = std::path::PathBuf::from(root.src());
    std::fs::create_dir_all(src.join("../elsewhere")).unwrap();
    std::fs::create_dir_all(src.join("alpha/inner")).unwrap();
    std::fs::create_dir_all(src.join("beta")).unwrap();
    for (name, target) in [
        ("to-else", "../elsewhere"),
        ("to-self", "."),
        ("into-alpha", "alpha/inner"),
        ("to-file", "b.txt"),
        ("dangling", "missing"),
    ] {
        symlink(target, src.join(name)).unwrap();
    }
    let rows = vec![
        row("alpha", 0o040755, "folder", None),
        row("b.txt", 0o100644, "text-x-generic", None),
        row("to-else", 0o120777, "folder", Some("../elsewhere")),
        row("to-self", 0o120777, "folder", Some(".")),
        row("into-alpha", 0o120777, "folder", Some("alpha/inner")),
        row("to-file", 0o120777, "text-x-generic", Some("b.txt")),
        row("dangling", 0o120777, "unknown", Some("missing")),
        row("beta", 0o040755, "folder", None),
    ];
    let (backend, lines) = Backend::test_operation_channel();
    let folder = root.src();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |_, cx| {
        let mut shell = shell_with(backend, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.path = folder;
        host.pane.held = Rc::new(Held {
            listing: 2,
            total: rows.len(),
            start: 0,
            rows,
        });
        host.list.borrow_mut().typeface = bench::bench_typeface();
        shell
    });
    frame(cx, handle);
    (handle, lines)
}

fn status(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) -> String {
    handle
        .update(vcx, |shell, _, _| {
            shell.panes[0].as_ref().unwrap().status.to_string()
        })
        .unwrap()
}

pub(super) fn footer(vcx: &mut VisualTestContext, handle: WindowHandle<Shell>) -> Vec<String> {
    handle
        .update(vcx, |shell, _, cx| {
            shell.status_messages(cx, |messages, _| {
                messages
                    .activities()
                    .iter()
                    .map(|activity| activity.text.to_string())
                    .collect()
            })
        })
        .unwrap()
}

// Row `from` lifted and held over row `to`, the button still down.
pub(super) fn hold(
    vcx: &mut VisualTestContext,
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    from: usize,
    to: usize,
) {
    let (from, to) = (centre(cx, handle, from), centre(cx, handle, to));
    vcx.simulate_mouse_move(from, None, Modifiers::default());
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_move(
        point(from.x + px(12.), from.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    vcx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    vcx.run_until_parked();
}

pub(super) fn cue(
    vcx: &mut VisualTestContext,
    handle: WindowHandle<Shell>,
) -> (Option<usize>, Option<String>) {
    handle
        .update(vcx, |shell, window, cx| {
            let states = shell.list_states(0, window, cx);
            (
                states.drop_target,
                states.drop_label.map(|label| label.to_string()),
            )
        })
        .unwrap()
}

// A dropped onto B: the lines the backend was sent and what the pane said.
pub(super) fn dropped(from: usize, to: usize, keys: Modifiers) -> (Vec<String>, String, String) {
    let root = Root::new("link-target");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, &root);
    let (from, to) = (centre(&mut cx, handle, from), centre(&mut cx, handle, to));
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    if keys == Modifiers::default() {
        drag(&mut vcx, from, to);
    } else {
        vcx.simulate_mouse_move(from, None, Modifiers::default());
        vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        vcx.simulate_mouse_move(
            point(from.x + px(3.), from.y - px(4.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        vcx.simulate_mouse_move(to, Some(MouseButton::Left), keys);
        vcx.simulate_mouse_up(to, MouseButton::Left, keys);
    }
    vcx.run_until_parked();
    let mut sent = drain(&mut lines);
    // op1 asks for collisions first; an empty answer releases the request itself.
    if sent.iter().any(|line| line.contains(r#""c":"collisions""#)) {
        handle
            .update(&mut vcx, |shell, window, cx| {
                let line = r#"{"t":"collisions","id":1,"total":0,"names":[],"mergeable":false}"#;
                let answer = crate::backend::Event::Operations(line.into());
                shell.receive(0, answer, window, cx);
            })
            .unwrap();
        vcx.run_until_parked();
        sent.extend(drain(&mut lines));
    }
    let said = status(&mut vcx, handle);
    (sent, said, root.src())
}

pub(super) fn nothing_sent(sent: &[String]) -> bool {
    sent.iter().all(|line| {
        !line.contains("transfer")
            && !line.contains("collisions")
            && !line.contains(r#""c":"link""#)
    })
}

#[test]
fn fxdroptarget_a_link_to_a_folder_lights_and_is_named_like_a_folder_row() {
    let root = Root::new("link-cue");
    let mut cx = bench::real_cx();
    let (handle, _lines) = open(&mut cx, &root);
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    hold(&mut vcx, &mut cx, handle, FILE, TO_ELSE);
    assert_eq!(
        cue(&mut vcx, handle),
        (Some(TO_ELSE), Some("move here".to_owned()))
    );
    let lines = footer(&mut vcx, handle);
    assert!(
        lines.contains(&"Move 1 item to to-else · ctrl copies, shift moves".to_owned()),
        "{lines:?}"
    );
}

#[test]
fn fxdroptarget_a_drop_on_a_link_to_a_folder_is_sent_through_the_links_path() {
    let root = Root::new("link-move");
    let mut cx = bench::real_cx();
    let (handle, mut lines) = open(&mut cx, &root);
    let (file, link) = (
        centre(&mut cx, handle, FILE),
        centre(&mut cx, handle, TO_ELSE),
    );
    let mut vcx = VisualTestContext::from_window(handle.into(), &cx);
    drag(&mut vcx, file, link);
    vcx.run_until_parked();
    let transfer = transfer(&mut vcx, handle, &mut lines);
    assert!(transfer.contains(r#""op":"move""#), "{transfer}");
    assert!(
        transfer.contains(&format!("{}/b.txt", root.src())),
        "{transfer}"
    );
    assert!(
        transfer.contains(&format!(r#""dest":"{}/to-else""#, root.src())),
        "{transfer}"
    );
}

#[test]
fn fxdroptarget_ctrl_copies_and_ctrl_shift_links_into_a_link_to_a_folder() {
    let (sent, _, src) = dropped(FILE, TO_ELSE, Modifiers::control());
    assert!(
        sent.iter().any(|line| line.contains(r#""op":"copy""#)
            && line.contains(&format!(r#""dest":"{src}/to-else""#))),
        "a copy is asked into the link's folder: {sent:?}"
    );
    let (sent, _, src) = dropped(FILE, TO_ELSE, Modifiers::control_shift());
    assert!(
        sent.iter().any(|line| line.contains(r#""c":"link""#)
            && line.contains(&format!(r#""dest":"{src}/to-else""#))),
        "a link is asked into the link's folder: {sent:?}"
    );
}
