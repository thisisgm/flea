//! A refused eject names the transfer that holds the drive by its own kind, and a send holds its paths from its own line.
use super::*;

fn refused(kind: &str) -> String {
    format!("{kind} on 128GB is still running. Try again when it finishes.")
}

#[gpui::test]
fn fx_eject3_a_running_move_refuses_by_its_own_kind(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    backend_line(
        &shell,
        r#"{"c":"transfer","op":"move","paths":["/fixture/home/big"],"dest":"/media/128GB"}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    assert_eq!(status(&shell, cx), refused("A move"), "before it starts");
    reply(
        &shell,
        r#"{"t":"transferstarted","id":12,"n":1,"moving":true}"#,
        cx,
    );
    roots(
        &shell,
        r#"{"t":"roots","id":12,"job":true,"kind":"move","paths":["/fixture/home/big","/media/128GB"]}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    assert_eq!(
        status(&shell, cx),
        refused("A move"),
        "once the backend names it"
    );
}

#[gpui::test]
fn fx_eject3_an_archive_and_an_action_refuse_by_their_own_kinds(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    backend_line(
        &shell,
        r#"{"c":"archive","op":"compress","paths":["/fixture/home/a"],"path":"/media/128GB/a.7z","dest":"/media/128GB","format":"7z"}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(status(&shell, cx), refused("An archive"));
    roots(
        &shell,
        r#"{"t":"archivedone","id":31,"ok":false,"verified":true,"err":"x"}"#,
        cx,
    );
    assert!(!held(&shell, cx), "a refused archive holds nothing");
    backend_line(
        &shell,
        r#"{"c":"actions","op":"run","id":5,"action":"action:resize.toml","paths":["/media/128GB/a.jpg"]}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(status(&shell, cx), refused("An action"));
    roots(
        &shell,
        r#"{"t":"actionstarted","requestId":5,"id":13,"ok":true,"error":""}"#,
        cx,
    );
    roots(
        &shell,
        r#"{"t":"roots","id":13,"job":true,"kind":"action","paths":["/media/128GB/a.jpg"]}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    assert_eq!(status(&shell, cx), refused("An action"));
}

#[gpui::test]
fn fx_eject3_a_retry_refuses_by_its_jobs_kind(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    let named = r#"{"t":"roots","id":12,"job":true,"kind":"move","paths":["/fixture/home/big","/media/128GB"]}"#;
    roots(&shell, named, cx);
    reply(
        &shell,
        r#"{"t":"job","job":{"id":12,"kind":"move","state":"failed","head":""},"unfinished":0}"#,
        cx,
    );
    assert!(!held(&shell, cx), "a failed move writes nothing");
    backend_line(&shell, r#"{"c":"jobs","op":"retry","id":12}"#, cx);
    assert!(
        !held(&shell, cx),
        "a retry holds from the roots line the backend writes for it"
    );
    roots(&shell, named, cx);
    click_mark(&shell, cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    assert_eq!(status(&shell, cx), refused("A move"));
}

#[gpui::test]
fn fx_eject3_a_localsend_send_holds_the_drive_from_its_own_line(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    backend_line(
        &shell,
        r#"{"c":"localsend","op":"send","peer":"Desk","paths":["/media/128GB/a.jpg"],"id":3}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(
        ejects(&service),
        0,
        "the send holds before its roots line: {:?}",
        jobs(&service)
    );
    assert_eq!(status(&shell, cx), refused("A send"));
    roots(
        &shell,
        r#"{"t":"roots","id":21,"job":true,"kind":"send","requestId":3,"paths":["/media/128GB/a.jpg"]}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    reply(
        &shell,
        r#"{"t":"job","job":{"id":21,"kind":"send","state":"done","head":""},"unfinished":0}"#,
        cx,
    );
    assert!(
        !held(&shell, cx),
        "the roots line took the request's paths over"
    );
    click_mark(&shell, cx);
    assert_eq!(
        ejects(&service),
        1,
        "the send has ended: {:?}",
        jobs(&service)
    );
}

#[gpui::test]
fn fx_eject3_an_airdrop_and_an_rclone_send_hold_the_drive_from_their_own_lines(
    cx: &mut TestAppContext,
) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    let rclone = r#"{"c":"rclone","op":"send","id":72,"remote":"gdrive","folder":"Uploads","paths":["/media/128GB/a.jpg"]}"#;
    backend_line(&shell, rclone, cx);
    assert!(held(&shell, cx), "an rclone send holds from its line");
    roots(
        &shell,
        r#"{"t":"rclonestarted","requestId":72,"ok":false,"error":"x"}"#,
        cx,
    );
    assert!(!held(&shell, cx), "a refused rclone send holds nothing");
    backend_line(&shell, rclone, cx);
    roots(
        &shell,
        r#"{"t":"rclonestarted","requestId":72,"id":22,"ok":true,"error":""}"#,
        cx,
    );
    assert!(
        held(&shell, cx),
        "a started send holds until its roots line"
    );
    roots(
        &shell,
        r#"{"t":"roots","id":22,"job":true,"kind":"send","requestId":72,"paths":["/media/128GB/a.jpg"]}"#,
        cx,
    );
    reply(
        &shell,
        r#"{"t":"job","job":{"id":22,"kind":"send","state":"done","head":""},"unfinished":0}"#,
        cx,
    );
    assert!(!held(&shell, cx));
    backend_line(
        &shell,
        r#"{"c":"airdrop","op":"send","id":4,"peer":"3a9f0c2be41d","name":"Phone","paths":["/media/128GB/a.jpg"]}"#,
        cx,
    );
    click_mark(&shell, cx);
    assert_eq!(ejects(&service), 0, "{:?}", jobs(&service));
    assert_eq!(status(&shell, cx), refused("A send"));
}
