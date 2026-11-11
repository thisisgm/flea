//! The address bar's single tap and its copy mark, through the mounted window's own pointer and keyboard paths.
use crate::window::dispatch::test_shell::{asked, land, open_root, row, status};
use crate::window::u07_tests::{press, read};
use gpui::{point, px, Entity, Modifiers, Point, TestAppContext, VisualTestContext};

// Board 0401-address-copy-a at text size 14: path gap, hit box and clearance past the slot.
const COPY_GAP: f32 = 9.;
const COPY_WIDTH: f32 = 24.;
const COPY_HEIGHT: f32 = 27.;
const CLEARANCE_AFTER_COPY: f32 = 40.;

fn alpha(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) {
    land(shell, cx, "/files", vec![row("alpha", true)], 1);
    cx.simulate_keystrokes("enter");
    land(shell, cx, "/files/alpha", vec![row("child", true)], 2);
}

// Sample input: "118 13", a centre reader's window-relative x and y.
fn at(centre: &str) -> Point<gpui::Pixels> {
    let mut values = centre.split_whitespace().map(|v| v.parse::<f32>().unwrap());
    point(px(values.next().unwrap()), px(values.next().unwrap()))
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn hover(cx: &mut VisualTestContext, to: Point<gpui::Pixels>) {
    cx.simulate_mouse_move(to, None, Modifiers::none());
    draw(cx);
}

fn mark(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> flea::jsondoc::Json {
    flea::jsondoc::parse(&read(shell, cx, "chromeButtonState", &["copy"])).unwrap()
}

fn shown(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> bool {
    mark(shell, cx).get("visible").unwrap().as_bool().unwrap()
}

// The strip's left edge and where its line ends, from the chrome's last paint.
fn line(shell: &Entity<crate::state::Shell>, cx: &mut VisualTestContext) -> (f32, f32, f32) {
    shell.read_with(cx, |shell, cx| {
        let chrome = shell.chrome.view.as_ref().unwrap().read(cx);
        let paint = chrome.path.read(cx).paint.clone();
        let paint = paint.borrow();
        let area = paint.area.expect("a painted chrome strip");
        (
            f32::from(area.origin.x),
            paint.chrome_end.expect("the copy mark follows the line"),
            f32::from(area.center().y),
        )
    })
}

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

#[gpui::test]
fn addrcopy_one_tap_on_the_leaf_or_the_ground_opens_the_field(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    alpha(&shell, cx);
    let leaf = read(&shell, cx, "crumbCentre", &["2"]);
    press(cx, &leaf, 1);
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "true");
    cx.simulate_keystrokes("escape");
    let (left, end, y) = line(&shell, cx);
    let ground = format!(
        "{} {y}",
        left + end + COPY_GAP + COPY_WIDTH + CLEARANCE_AFTER_COPY
    );
    press(cx, &ground, 1);
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "true");
    cx.simulate_keystrokes("escape");
    // A habitual double click keeps the whole path selected, as the double click did before.
    press(cx, &leaf, 1);
    press(cx, &leaf, 2);
    let (text, selected) = shell.read_with(cx, |shell, cx| {
        let chrome = shell.chrome.view.as_ref().unwrap().read(cx);
        let entry = &chrome.path.read(cx).entry;
        (entry.text.clone(), entry.selected.clone())
    });
    assert_eq!(text, "/files/alpha/");
    assert_eq!(selected, 0..text.len());
    cx.simulate_keystrokes("escape");
    let parent = read(&shell, cx, "crumbCentre", &["1"]);
    press(cx, &parent, 1);
    assert_eq!(asked(&shell, cx), "/files");
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "false");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn addrcopy_the_mark_shows_only_while_the_pointer_is_over_the_address(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    alpha(&shell, cx);
    assert!(!shown(&shell, cx), "nothing is added at rest");
    let leaf = at(&read(&shell, cx, "crumbCentre", &["2"]));
    hover(cx, leaf);
    assert!(shown(&shell, cx));
    let (left, end, _) = line(&shell, cx);
    let rect = mark(&shell, cx)
        .get("rect")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    // Sample input: "176 0 24 27", the mark's window-relative x y width height.
    let rect: Vec<f32> = rect
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    let bounds = shell.read_with(cx, |shell, cx| {
        let chrome = shell.chrome.view.as_ref().unwrap().read(cx);
        let frame = chrome.frame.borrow();
        frame.controls[crate::path::chrome::COPY].bounds
    });
    // The headless window is 2x: layout snaps to device pixels before IPC rounds its logical coordinates.
    let origin = cx.update(|window, _| window.pixel_snap(px(left + (end + COPY_GAP))));
    assert_eq!(
        bounds.expect("the painted copy mark").origin.x,
        origin,
        "one chrome gap after the line on the device grid"
    );
    assert_eq!(rect[0], f32::from(origin).round());
    assert_eq!(
        (rect[2], rect[3]),
        (COPY_WIDTH, COPY_HEIGHT),
        "the chrome's own hit box"
    );
    hover(cx, point(leaf.x, px(300.)));
    assert!(!shown(&shell, cx), "gone the moment the pointer leaves");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn addrcopy_a_click_on_the_mark_copies_the_folder_and_never_opens_the_field(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root(cx, "default");
    alpha(&shell, cx);
    let leaf = at(&read(&shell, cx, "crumbCentre", &["2"]));
    hover(cx, leaf);
    let centre = read(&shell, cx, "chromeButtonCentre", &["copy"]);
    hover(cx, at(&centre));
    assert!(shown(&shell, cx), "the mark stays under the pointer");
    press(cx, &centre, 1);
    assert_eq!(clipboard(cx).as_deref(), Some("/files/alpha"));
    assert_eq!(status(&shell, cx), "Copied /files/alpha");
    assert_eq!(read(&shell, cx, "pathBarOpen", &[]), "false");
    assert_eq!(asked(&shell, cx), "/files/alpha");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn addrcopy_the_keyboard_reaches_the_mark_after_the_path(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    alpha(&shell, cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.enter_chrome(true, window, cx)));
    draw(cx);
    let start = read(&shell, cx, "focusStop", &[]);
    assert_eq!(start, "chrome:arrow-left");
    cx.simulate_keystrokes("right right");
    draw(cx);
    assert_eq!(read(&shell, cx, "focusStop", &[]), "chrome:copy");
    assert!(shown(&shell, cx), "a focused mark draws with its ring");
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert_eq!(clipboard(cx).as_deref(), Some("/files/alpha"));
    assert_eq!(status(&shell, cx), "Copied /files/alpha");
    assert_eq!(read(&shell, cx, "focusStop", &[]), "chrome:copy");
    cx.simulate_keystrokes("right");
    assert_eq!(read(&shell, cx, "focusStop", &[]), "chrome:search");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn addrcopy_a_folder_that_is_not_utf8_copies_its_exact_bytes(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let exact = "/files/col\u{fffd}\u{0}636f6cff";
    land(
        &shell,
        cx,
        "/files",
        vec![row("col\u{fffd}\u{0}636f6cff", true)],
        1,
    );
    cx.simulate_keystrokes("enter");
    land(&shell, cx, exact, vec![row("child", true)], 2);
    let y = flea::keys::binding_rows("default", flea::keys::Frontend::Gui)
        .find(|b| b.action == "copydirpath" && b.mods == "text" && b.key == "Y")
        .expect("default binds Y to copydirpath");
    cx.simulate_event(crate::window::dispatch::test_shell::keystroke(y));
    cx.run_until_parked();
    assert_eq!(clipboard(cx).as_deref(), Some("$'/files/col\\xff'"));
    cx.write_to_clipboard(gpui::ClipboardItem::new_string(String::new()));
    cx.update(|_, cx| shell.update(cx, |shell, cx| shell.copy_address(cx)));
    assert_eq!(clipboard(cx).as_deref(), Some("$'/files/col\\xff'"));
    assert_eq!(status(&shell, cx), "Copied /files/col\u{fffd}");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
