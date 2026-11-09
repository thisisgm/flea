//! Review round 1 probes for g2-sd2, kept as regressions; keys are the block keys `blocks` hands out.
use flea::i18n::{I18n, Paths};
use flea_gui::g2::rclone_send::seams::Wire;
use flea_gui::g2::rclone_send::RcloneSend;
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;

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
            .arg(parent.join("review.XXXXXXXX"))
            .output()
            .unwrap();
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute());
        std::fs::write(root.join(".flea-test-sandbox"), "g2-sd2 review\n").unwrap();
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
            && path.contains("/.superpowers/g2-sd2/review.")
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

// The producer's job line shape, src/backend/jobview.rs Job::json at af7111f09.
fn job(id: u64, state: &str, phase: &str, check: &str, done: usize, reason: &str) -> String {
    format!(
        r#"{{"t":"job","job":{{"id":{id},"kind":"send","head":"x","file":"","done":{done},"total":3,"bytes":0,"totalBytes":0,"itemBytes":0,"rate":0,"left":null,"state":"{state}","reason":"{reason}","output":[],"undo":false,"cancelled":false,"phase":"{phase}","check":"{check}","receipt":"","note":""}},"unfinished":0}}"#
    )
}

fn submit(send: &mut RcloneSend, fake: &Fake) -> u64 {
    send.remember("gdrive", "Uploads");
    assert!(send.choose("gdrive", paths()));
    assert!(send.submit(fake));
    id_of(&fake.last())
}

fn failed(send: &mut RcloneSend, job_id: u64, request: u64) {
    let reason = "DSC_4930.jpg did not match on gdrive.";
    assert!(send.on_line(&job(job_id, "failed", "finished", "none", 2, reason)));
    assert!(send.on_line(&format!(
        r#"{{"t":"rclonesent","id":{job_id},"requestId":{request},"ok":false,"cancelled":false,"check":"none","receipt":"","note":"","error":"{reason}"}}"#
    )));
}

#[test]
fn review_a_refused_retry_returns_the_send_to_failed() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let request = submit(&mut send, &fake);
    assert!(send.on_line(&format!(
        r#"{{"t":"rclonestarted","requestId":{request},"id":12,"ok":true,"error":""}}"#
    )));
    failed(&mut send, 12, request);
    assert!(send.retry(request, &fake));
    // src/backend/jobevents.rs request "retry" answers this when Work::retry refuses a changed or missing source.
    send.on_line(
        r#"{"t":"joberror","id":12,"error":"A failed item changed; it was not retried."}"#,
    );
    let (_, block) = send.blocks(&i18n)[0].clone();
    eprintln!(
        "after refused retry: head={:?} cancel={:?} retry={:?} dismiss={:?} failed={}",
        block.head, block.cancel, block.retry, block.dismiss, block.failed
    );
    // The producer's jobs cancel never answers a failed job, so a Cancel press here would wait forever.
    let cancel_sent = send.cancel(request, &fake);
    let (_, after) = send.blocks(&i18n)[0].clone();
    eprintln!(
        "cancel accepted={cancel_sent} then: head={:?} cancel={:?} dismiss={:?}",
        after.head, after.cancel, after.dismiss
    );
    assert!(
        block.failed && block.dismiss.is_some(),
        "a refused retry left the send marked running with no way out"
    );
}

#[test]
fn review_a_failed_send_from_a_dead_backend_never_addresses_the_next_backend() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let first = submit(&mut send, &fake);
    // Job ids restart at 1 in every backend (src/backend/opsdispatch.rs next_id: 1).
    assert!(send.on_line(&format!(
        r#"{{"t":"rclonestarted","requestId":{first},"id":1,"ok":true,"error":""}}"#
    )));
    failed(&mut send, 1, first);
    let ended = send.backend_ended();
    let (_, lost) = send.blocks(&i18n)[0].clone();
    eprintln!(
        "backend_ended changed={ended}; old block retry={:?} dismiss={:?}",
        lost.retry, lost.dismiss
    );
    // The next backend: a new send is job 1 there too.
    remotes(&mut send, &fake);
    let second = submit(&mut send, &fake);
    assert!(send.on_line(&format!(
        r#"{{"t":"rclonestarted","requestId":{second},"id":1,"ok":true,"error":""}}"#
    )));
    send.on_line(&job(1, "running", "sending", "none", 1, ""));
    send.on_line(&job(1, "running", "checking", "checksum", 3, ""));
    send.on_line(&job(1, "done", "checked", "checksum", 3, ""));
    send.on_line(&format!(
        r#"{{"t":"rclonesent","id":1,"requestId":{second},"ok":true,"cancelled":false,"check":"checksum","receipt":"Sent 3 items to Uploads on gdrive","note":"","error":""}}"#
    ));
    let blocks: Vec<_> = send
        .blocks(&i18n)
        .iter()
        .cloned()
        .map(|(id, block)| (id, block.head, block.cancel, block.retry))
        .collect();
    eprintln!(
        "after the second send ended: blocks={blocks:?} sweep(second)={}",
        send.sweep(second).is_some()
    );
    assert!(
        lost.retry.is_none(),
        "a dead backend's job id is offered for Retry on the next backend"
    );
    assert!(
        blocks.iter().all(|(_, _, cancel, _)| cancel.is_none()),
        "the old send took the new backend's job 1 and stays running"
    );
}

#[test]
fn review_a_send_lost_before_its_job_id_can_be_dismissed() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    submit(&mut send, &fake);
    assert!(send.backend_ended());
    let blocks = send.blocks(&i18n);
    assert_eq!(blocks.len(), 1);
    let (key, block) = (blocks[0].0, &blocks[0].1);
    eprintln!(
        "lost jobless block key={key} head={:?} dismiss={:?}",
        block.head, block.dismiss
    );
    let dismissed = send.dismiss(key, &fake);
    eprintln!(
        "dismiss({key}) returned {dismissed}; blocks left {}",
        send.blocks(&i18n).len()
    );
    assert!(
        dismissed && send.blocks(&i18n).is_empty(),
        "the lost block's Dismiss cannot reach it"
    );
}

#[test]
fn review_block_keys_never_collide() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let first = submit(&mut send, &fake);
    assert!(send.on_line(&format!(
        r#"{{"t":"rclonestarted","requestId":{first},"id":{},"ok":true,"error":""}}"#,
        first + 1
    )));
    // The next send is still waiting for rclonestarted; its key is its request id, the same number as the first send's job id.
    submit(&mut send, &fake);
    let keys: Vec<u64> = send.blocks(&i18n).iter().map(|(id, _)| *id).collect();
    eprintln!("block keys {keys:?}");
    assert!(
        keys.windows(2).all(|pair| pair[0] != pair[1]),
        "two blocks share key {keys:?}"
    );
}

#[test]
fn review_a_new_card_never_shows_an_old_refusal() {
    let root = Root::new();
    let i18n = root.i18n();
    let (mut send, fake) = (RcloneSend::default(), Fake::default());
    remotes(&mut send, &fake);
    let request = submit(&mut send, &fake);
    // Card Cancel before the backend answers, then the refusal arrives.
    send.close_card();
    send.on_line(&format!(r#"{{"t":"rclonestarted","requestId":{request},"ok":false,"error":"rclone remote gdrive is no longer configured."}}"#));
    assert!(send.choose("b2-photos", paths()));
    let error = send.card(&i18n).unwrap().error;
    eprintln!("new b2-photos card error {error:?}");
    assert_eq!(
        error, None,
        "a closed card's refusal shows on the next card"
    );
}
