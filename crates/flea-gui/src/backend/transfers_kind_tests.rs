//! Each held transfer keeps its own kind, so the refusal names it, and a send holds its own paths until its job names them.
use super::super::{Kind, Transfers};

fn on(kept: &Transfers, volume: &str) -> Option<Kind> {
    kept.touch_where(&|path| flea::ui_logic_device_rail::mounts::inside(path, volume))
}

#[test]
fn each_command_holds_under_its_own_kind() {
    let lines = [
        (
            r#"{"c":"transfer","op":"copy","paths":["/media/s/a"],"dest":"/home/u"}"#,
            Kind::Copy,
        ),
        (
            r#"{"c":"transfer","op":"move","paths":["/media/s/a"],"dest":"/home/u"}"#,
            Kind::Move,
        ),
        (
            r#"{"c":"transfer","paths":["/media/s/a"],"dest":"/home/u"}"#,
            Kind::Copy,
        ),
        (
            r#"{"c":"archive","op":"extract","path":"/media/s/x.zip","dest":"/home/u"}"#,
            Kind::Archive,
        ),
        (
            r#"{"c":"actions","op":"run","id":5,"action":"action:a.toml","paths":["/media/s/a"]}"#,
            Kind::Action,
        ),
        (
            r#"{"c":"localsend","op":"send","peer":"Desk","paths":["/media/s/a"],"id":3}"#,
            Kind::Send,
        ),
        (
            r#"{"c":"airdrop","op":"send","id":4,"peer":"3a9f","name":"P","paths":["/media/s/a"]}"#,
            Kind::Send,
        ),
        (
            r#"{"c":"rclone","op":"send","id":72,"remote":"g","folder":"","paths":["/media/s/a"]}"#,
            Kind::Send,
        ),
    ];
    for (line, kind) in lines {
        let mut kept = Transfers::default();
        kept.sent(line, None);
        assert_eq!(on(&kept, "/media/s"), Some(kind), "{line}");
    }
    let mut kept = Transfers::default();
    kept.sent(r#"{"c":"localsend","op":"peers","id":3}"#, None);
    kept.sent(r#"{"c":"rclone","op":"remotes","id":71}"#, None);
    assert_eq!(on(&kept, "/media/s"), None, "only a send is kept");
}

#[test]
fn a_roots_line_names_the_kind_its_job_or_drop_holds_under() {
    let mut kept = Transfers::default();
    kept.sent(
        r#"{"c":"transfer","op":"","paths":[],"dest":"/home/u","shelf":"9f2c"}"#,
        None,
    );
    kept.answered(r#"{"t":"transferstarted","id":13,"n":1,"moving":true}"#);
    assert_eq!(
        on(&kept, "/home/u"),
        Some(Kind::Move),
        "a drop's start says it moves"
    );
    kept.answered(
        r#"{"t":"roots","id":13,"job":false,"kind":"move","paths":["/media/s/a.jpg","/home/u"]}"#,
    );
    assert_eq!(on(&kept, "/media/s"), Some(Kind::Move));
    for (kind, held) in [
        ("compress", Kind::Archive),
        ("extract", Kind::Archive),
        ("send", Kind::Send),
        ("action", Kind::Action),
        ("copy", Kind::Copy),
    ] {
        let mut kept = Transfers::default();
        kept.answered(&format!(
            r#"{{"t":"roots","id":12,"job":true,"kind":"{kind}","paths":["/media/s"]}}"#
        ));
        assert_eq!(on(&kept, "/media/s"), Some(held), "{kind}");
    }
    let mut kept = Transfers::default();
    kept.answered(r#"{"t":"roots","id":12,"job":true,"paths":["/media/s"]}"#);
    assert_eq!(
        on(&kept, "/media/s"),
        Some(Kind::Copy),
        "an older backend's line reads as a copy"
    );
}

#[test]
fn a_send_holds_its_request_paths_until_its_own_roots_line() {
    let mut kept = Transfers::default();
    kept.sent(
        r#"{"c":"localsend","op":"send","peer":"Desk","paths":["/media/s/a"],"id":3}"#,
        None,
    );
    kept.sent(
        r#"{"c":"airdrop","op":"send","id":3,"peer":"3a9f","name":"P","paths":["/media/t/b"]}"#,
        None,
    );
    kept.answered(r#"{"t":"roots","id":20,"job":true,"kind":"send","paths":["/home/u/x"]}"#);
    assert!(
        kept.touch("/media/s"),
        "a retry's roots line names no request"
    );
    kept.answered(
        r#"{"t":"roots","id":21,"job":true,"kind":"send","requestId":3,"paths":["/home/u/a"]}"#,
    );
    assert!(
        !kept.touch("/media/s"),
        "the oldest send of that request gave way to its job"
    );
    assert!(kept.touch("/home/u"), "its job's own paths hold now");
    assert!(
        kept.touch("/media/t"),
        "the newer send of the same number still waits"
    );
    kept.answered(
        r#"{"t":"roots","id":22,"job":true,"kind":"send","requestId":3,"paths":["/media/t/b"]}"#,
    );
    kept.answered(r#"{"t":"jobs","jobs":[{"id":21,"kind":"send","state":"done"},{"id":22,"kind":"send","state":"done"}],"unfinished":0}"#);
    assert!(!kept.touch("/media/t") && !kept.touch("/home/u"));
}

#[test]
fn a_refused_rclone_send_lets_go_and_a_started_one_waits_for_its_roots() {
    let mut kept = Transfers::default();
    let send =
        r#"{"c":"rclone","op":"send","id":72,"remote":"g","folder":"","paths":["/media/s/a"]}"#;
    kept.sent(send, None);
    kept.answered(r#"{"t":"rclonestarted","requestId":71,"ok":false,"error":"x"}"#);
    assert!(kept.touch("/media/s"), "another request's refusal");
    kept.answered(r#"{"t":"rclonestarted","requestId":72,"ok":false,"error":"x"}"#);
    assert!(!kept.touch("/media/s"));
    kept.sent(send, None);
    kept.answered(r#"{"t":"rclonestarted","requestId":72,"id":22,"ok":true,"error":""}"#);
    assert!(kept.touch("/media/s"));
    kept.answered(
        r#"{"t":"roots","id":22,"job":true,"kind":"send","requestId":72,"paths":["/media/s/a"]}"#,
    );
    kept.answered(r#"{"t":"job","job":{"id":22,"kind":"send","state":"failed"},"unfinished":0}"#);
    assert!(
        !kept.touch("/media/s"),
        "a failed send holds nothing until retried"
    );
}
