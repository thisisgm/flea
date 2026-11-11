use super::Transfers;

const ONTO: &str = r#"{"c":"transfer","op":"copy","paths":["/home/u/a"],"dest":"/media/s/in"}"#;
const ELSEWHERE: &str = r#"{"c":"transfer","op":"copy","paths":["/home/a"],"dest":"/home/b"}"#;
const STARTED: &str = r#"{"t":"transferstarted","id":12,"n":1,"moving":false}"#;
const DONE: &str =
    r#"{"t":"transferdone","id":12,"ok":1,"failed":0,"skipped":0,"cancelled":false}"#;
const IDLE: &str = r#"{"t":"jobs","jobs":[],"unfinished":0,"newest":null,"head":""}"#;
const BUSY: &str =
    r#"{"t":"error","where":"transfer","path":"","msg":"an operation is already running"}"#;

#[test]
fn a_transfer_names_its_sources_and_destination_until_it_ends() {
    let mut kept = Transfers::default();
    kept.sent(r#"{"c":"thumb","path":"/media/s/a.jpg"}"#, None);
    assert!(
        !kept.touch("/media/s"),
        "only transfers and archives are kept"
    );
    kept.sent(ONTO, None);
    assert!(kept.touch("/media/s"), "sent and not yet answered");
    kept.answered(IDLE);
    assert!(
        kept.touch("/media/s"),
        "an idle line written before the backend read the transfer ends nothing"
    );
    kept.answered(STARTED);
    assert!(kept.touch("/media/s") && kept.touch("/home/u"));
    assert!(!kept.touch("/media/st"));
    kept.answered(r#"{"t":"transferdone","id":9,"ok":1}"#);
    assert!(
        kept.touch("/media/s"),
        "another job's end ends nothing here"
    );
    kept.answered(DONE);
    assert!(!kept.touch("/media/s"), "its own end lets it go");
    kept.sent(
        r#"{"c":"transfer","op":"move","menuId":4,"dest":"/home/u"}"#,
        Some("/media/s/dcim"),
    );
    assert!(
        kept.touch("/media/s"),
        "a menu's rows come from the listed folder"
    );
}

#[test]
fn a_refusal_lets_go_of_the_oldest_unanswered_transfer_only() {
    let mut kept = Transfers::default();
    kept.sent(ONTO, None);
    kept.sent(
        r#"{"c":"transfer","op":"copy","paths":["/home/a"],"dest":"/media/t"}"#,
        None,
    );
    kept.answered(BUSY);
    assert!(!kept.touch("/media/s"), "the refusal answers the oldest");
    assert!(kept.touch("/media/t"), "the newer one is still unread");
    kept.answered(r#"{"t":"error","where":"stale","path":"transfer","msg":"x"}"#);
    assert!(!kept.touch("/media/t"), "a stale-row refusal is a refusal");
    kept.sent(ONTO, None);
    kept.answered(r#"{"t":"error","where":"rename","path":"/home/a","msg":"x"}"#);
    assert!(
        kept.touch("/media/s"),
        "another command's error refuses nothing"
    );
    kept.answered(r#"{"t":"error","where":"transfer","path":"/home/u/b.txt","msg":"x"}"#);
    assert!(
        kept.touch("/media/s"),
        "an undo failing under transfer names its own path"
    );
    kept.answered(
        r#"{"t":"error","where":"transfer","path":"/media/s/in","msg":"/media/s is not responding."}"#,
    );
    assert!(
        !kept.touch("/media/s"),
        "a refusal naming the destination is this one's"
    );
}

#[test]
fn a_backend_that_exited_holds_nothing() {
    let mut kept = Transfers::default();
    kept.sent(ONTO, None);
    kept.answered(STARTED);
    kept.stop();
    assert!(!kept.touch("/media/s"), "its copy died with it");
    kept.sent(ONTO, None);
    assert!(
        !kept.touch("/media/s"),
        "a line sent after the exit never runs"
    );
}

#[test]
fn a_start_pairs_with_its_own_command_kind() {
    let mut kept = Transfers::default();
    kept.sent(
        r#"{"c":"archive","op":"extract","path":"/home/u/x.zip","dest":"/media/t"}"#,
        None,
    );
    kept.sent(ONTO, None);
    kept.answered(STARTED);
    kept.answered(r#"{"t":"archivestarted","id":13}"#);
    kept.answered(DONE);
    assert!(
        !kept.touch("/media/s"),
        "the copy's start and end were its own"
    );
    assert!(kept.touch("/media/t"), "the extract still runs");
    kept.answered(r#"{"t":"archivedone","id":13,"ok":true,"verified":true,"err":""}"#);
    assert!(!kept.touch("/media/t"));
    kept.sent(
        r#"{"c":"archive","op":"extract","path":"/media/t/disk.iso","dest":"/home/u"}"#,
        None,
    );
    kept.sent(ONTO, None);
    kept.answered(r#"{"t":"transferstarted","id":14,"n":1,"moving":false}"#);
    kept.answered(r#"{"t":"archivedone","id":14,"ok":false,"verified":true,"err":"x"}"#);
    assert!(!kept.touch("/media/t"), "a refused disk image never starts");
    assert!(
        kept.touch("/media/s"),
        "an archive's end never ends a transfer"
    );
}

#[test]
fn a_long_paste_never_pushes_out_a_transfer_that_still_runs() {
    let mut kept = Transfers::default();
    kept.sent(
        r#"{"c":"transfer","op":"copy","paths":["/media/s/dcim"],"dest":"/home/u"}"#,
        None,
    );
    let many: Vec<String> = (0..300).map(|n| format!("\"/home/a/{n}\"")).collect();
    let paste = format!(
        r#"{{"c":"transfer","op":"copy","paths":[{}],"dest":"/home/b"}}"#,
        many.join(",")
    );
    kept.sent(&paste, None);
    kept.answered(STARTED);
    kept.answered(r#"{"t":"transferstarted","id":13,"n":300,"moving":false}"#);
    kept.answered(r#"{"t":"transferdone","id":13,"ok":300}"#);
    kept.sent(ELSEWHERE, None);
    assert!(kept.touch("/media/s"));
}

const ROOTS: &str = r#"{"t":"roots","id":12,"job":true,"paths":["/home/u/a","/media/s/in"]}"#;

fn job(id: usize, state: &str) -> String {
    format!(
        r#"{{"t":"job","job":{{"id":{id},"kind":"copy","head":"","state":"{state}","output":[]}},"unfinished":0}}"#
    )
}

#[test]
fn a_menu_copy_holds_the_drive_its_rows_live_on_once_the_backend_names_them() {
    let mut kept = Transfers::default();
    kept.sent(
        r#"{"c":"transfer","op":"copy","menuId":4,"dest":"/home/u"}"#,
        Some("/home/u/found"),
    );
    kept.answered(STARTED);
    assert!(!kept.touch("/media/s"), "the listed folder is only a guess");
    assert!(kept
        .answered(r#"{"t":"roots","id":12,"job":true,"paths":["/home/u","/media/s/dcim/a.jpg"]}"#));
    assert!(kept.touch("/media/s"), "a search result's own folder");
    assert!(!kept.touch("/home/u/found"), "the guess gives way");
    kept.answered(&job(12, "running"));
    kept.answered(DONE);
    kept.answered(&job(12, "done"));
    assert!(!kept.touch("/media/s"));
}

#[test]
fn a_failed_job_lets_go_and_its_retry_holds_again() {
    let mut kept = Transfers::default();
    kept.sent(ONTO, None);
    kept.answered(STARTED);
    kept.answered(ROOTS);
    kept.answered(&job(12, "running"));
    kept.answered(r#"{"t":"transferdone","id":12,"ok":0,"failed":1}"#);
    kept.answered(&job(12, "failed"));
    assert!(!kept.touch("/media/s"), "a failed job writes nothing");
    kept.sent(r#"{"c":"jobs","op":"retry","id":12}"#, None);
    assert!(!kept.touch("/media/s"), "a retry is no transfer line");
    kept.answered(r#"{"t":"roots","id":12,"job":true,"paths":["/home/u/a","/media/s/in"]}"#);
    assert!(kept.touch("/media/s"), "the retry names its paths again");
    kept.answered(&job(12, "waiting"));
    assert!(
        kept.touch("/media/s"),
        "queued behind another job still holds"
    );
    kept.answered(&job(12, "failed"));
    kept.answered(r#"{"t":"jobs","jobs":[{"id":9,"kind":"copy","state":"done"}],"unfinished":0}"#);
    kept.answered(&job(12, "running"));
    assert!(!kept.touch("/media/s"), "a dismissed job is gone for good");
}

#[test]
fn a_shelf_drop_holds_its_entries_until_its_transfer_ends() {
    let mut kept = Transfers::default();
    kept.sent(
        r#"{"c":"transfer","op":"","paths":[],"dest":"/home/u","shelf":"9f2c"}"#,
        None,
    );
    kept.answered(r#"{"t":"transferstarted","id":13,"n":1,"moving":true}"#);
    assert!(!kept.touch("/media/s"), "the token names no entry");
    kept.answered(r#"{"t":"roots","id":13,"job":false,"paths":["/media/s/a.jpg","/home/u"]}"#);
    assert!(kept.touch("/media/s"));
    kept.answered(r#"{"t":"jobs","jobs":[],"unfinished":0,"newest":null,"head":""}"#);
    kept.answered(&job(13, "done"));
    assert!(
        kept.touch("/media/s"),
        "a drop is no job, so job lines never end it"
    );
    kept.answered(r#"{"t":"transferdone","id":13,"ok":1,"failed":0}"#);
    assert!(!kept.touch("/media/s"));
}

#[test]
fn an_action_run_holds_its_paths_until_its_job_names_them() {
    let mut kept = Transfers::default();
    let run = r#"{"c":"actions","op":"run","id":5,"action":"action:resize.toml","paths":["/media/s/a.jpg"]}"#;
    kept.sent(
        r#"{"c":"actions","op":"list","id":4,"paths":["/media/s/a.jpg"]}"#,
        None,
    );
    assert!(!kept.touch("/media/s"), "a list reads nothing");
    kept.sent(run, None);
    assert!(kept.touch("/media/s"));
    kept.answered(r#"{"t":"actionstarted","requestId":5,"id":13,"ok":true,"error":""}"#);
    assert!(
        kept.touch("/media/s"),
        "a started run holds until its roots line"
    );
    kept.answered(r#"{"t":"roots","id":13,"job":true,"paths":["/media/s/a.jpg"]}"#);
    assert!(kept.touch("/media/s"));
    kept.answered(&job(13, "done"));
    assert!(!kept.touch("/media/s"));
    kept.sent(run, None);
    kept.answered(r#"{"t":"actionstarted","requestId":5,"ok":false,"error":"x"}"#);
    assert!(!kept.touch("/media/s"), "a refused run holds nothing");
    kept.sent(run, None);
    kept.answered(r#"{"t":"actionstarted","requestId":5,"ok":true,"error":""}"#);
    assert!(!kept.touch("/media/s"), "a terminal run starts no job");
}

#[test]
fn an_archive_the_backend_named_still_ends_by_its_own_done_line() {
    let mut kept = Transfers::default();
    kept.sent(
        r#"{"c":"archive","op":"extract","path":"/home/u/x.zip","dest":"/home/u"}"#,
        None,
    );
    kept.answered(r#"{"t":"archivestarted","id":13}"#);
    kept.answered(r#"{"t":"roots","id":13,"job":true,"paths":["/home/u","/home/u/x.zip"]}"#);
    kept.sent(
        r#"{"c":"archive","op":"extract","path":"/home/u/y.zip","dest":"/media/t"}"#,
        None,
    );
    kept.answered(r#"{"t":"archivedone","id":13,"ok":true,"verified":true,"err":""}"#);
    assert!(
        kept.touch("/media/t"),
        "the queued extract is not the one that ended"
    );
}

#[test]
fn a_line_a_plugin_place_took_waits_for_that_place_alone() {
    let mut kept = Transfers::default();
    let routed = r#"{"c":"transfer","op":"copy","paths":["plugin://p/a"],"dest":"/media/s"}"#;
    let seq = kept.sent(routed, None).unwrap();
    kept.placed(seq, Ok(true));
    kept.sent(ELSEWHERE, None);
    kept.answered(STARTED);
    kept.answered(BUSY);
    assert!(
        kept.touch("/media/s"),
        "the backend's answers are never the place's"
    );
    assert!(
        kept.touch("/home/b"),
        "the backend's start paired with its own line"
    );
    kept.routed_done(r#"{"c":"list","path":"plugin://p/"}"#);
    assert!(kept.touch("/media/s"), "a list answers no transfer");
    kept.routed_done(routed);
    assert!(!kept.touch("/media/s"));
    let lost = kept.sent(ONTO, None).unwrap();
    kept.placed(lost, Err(()));
    assert!(
        !kept.touch("/media/s"),
        "a line that never left holds nothing"
    );
}

#[test]
fn a_backend_plugin_route_records_the_line_it_takes() {
    let (backend, _lines) = super::Backend::test_lines();
    let (route, requests, _output) = backend.plugin_route().unwrap();
    let line = r#"{"c":"transfer","op":"copy","paths":["plugin://p/a"],"dest":"/media/s"}"#;
    backend.line(line.into()).unwrap();
    assert_eq!(requests.try_recv().unwrap().1, line, "the place took it");
    backend.test_answered(BUSY);
    assert!(backend.transfers_on("/media/s"));
    route.answered(line);
    assert!(!backend.transfers_on("/media/s"));
}

#[path = "transfers_kind_tests.rs"]
mod kind;
