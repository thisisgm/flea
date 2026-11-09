//! Review round 2 probes for g2-sd2, kept as tests: the FIX1 delta against the int4 producer's real line order at af7111f09.
use flea::i18n::{I18n, Paths};
use flea_gui::g2::rclone_send::seams::Wire;
use flea_gui::g2::rclone_send::RcloneSend;
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

#[derive(Default)]
struct Fake {
    lines: RefCell<Vec<String>>,
}

impl Wire for Fake {
    fn send(&self, line: String) -> bool {
        self.lines.borrow_mut().push(line);
        true
    }
}

impl Fake {
    fn last(&self) -> String {
        self.lines.borrow().last().cloned().unwrap_or_default()
    }
}

struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-sd2");
        std::fs::create_dir_all(&parent).unwrap();
        let output = Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("r2.XXXXXXXX"))
            .output()
            .unwrap();
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute());
        std::fs::write(root.join(".flea-test-sandbox"), "g2-sd2-r2\n").unwrap();
        Self(root)
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn i18n(&self) -> I18n {
        self.write("omarchy/default/i18n/locales.json", r#"{"en":{}}"#);
        self.write(
            "omarchy/default/i18n/en/shell.json",
            r#"{"common.cancel":"Cancel"}"#,
        );
        self.write("language", "en");
        I18n::load(Paths {
            omarchy: self.0.join("omarchy"),
            language_file: self.0.join("language"),
            catalogs: vec![self.0.join("catalog")],
        })
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let path = self.0.to_string_lossy();
        if self.0.is_absolute()
            && path.contains("/.superpowers/g2-sd2/r2.")
            && self.0.join(".flea-test-sandbox").is_file()
        {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn paths() -> Vec<String> {
    [
        "/fixture/DSC_4821.jpg",
        "/fixture/DSC_4822.jpg",
        "/fixture/DSC_4930.jpg",
    ]
    .iter()
    .map(|path| path.to_string())
    .collect()
}

fn id_of(line: &str) -> u64 {
    let at = line.find("\"id\":").unwrap() + 5;
    line[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}

fn remotes(send: &mut RcloneSend, fake: &Fake) {
    send.menu_opened(fake);
    let id = id_of(&fake.last());
    assert!(send.on_line(&format!(r#"{{"t":"rcloneremotes","id":{id},"visible":true,"remotes":["gdrive","b2-photos","nas-crypt"]}}"#)));
}

// Job::json at af7111f09 src/backend/jobview.rs, every field in its order.
fn row(
    id: u64,
    state: &str,
    phase: &str,
    check: &str,
    done: usize,
    total: usize,
    reason: &str,
) -> String {
    format!(
        r#"{{"id":{id},"kind":"send","head":"Sending","file":"","done":{done},"total":{total},"bytes":0,"totalBytes":0,"itemBytes":0,"rate":0,"left":null,"state":"{state}","reason":"{reason}","output":[],"undo":false,"cancelled":false,"phase":"{phase}","check":"{check}","receipt":"","note":""}}"#
    )
}

fn update(row: &str) -> String {
    format!(r#"{{"t":"job","job":{row},"unfinished":1}}"#)
}

fn snapshot(rows: &[String]) -> String {
    format!(
        r#"{{"t":"jobs","jobs":[{}],"unfinished":1,"newest":null,"head":""}}"#,
        rows.join(",")
    )
}

fn submit(send: &mut RcloneSend, fake: &Fake, job: u64) -> u64 {
    send.remember("gdrive", "Uploads");
    assert!(send.choose("gdrive", paths()));
    assert!(send.submit(fake));
    let request = id_of(&fake.last());
    assert!(send.on_line(&format!(
        r#"{{"t":"rclonestarted","requestId":{request},"id":{job},"ok":true,"error":""}}"#
    )));
    request
}

// JobDone's order: the job update, advance's snapshot, then the rclonesent line (jobevents.rs observe, then the reporter).
fn fail(send: &mut RcloneSend, job: u64, request: u64, done: usize, total: usize) {
    let reason = "Could not send DSC_4822.jpg to gdrive.";
    let failed = row(job, "failed", "finished", "none", done, total, reason);
    send.on_line(&update(&failed));
    send.on_line(&snapshot(std::slice::from_ref(&failed)));
    send.on_line(&format!(
        r#"{{"t":"rclonesent","id":{job},"requestId":{request},"ok":false,"cancelled":false,"check":"none","receipt":"","note":"","error":"{reason}"}}"#
    ));
}

#[test]
fn r2_an_accepted_retry_runs_to_its_own_receipt() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let request = submit(&mut send, &fake, 12);
    fail(&mut send, 12, request, 2, 3);
    assert!(send.retry(request, &fake));
    assert_eq!(fake.last(), r#"{"c":"jobs","op":"retry","id":12}"#);
    // jobevents.rs "retry" Ok: advance starts it (update running, phase cleared, total = failed items), then snapshots.
    let running = row(12, "running", "", "", 0, 1, "");
    assert!(send.on_line(&update(&running)));
    send.on_line(&snapshot(std::slice::from_ref(&running)));
    let block = send.blocks(&i18n)[0].1.clone();
    eprintln!(
        "accepted retry: head={:?} cancel={:?} retry={:?} dismiss={:?}",
        block.head, block.cancel, block.retry, block.dismiss
    );
    assert_eq!(
        (block.head.as_str(), block.cancel.clone(), block.failed),
        (
            "Sending 1 of 1 to gdrive",
            Some(("Cancel".into(), true)),
            false
        )
    );
    send.on_line(&update(&row(
        12, "running", "checking", "checksum", 1, 1, "",
    )));
    assert!(send.sweep(request).is_some());
    send.on_line(&update(&row(12, "done", "checked", "checksum", 1, 1, "")));
    assert!(send.on_line(&format!(
        r#"{{"t":"rclonesent","id":12,"requestId":{request},"ok":true,"cancelled":false,"check":"checksum","receipt":"Sent 1 item to Uploads on gdrive","note":"","error":""}}"#
    )));
    let receipt = send.receipt(&i18n).unwrap();
    eprintln!("retry receipt: {:?} {:?}", receipt.text, receipt.hint);
    assert_eq!(
        (receipt.text.as_str(), receipt.hint.as_str()),
        ("Sent 1 item to Uploads on gdrive", " · checked by checksum")
    );
    assert!(send.blocks(&i18n).is_empty() && send.sweep(request).is_none());
}

#[test]
fn r2_a_retry_cancelled_while_waiting_leaves_nothing() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let request = submit(&mut send, &fake, 12);
    fail(&mut send, 12, request, 0, 3);
    assert!(send.retry(request, &fake));
    // Blocked behind an unprepared older job: advance writes a waiting row with its reason.
    assert!(send.on_line(&update(&row(
        12,
        "waiting",
        "",
        "",
        0,
        3,
        "waiting for the copy to Photos"
    ))));
    assert!(send.cancel(request, &fake));
    // jobs.rs advance on a cancelled waiting job: the cancelled rclonesent first, then the done update.
    assert!(send.on_line(&format!(
        r#"{{"t":"rclonesent","id":12,"requestId":{request},"ok":false,"cancelled":true,"check":"none","receipt":"","note":"","error":""}}"#
    )));
    send.on_line(&update(&row(12, "done", "", "", 0, 3, "")));
    assert!(send.blocks(&i18n).is_empty() && send.receipt(&i18n).is_none());
}

#[test]
fn r2_a_retry_in_flight_when_the_backend_dies_can_be_dismissed() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let request = submit(&mut send, &fake, 12);
    fail(&mut send, 12, request, 2, 3);
    assert!(send.retry(request, &fake));
    assert!(send.backend_ended());
    let block = send.blocks(&i18n)[0].1.clone();
    assert_eq!(
        (block.retry, block.dismiss.as_deref(), block.cancel),
        (None, Some("Dismiss"), None)
    );
    let before = fake.lines.borrow().len();
    assert!(send.dismiss(request, &fake));
    assert_eq!(
        fake.lines.borrow().len(),
        before,
        "no request to a backend that never knew the send"
    );
    assert!(send.blocks(&i18n).is_empty());
}

#[test]
fn r2_the_cache_follows_every_line_that_changes_a_block() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let request = submit(&mut send, &fake, 12);
    let first = send.blocks(&i18n);
    assert!(send.on_line(&update(&row(12, "running", "sending", "none", 1, 3, ""))));
    let second = send.blocks(&i18n);
    assert!(!Rc::ptr_eq(&first, &second));
    assert_eq!(second[0].1.head, "Sending 2 of 3 to gdrive");
    send.on_line(&update(&row(12, "running", "checking", "size", 3, 3, "")));
    assert_eq!(send.blocks(&i18n)[0].1.file, "comparing sizes");
    assert!(send.cancel(request, &fake));
    assert_eq!(
        send.blocks(&i18n)[0].1.cancel,
        Some(("Cancelling".into(), false))
    );
    assert!(send.backend_ended());
    assert_eq!(send.blocks(&i18n)[0].1.head, "Send failed");
    assert!(send.dismiss(request, &fake));
    assert!(send.blocks(&i18n).is_empty());
}

#[test]
fn r2_two_checking_sends_keep_two_sweeps_by_request() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let first = submit(&mut send, &fake, 3);
    let second = submit(&mut send, &fake, 4);
    send.on_line(&update(&row(
        3, "running", "checking", "checksum", 3, 3, "",
    )));
    send.on_line(&update(&row(4, "running", "checking", "size", 3, 3, "")));
    let (a, b) = (send.sweep(first).unwrap(), send.sweep(second).unwrap());
    assert!(!Rc::ptr_eq(&a, &b));
    let keys: Vec<u64> = send.blocks(&i18n).iter().map(|(key, _)| *key).collect();
    assert_eq!(keys, [second, first]);
    assert!(send.cancel(first, &fake));
    assert_eq!(fake.last(), r#"{"c":"jobs","op":"cancel","id":3}"#);
}

// A snapshot the backend wrote before it read the retry still shows the job failed; it must not bring Retry or Dismiss back.
#[test]
fn r2_a_stale_failed_snapshot_during_a_retry_offers_no_button() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let request = submit(&mut send, &fake, 12);
    fail(&mut send, 12, request, 2, 3);
    assert!(send.retry(request, &fake));
    let stale = row(
        12,
        "failed",
        "finished",
        "none",
        2,
        3,
        "Could not send DSC_4822.jpg to gdrive.",
    );
    send.on_line(&update(&stale));
    send.on_line(&snapshot(std::slice::from_ref(&stale)));
    let block = send.blocks(&i18n)[0].1.clone();
    assert_eq!(
        (block.failed, block.retry, block.dismiss),
        (true, None, None)
    );
    let before = fake.lines.borrow().len();
    assert!(!send.retry(request, &fake) && !send.dismiss(request, &fake));
    assert_eq!(fake.lines.borrow().len(), before);
    // The retried job's first row still lands, so the send runs on.
    assert!(send.on_line(&update(&row(12, "running", "", "", 0, 1, ""))));
    assert_eq!(
        send.blocks(&i18n)[0].1.cancel,
        Some(("Cancel".into(), true))
    );
}

// A refused retry after a stale snapshot still restores Retry and Dismiss.
#[test]
fn r2_a_refused_retry_after_a_stale_snapshot_offers_both_again() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let request = submit(&mut send, &fake, 12);
    fail(&mut send, 12, request, 2, 3);
    assert!(send.retry(request, &fake));
    send.on_line(&snapshot(&[row(
        12,
        "failed",
        "finished",
        "none",
        2,
        3,
        "Could not send DSC_4822.jpg to gdrive.",
    )]));
    assert!(send.on_line(r#"{"t":"joberror","id":12,"error":"Nothing to retry."}"#));
    let block = send.blocks(&i18n)[0].1.clone();
    assert!(block.retry.is_some() && block.dismiss.is_some());
}

// The window keeps one RcloneSend per pane backend; both number their jobs from 1, and neither instance sees the other's lines.
#[test]
fn r2_two_pane_instances_with_the_same_job_id_never_touch_each_other() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut left, left_wire) = (RcloneSend::default(), Fake::default());
    let (mut right, right_wire) = (RcloneSend::default(), Fake::default());
    remotes(&mut left, &left_wire);
    remotes(&mut right, &right_wire);
    let left_key = submit(&mut left, &left_wire, 12);
    let right_key = submit(&mut right, &right_wire, 12);
    assert!(left.on_line(&update(&row(
        12,
        "failed",
        "finished",
        "none",
        0,
        3,
        "Could not send DSC_4821.jpg to gdrive."
    ))));
    assert!(right.on_line(&update(&row(
        12, "running", "checking", "checksum", 3, 3, ""
    ))));
    assert_eq!(left.blocks(&i18n)[0].1.head, "Send failed");
    assert!(left.sweep(left_key).is_none());
    assert_eq!(right.blocks(&i18n)[0].1.file, "comparing checksums");
    assert!(right.sweep(right_key).is_some());
    assert!(left.retry(left_key, &left_wire));
    assert_eq!(left_wire.last(), r#"{"c":"jobs","op":"retry","id":12}"#);
    assert!(!right_wire
        .lines
        .borrow()
        .iter()
        .any(|line| line.contains(r#""op":"retry""#)));
    assert!(left.backend_ended());
    assert_eq!(right.blocks(&i18n)[0].1.file, "comparing checksums");
    assert!(right.sweep(right_key).is_some());
    assert!(right.cancel(right_key, &right_wire));
    assert_eq!(right_wire.last(), r#"{"c":"jobs","op":"cancel","id":12}"#);
    assert_eq!(left.blocks(&i18n)[0].1.cancel, None);
}
