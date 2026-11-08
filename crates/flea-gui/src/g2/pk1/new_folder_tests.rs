//! New Folder in directory dialogs: the control, its shortcut, its focus stop, its child and its refusals.
use super::process::Line;
use super::tests::fixture::{fixture, Fixture};
use gpui::{Modifiers, TestAppContext};

// The directory a request opens on, three rows, and the keyboard on the listing.
pub(super) fn seed(f: &mut Fixture, path: &str) {
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","n":3,"path":"{path}"}}"#
    )));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"a.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"},{"n":"b.txt","d":false,"s":7,"m":0,"p":33188,"i":"text-x-generic"},{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"}]}"#.into()));
    f.cx.run_until_parked();
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
}

// The empty listing of a directory just made, ready for the answer.
pub(super) fn seed_empty(f: &mut Fixture, path: &str) {
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"listed","n":0,"path":"{path}"}}"#
    )));
    f.listing.say(Line::Reply(
        r#"{"t":"rows","start":0,"listing":1,"rows":[]}"#.into(),
    ));
    f.cx.run_until_parked();
}

// The backend lines that asked for a directory.
pub(super) fn mkdirs(f: &Fixture) -> Vec<String> {
    f.checks
        .sent()
        .into_iter()
        .filter(|line| line.contains(r#""c":"mkdir""#))
        .collect()
}

// A press on the drawn control, which is the path a pointer takes.
pub(super) fn press_new_folder(f: &mut Fixture) {
    let at = f
        .cx
        .debug_bounds("picker-new-folder")
        .expect("New Folder is mounted for this request")
        .center();
    f.cx.simulate_click(at, Modifiers::none());
    f.cx.run_until_parked();
}

// The backend's own successful creation line.
pub(super) fn made(f: &mut Fixture, path: &str) {
    f.checks.say(Line::Reply(format!(
        r#"{{"t":"made","ok":true,"path":"{path}"}}"#
    )));
    f.cx.run_until_parked();
}

// Which focus handle holds the keyboard, read from the window rather than counted in Tab presses.
pub(super) fn focused_control(f: &mut Fixture) -> &'static str {
    let picker = f.picker.clone();
    f.cx.update(|window, cx| {
        let picker = picker.read(cx);
        if picker.focus.is_focused(window) {
            "picker-listing"
        } else if picker.chrome.new_folder.is_focused(window) {
            "picker-new-folder"
        } else if picker.chrome.cancel.is_focused(window) {
            "picker-cancel"
        } else if picker.chrome.answer.is_focused(window) {
            "picker-answer"
        } else {
            "picker-somewhere-else"
        }
    })
}

#[gpui::test]
fn nf_directory_button_creates_enters_and_returns_the_child(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    assert!(
        f.read(|picker| picker.browse.filename.is_none()),
        "a directory request mounts no Save name field"
    );
    press_new_folder(&mut f);
    assert_eq!(
        mkdirs(&f).len(),
        1,
        "the control asks the existing backend for one folder"
    );
    assert!(
        mkdirs(&f)[0].contains(r#""path":"/d""#),
        "the folder is made in the listing's own location"
    );
    made(&mut f, "/d/New Folder");
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d/New Folder",
        "the picker enters the child the backend made"
    );
    assert!(
        f.listing
            .sent()
            .iter()
            .any(|line| line.contains(r#""c":"list""#) && line.contains("/d/New Folder")),
        "the created child is listed"
    );
    seed_empty(&mut f, "/d/New Folder");
    f.update(|picker, cx| picker.accept(cx));
    f.cx.run_until_parked();
    let id = f.read(|picker| picker.mark_request);
    let mark = f
        .checks
        .sent()
        .into_iter()
        .find(|line| line.contains(r#""op":"mark""#))
        .expect("the answer checks the current directory");
    assert!(
        mark.contains(r#""path":"/d/New Folder""#) && mark.contains(r#""directory":true"#),
        "the answer marks the created directory: {mark}"
    );
    f.checks.say(Line::Reply(format!(
        r#"{{"t":"picker","id":{id},"op":"mark","ok":true,"marks":[{{"path":"/d/New Folder","bytes":0}}],"removed":0}}"#
    )));
    f.settle(|f| f.written().is_some());
    assert_eq!(
        f.written().as_deref(),
        Some(r#"{"response":0,"uris":["file:///d/New%20Folder"],"filter":-1}"#),
        "the caller receives the created directory's URI"
    );
}

#[gpui::test]
fn nf_directory_shortcut_creates_and_enters_in_list(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("ctrl-shift-n");
    f.cx.run_until_parked();
    assert_eq!(
        mkdirs(&f).len(),
        1,
        "Ctrl+Shift+N reaches the action from a List listing"
    );
    made(&mut f, "/d/New Folder");
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d/New Folder"
    );
}

#[gpui::test]
fn nf_directory_shortcut_creates_and_enters_in_grid(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("ctrl-3");
    f.cx.run_until_parked();
    assert!(
        f.cx.debug_bounds("picker-grid").is_some(),
        "the listing is in Grid before the shortcut"
    );
    f.cx.simulate_keystrokes("ctrl-shift-n");
    f.cx.run_until_parked();
    assert_eq!(
        mkdirs(&f).len(),
        1,
        "the shortcut is dispatched before the Grid key path"
    );
    made(&mut f, "/d/New Folder");
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d/New Folder"
    );
}

#[gpui::test]
fn nf_tab_reaches_the_control_and_shift_tab_returns(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("tab");
    f.cx.run_until_parked();
    assert_eq!(
        focused_control(&mut f),
        "picker-new-folder",
        "Tab from the listing reaches New Folder"
    );
    f.cx.simulate_keystrokes("shift-tab");
    f.cx.run_until_parked();
    assert_eq!(
        focused_control(&mut f),
        "picker-listing",
        "Shift+Tab returns to the listing"
    );
    let mut seen = Vec::new();
    for _ in 0..8 {
        f.cx.simulate_keystrokes("tab");
        f.cx.run_until_parked();
        let at = focused_control(&mut f);
        seen.push(at);
        if at == "picker-listing" {
            break;
        }
    }
    assert_eq!(
        seen.first(),
        Some(&"picker-new-folder"),
        "forward traversal leaves the listing for New Folder: {seen:?}"
    );
    assert_eq!(
        seen.iter()
            .filter(|control| **control == "picker-new-folder")
            .count(),
        1,
        "forward traversal crosses New Folder once: {seen:?}"
    );
    assert_eq!(
        seen.last(),
        Some(&"picker-listing"),
        "forward traversal returns to the listing: {seen:?}"
    );
    assert!(f.read(|picker| picker.browse.filename.is_none()));
}

#[gpui::test]
fn nf_enter_activates_the_focused_control(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("tab enter");
    f.cx.run_until_parked();
    assert_eq!(
        mkdirs(&f).len(),
        1,
        "Enter on the focused New Folder starts the same creation"
    );
    made(&mut f, "/d/New Folder");
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d/New Folder"
    );
}

#[gpui::test]
fn nf_space_activates_the_focused_control(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    f.cx.simulate_keystrokes("tab space");
    f.cx.run_until_parked();
    assert_eq!(
        mkdirs(&f).len(),
        1,
        "Space on the focused New Folder starts the created folder"
    );
    made(&mut f, "/d/New Folder");
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d/New Folder"
    );
}

#[gpui::test]
fn nf_save_keeps_the_control_and_its_filename(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"a.txt"}"#,
        None,
    );
    seed(&mut f, "/d");
    assert!(
        f.cx.debug_bounds("picker-new-folder").is_some(),
        "Save still draws New Folder"
    );
    assert!(
        f.read(|picker| picker.browse.filename.is_some()),
        "Save still draws its name field"
    );
    press_new_folder(&mut f);
    made(&mut f, "/d/New Folder");
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d/New Folder",
        "Save still enters the folder it made"
    );
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "a.txt",
        "Save's name survives the folder change"
    );
}

#[gpui::test]
fn nf_savefiles_draws_the_control_without_a_filename(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"mode":"savefiles","folder":"/d"}"#, None);
    seed(&mut f, "/d");
    assert!(
        f.cx.debug_bounds("picker-new-folder").is_some(),
        "a destination-folder chooser draws New Folder"
    );
    assert!(
        f.read(|picker| picker.browse.filename.is_none()),
        "a destination-folder chooser names no file"
    );
    press_new_folder(&mut f);
    assert_eq!(mkdirs(&f).len(), 1);
}

// One ordinary Open-file request: no control, no focus stop, and the shortcut changes nothing.
fn open_file_offers_no_creation(cx: &mut TestAppContext, request: &str) {
    let mut f = fixture(cx, request, None);
    seed(&mut f, "/d");
    assert!(
        f.cx.debug_bounds("picker-new-folder").is_none(),
        "an ordinary file request draws no New Folder: {request}"
    );
    f.cx.simulate_keystrokes("ctrl-shift-n");
    f.cx.run_until_parked();
    assert!(
        mkdirs(&f).is_empty(),
        "Ctrl+Shift+N creates nothing for {request}"
    );
    assert_eq!(f.read(|picker| picker.listing.location.clone()), "/d");
    f.cx.simulate_keystrokes("tab");
    f.cx.run_until_parked();
    assert_eq!(
        focused_control(&mut f),
        "picker-cancel",
        "the focus ring of {request} carries no New Folder stop"
    );
}

#[gpui::test]
fn nf_open_file_offers_no_creation(cx: &mut TestAppContext) {
    open_file_offers_no_creation(cx, r#"{"folder":"/d"}"#);
    open_file_offers_no_creation(cx, r#"{"folder":"/d","multiple":true}"#);
}
