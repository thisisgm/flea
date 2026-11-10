//! Every way a transfer starts reaches the eject check by the paths the backend names: search results, a Retry and a shelf drop.
use super::*;

const REFUSED: &str = "A copy on 128GB is still running. Try again when it finishes.";

fn backend_line(shell: &Entity<Shell>, line: &str, cx: &mut VisualTestContext) {
    shell.read_with(cx, |shell, _| {
        let backend = shell.panes[0].as_ref().unwrap().backend.as_ref().unwrap();
        backend.line(line.into()).unwrap();
    });
}

/// A roots line feeds the eject check alone, as the worker keeps it from every other owner.
fn roots(shell: &Entity<Shell>, line: &str, cx: &mut VisualTestContext) {
    shell.read_with(cx, |shell, _| {
        let backend = shell.panes[0].as_ref().unwrap().backend.as_ref().unwrap();
        backend.test_answered(line);
    });
}

fn held(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.read_with(cx, |shell, _| {
        let backend = shell.panes[0].as_ref().unwrap().backend.as_ref().unwrap();
        backend.transfers_on("/media/128GB")
    })
}

#[gpui::test]
fn fx_eject2_a_copy_from_search_results_holds_the_drive_its_rows_live_on(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    backend_line(
        &shell,
        r#"{"c":"transfer","op":"copy","menuId":4,"dest":"/fixture/home"}"#,
        cx,
    );
    reply(
        &shell,
        r#"{"t":"transferstarted","id":12,"n":1,"moving":false}"#,
        cx,
    );
    roots(
        &shell,
        r#"{"t":"roots","id":12,"job":true,"paths":["/fixture/home","/media/128GB/dcim/a.jpg"]}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    assert_eq!(status(&shell, cx), REFUSED);
}

#[gpui::test]
fn fx_eject2_a_retry_of_a_failed_copy_holds_the_drive_again(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    send_transfer(
        &shell,
        r#"{"c":"transfer","op":"copy","paths":["/fixture/home/big"],"dest":"/media/128GB"}"#,
        cx,
    );
    let named = r#"{"t":"roots","id":12,"job":true,"paths":["/fixture/home/big","/media/128GB"]}"#;
    roots(&shell, named, cx);
    reply(
        &shell,
        r#"{"t":"transferdone","id":12,"ok":0,"failed":1,"cancelled":false}"#,
        cx,
    );
    reply(
        &shell,
        r#"{"t":"job","job":{"id":12,"kind":"copy","state":"failed","head":""},"unfinished":0}"#,
        cx,
    );
    assert!(!held(&shell, cx), "a failed copy writes nothing");
    backend_line(&shell, r#"{"c":"jobs","op":"retry","id":12}"#, cx);
    roots(&shell, named, cx);
    reply(
        &shell,
        r#"{"t":"job","job":{"id":12,"kind":"copy","state":"running","head":""},"unfinished":1}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    assert_eq!(status(&shell, cx), REFUSED);
}

#[gpui::test]
fn fx_eject2_a_drop_out_of_the_shelf_holds_the_drive_its_entries_live_on(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    backend_line(
        &shell,
        r#"{"c":"transfer","op":"","paths":[],"dest":"/fixture/work","shelf":"9f2c"}"#,
        cx,
    );
    reply(
        &shell,
        r#"{"t":"transferstarted","id":13,"n":1,"moving":true}"#,
        cx,
    );
    roots(
        &shell,
        r#"{"t":"roots","id":13,"job":false,"kind":"move","paths":["/media/128GB/a.jpg","/fixture/work"]}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    assert_eq!(
        status(&shell, cx),
        "A move on 128GB is still running. Try again when it finishes.",
        "a drop out of the shelf moves its entries"
    );
    reply(
        &shell,
        r#"{"t":"transferdone","id":13,"ok":1,"failed":0,"cancelled":false}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(
        ejects(&service),
        1,
        "the drop has ended: {:?}",
        jobs(&service)
    );
}

#[gpui::test]
fn fx_eject2_a_release_with_no_volume_is_never_held_by_a_copy_elsewhere(cx: &mut TestAppContext) {
    let (shell, _service, cx) = shell_at("/fixture/work", cx);
    send_transfer(
        &shell,
        r#"{"c":"transfer","op":"copy","paths":["/fixture/home/a"],"dest":"/fixture/home/b"}"#,
        cx,
    );
    shell.update_in(cx, |shell, window, cx| {
        shell.quiesce_volume(0, "", window, cx)
    });
    frame(cx);
    let said = status(&shell, cx);
    assert!(!said.contains("still running"), "{said}");
}

#[path = "rail_devices_eject_kind_tests.rs"]
mod kind;
