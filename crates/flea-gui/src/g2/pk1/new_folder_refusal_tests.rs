//! New Folder refusals in directory dialogs: a failed mkdir, a location that cannot hold a folder,
//! an answer already in flight, a lost backend, and answers that must not navigate.
use super::new_folder_tests::{made, mkdirs, press_new_folder, seed};
use super::process::Line;
use super::tests::fixture::fixture;
use gpui::TestAppContext;

#[gpui::test]
fn nf_mkdir_error_keeps_the_folder_reports_it_and_retries(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    press_new_folder(&mut f);
    f.checks.say(Line::Reply(
        r#"{"t":"error","where":"mkdir","msg":"Read-only file system"}"#.into(),
    ));
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d",
        "a refused mkdir changes no folder"
    );
    let message = f
        .read(|picker| picker.message.clone())
        .expect("the backend's refusal is shown");
    assert!(message.error, "the refusal is loud");
    assert!(message.text.contains("Read-only file system"));
    assert!(
        f.read(|picker| picker.chrome.folder_request.is_none()),
        "the refusal clears the pending creation"
    );
    assert!(f.written().is_none(), "a refusal writes no response");
    press_new_folder(&mut f);
    assert_eq!(
        mkdirs(&f).len(),
        2,
        "the cleared operation can be asked again"
    );
    made(&mut f, "/d/New Folder");
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d/New Folder"
    );
}

#[gpui::test]
fn nf_recent_location_refuses_creation(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    f.update(|picker, cx| picker.open_folder(flea::ui_logic_pk1::RECENT, cx));
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        flea::ui_logic_pk1::RECENT
    );
    assert!(
        f.cx.debug_bounds("picker-new-folder").is_some(),
        "Recent still draws the control its mode allows"
    );
    press_new_folder(&mut f);
    assert!(
        mkdirs(&f).is_empty(),
        "a location that is not a directory refuses creation"
    );
}

#[gpui::test]
fn nf_a_pending_creation_refuses_a_second_request(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    press_new_folder(&mut f);
    assert_eq!(mkdirs(&f).len(), 1);
    press_new_folder(&mut f);
    assert_eq!(
        mkdirs(&f).len(),
        1,
        "a creation already in flight sends no second mkdir"
    );
}

#[gpui::test]
fn nf_submitting_answer_refuses_creation(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    f.update(|picker, cx| picker.accept(cx));
    f.cx.run_until_parked();
    assert!(
        f.read(|picker| picker.accept_state().submitting()),
        "the answer is in flight"
    );
    press_new_folder(&mut f);
    assert!(
        mkdirs(&f).is_empty(),
        "an answer already submitting refuses creation"
    );
}

#[gpui::test]
fn nf_unavailable_backend_refuses_creation(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    f.update(|picker, cx| picker.backend_failed(cx));
    f.cx.run_until_parked();
    press_new_folder(&mut f);
    assert!(
        mkdirs(&f).is_empty(),
        "a lost backend refuses creation instead of sending into it"
    );
}

#[gpui::test]
fn nf_a_completion_for_another_parent_does_not_navigate(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    press_new_folder(&mut f);
    made(&mut f, "/other/New Folder");
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d",
        "a folder made somewhere else is not entered"
    );
}

#[gpui::test]
fn nf_a_completion_that_is_not_an_immediate_child_does_not_navigate(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"open","directory":true,"folder":"/d"}"#,
        None,
    );
    seed(&mut f, "/d");
    press_new_folder(&mut f);
    let before = f.listing.sent().len();
    made(&mut f, "/d/sub/deep");
    assert_eq!(
        f.read(|picker| picker.listing.location.clone()),
        "/d",
        "a nested answer is not the child of this parent"
    );
    assert_eq!(
        f.listing.sent().len(),
        before,
        "no listing is asked for a folder this parent did not make"
    );
}
