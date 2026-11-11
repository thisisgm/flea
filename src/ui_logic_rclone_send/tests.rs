use super::present::{self, Bar, Part};
use super::wire::parse;
use super::*;
use crate::i18n::{I18n, Paths};
use std::path::{Path, PathBuf};
use std::process::Command;

// Catalog fixtures live in a marked mktemp root of this test's own; nothing outside it is written.
struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(".superpowers/sd2");
        std::fs::create_dir_all(&base).unwrap();
        let out = Command::new("mktemp")
            .arg("-d")
            .arg(base.join("core.XXXXXX"))
            .output()
            .unwrap();
        let path = PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.starts_with(&base) && path != base);
        std::fs::write(path.join(".sd2-test-root"), b"sd2\n").unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) {
        assert!(self.0.join(".sd2-test-root").is_file());
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn i18n(&self, language: &str) -> I18n {
        self.write(
            "omarchy/default/i18n/locales.json",
            r#"{"en":{},"de":{"name":"Deutsch"}}"#,
        );
        self.write(
            "omarchy/default/i18n/en/shell.json",
            r#"{"common.cancel":"Cancel"}"#,
        );
        self.write(
            "omarchy/default/i18n/de/shell.json",
            r#"{"common.cancel":"Abbrechen"}"#,
        );
        self.write("language", language);
        I18n::load(Paths {
            omarchy: self.0.join("omarchy"),
            language_file: self.0.join("language"),
            catalogs: vec![self.0.join("catalog")],
        })
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        if self.0.is_absolute()
            && self.0.join(".sd2-test-root").is_file()
            && self.0.to_string_lossy().contains("/.superpowers/sd2/")
        {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn event(line: &str) -> Event {
    parse(line)
        .unwrap()
        .unwrap_or_else(|| panic!("not a send event: {line}"))
}

fn job(fields: &str) -> Event {
    event(&format!(
        r#"{{"t":"job","job":{{"id":12,"kind":"send",{fields}}},"unfinished":1}}"#
    ))
}

// The board's three photographs, the second 40% up at a home uplink's rate, as int4's producer serializes the job.
fn sending() -> Send {
    let mut send = Send::new(72, "gdrive", "Uploads", 3);
    assert!(send.apply(&event(
        r#"{"t":"rclonestarted","requestId":72,"id":12,"ok":true,"error":""}"#
    )));
    assert!(send.apply(&job(r#""head":"Sending 2 of 3 to gdrive","file":"DSC_4821.jpg","done":1,"total":3,"bytes":795120,"totalBytes":1702100,"itemBytes":565800,"rate":240000,"left":null,"state":"running","reason":"","output":[],"undo":false,"cancelled":false,"phase":"sending","check":"none","receipt":"","note":"""#)));
    send
}

fn text(parts: &[Part]) -> String {
    parts.iter().map(|part| part.text.as_str()).collect()
}

#[test]
fn sd2_remote_names_and_folders_follow_the_backend_rule() {
    for name in ["gdrive", "b2-photos", "nas-crypt", "my drive"] {
        assert!(remote_name_ok(name), "{name}");
    }
    for name in [
        "",
        ":local:",
        "gdrive,token=value",
        "folder/name",
        "a\\b",
        "bad\nname",
    ] {
        assert!(!remote_name_ok(name), "{name:?}");
    }
    assert_eq!(folder_check("Uploads"), Ok("Uploads"));
    assert_eq!(folder_check("Photos/2025"), Ok("Photos/2025"));
    assert_eq!(folder_check(""), Err(FolderRefusal::Empty));
    for bad in ["..", "a/../b", "../a", "a\u{0}b", "a\nb"] {
        assert_eq!(folder_check(bad), Err(FolderRefusal::Invalid), "{bad:?}");
    }
}

#[test]
fn sd2_request_lines_match_the_int4_wire() {
    let line = send_line(72, "gdrive", "Up\"loads", &["/a/b \"c\".jpg".into()]);
    assert_eq!(
        line,
        r#"{"c":"rclone","op":"send","id":72,"remote":"gdrive","folder":"Up\"loads","paths":["/a/b \"c\".jpg"]}"#
    );
    assert_eq!(
        crate::json::field_str(&line, "folder").as_deref(),
        Some("Up\"loads")
    );
    assert_eq!(remotes_line(71), r#"{"c":"rclone","op":"remotes","id":71}"#);
    assert_eq!(
        jobs_line("cancel", 12),
        r#"{"c":"jobs","op":"cancel","id":12}"#
    );
}

#[test]
fn sd2_replies_parse_and_malformed_lines_refuse() {
    assert_eq!(
        parse(
            r#"{"t":"rcloneremotes","id":71,"visible":true,"remotes":["gdrive","b2-photos","nas-crypt"]}"#
        ),
        Ok(Some(Event::Remotes {
            id: 71,
            visible: true,
            remotes: vec!["gdrive".into(), "b2-photos".into(), "nas-crypt".into()]
        }))
    );
    assert_eq!(
        parse(r#"{"t":"rcloneremotes","id":71,"visible":false,"remotes":[]}"#),
        Ok(Some(Event::Remotes {
            id: 71,
            visible: false,
            remotes: vec![]
        }))
    );
    assert_eq!(
        parse(
            r#"{"t":"rclonestarted","requestId":72,"ok":false,"error":"An rclone folder cannot contain a .. component."}"#
        ),
        Ok(Some(Event::Started {
            request: 72,
            job: None,
            error: "An rclone folder cannot contain a .. component.".into()
        }))
    );
    assert_eq!(
        parse(r#"{"t":"joberror","id":12,"error":"Only a failed job can be retried."}"#),
        Ok(Some(Event::Refused {
            job: 12,
            error: "Only a failed job can be retried.".into()
        }))
    );
    assert_eq!(parse(r#"{"t":"listed","rows":[]}"#), Ok(None));
    // A reply for another surface that merely carries a name starting with job or rclone is never parsed (review r1-6).
    assert_eq!(
        parse(r#"{"t":"rows","start":0,"rows":[{"name":"jobs.csv"},{"name":"rclone.conf.bak"#),
        Ok(None)
    );
    match parse(
        r#"{"t":"jobs","jobs":[{"id":3,"kind":"copy","state":"running","bytes":null,"left":null}],"unfinished":1,"newest":3,"head":"x"}"#,
    ) {
        Ok(Some(Event::Jobs {
            jobs,
            snapshot: true,
        })) => assert_eq!((jobs[0].id, jobs[0].bytes, jobs[0].left), (3, 0, None)),
        other => panic!("{other:?}"),
    }
    for bad in [
        r#"{"t":"rcloneremotes","id":2,"remotes":[1]}"#,
        r#"{"t":"rcloneremotes","id":2,"remotes":"gdrive"}"#,
        r#"{"t":"rclonestarted","requestId":72,"ok":true,"error":""}"#,
        r#"{"t":"job","job":{"kind":"send"}}"#,
        r#"{"t":"job","job":{"id":12,"bytes":"many"}}"#,
        r#"{"t":"jobs","jobs":{}}"#,
        r#"{"t":"rclonesent","id":-1}"#,
        r#"{"t":"rclonesent","id":1,"#,
    ] {
        assert!(parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn sd2_a_send_moves_forward_only_for_its_own_request_and_job() {
    let mut send = Send::new(72, "gdrive", "Uploads", 3);
    assert!(send.cancel().is_none(), "no job id to cancel yet");
    assert!(
        !send.apply(&event(
            r#"{"t":"rclonestarted","requestId":73,"id":13,"ok":true,"error":""}"#
        )),
        "another request"
    );
    assert!(send.apply(&event(
        r#"{"t":"rclonestarted","requestId":72,"id":12,"ok":true,"error":""}"#
    )));
    assert!(
        !send.apply(&event(
            r#"{"t":"rclonestarted","requestId":72,"id":14,"ok":true,"error":""}"#
        )),
        "a second start is stale"
    );
    assert!(
        !send.apply(&event(
            r#"{"t":"job","job":{"id":13,"kind":"send","state":"running","phase":"checking"}}"#
        )),
        "another job"
    );
    assert!(
        !send.apply(&event(
            r#"{"t":"job","job":{"id":12,"kind":"copy","state":"running"}}"#
        )),
        "not a send row"
    );
    assert!(send.apply(&job(
        r#""state":"running","phase":"checking","check":"checksum","total":3"#
    )));
    assert_eq!(send.phase, Phase::Checking(true));
    assert!(
        !send.apply(&job(
            r#""state":"running","phase":"sending","done":2,"total":3"#
        )),
        "no step back from checking"
    );
    assert_eq!(
        send.cancel().as_deref(),
        Some(r#"{"c":"jobs","op":"cancel","id":12}"#)
    );
    assert!(send.cancel().is_none(), "one cancel");
    assert!(
        send.apply(&event(
            r#"{"t":"joberror","id":12,"error":"Unknown jobs request."}"#
        )) && !send.cancelling,
        "a refused cancel frees the button"
    );
    assert!(send.apply(&job(
        r#""state":"failed","reason":"DSC_4930.jpg did not match on b2-photos.","done":2,"total":3"#
    )));
    assert!(send.apply(&event(r#"{"t":"rclonesent","id":12,"requestId":72,"ok":false,"cancelled":false,"check":"none","receipt":"","note":"","error":"DSC_4930.jpg did not match on b2-photos."}"#)));
    assert_eq!(
        send.phase,
        Phase::Failed("DSC_4930.jpg did not match on b2-photos.".into())
    );
    assert!(send.cancel().is_none(), "a failed send takes no cancel");
    assert_eq!(
        send.retry().as_deref(),
        Some(r#"{"c":"jobs","op":"retry","id":12}"#)
    );
    assert!(send.retry().is_none(), "only a failed send retries");
    assert!(send.apply(&job(
        r#""state":"running","phase":"sending","done":0,"total":1"#
    )));
    assert!(send.apply(&job(
        r#""state":"done","phase":"checked","check":"checksum","done":1,"total":1"#
    )));
    assert!(send.running(), "done waits for rclonesent");
    assert!(!send.apply(&event(r#"{"t":"rclonesent","id":12,"requestId":71,"ok":true,"check":"checksum","receipt":"x"}"#)), "another request's receipt");
    assert!(send.apply(&event(r#"{"t":"rclonesent","id":12,"requestId":72,"ok":true,"cancelled":false,"check":"checksum","receipt":"Sent 1 item to Uploads on gdrive","note":"","error":""}"#)));
    assert_eq!(
        send.phase,
        Phase::Receipt {
            check: "checksum".into(),
            note: String::new()
        }
    );
    assert!(
        !send.apply(&job(r#""state":"running","phase":"sending""#)),
        "a late row after the receipt"
    );
    assert!(
        !send.backend_ended() && send.job.is_none(),
        "an ended send is not lost but leaves its job id"
    );
}

#[test]
fn sd2_refusal_cancel_dismiss_and_backend_death() {
    let mut refused = Send::new(80, "gdrive", "..", 1);
    assert!(refused.apply(&event(r#"{"t":"rclonestarted","requestId":80,"ok":false,"error":"An rclone folder cannot contain a .. component."}"#)));
    assert!(!refused.running() && refused.job.is_none());
    let mut cancelled = sending();
    assert!(cancelled.cancel().is_some());
    assert!(
        cancelled.apply(&event(r#"{"t":"rclonesent","id":12,"requestId":72,"ok":false,"cancelled":true,"check":"none","receipt":"","note":"","error":""}"#))
    );
    assert_eq!(
        cancelled.phase,
        Phase::Gone,
        "cancelled before anything copied leaves nothing"
    );
    let mut failed = sending();
    assert!(failed.apply(&job(
        r#""state":"failed","reason":"Could not send a.jpg to gdrive.""#
    )));
    assert!(failed.apply(&event(
        r#"{"t":"jobs","jobs":[{"id":12,"kind":"send","state":"failed"}],"unfinished":0}"#
    )));
    assert!(
        matches!(failed.phase, Phase::Failed(_)),
        "a snapshot holding the row keeps it"
    );
    assert!(
        failed.apply(&event(
            r#"{"t":"jobs","jobs":[],"unfinished":0,"newest":null,"head":""}"#
        )),
        "a snapshot without the row"
    );
    assert_eq!(failed.phase, Phase::Gone);
    let mut failed = sending();
    assert!(failed.apply(&job(r#""state":"failed","reason":"x""#)));
    assert_eq!(
        failed.dismiss().as_deref(),
        Some(r#"{"c":"jobs","op":"dismiss","id":12}"#)
    );
    let mut lost = sending();
    assert!(
        lost.backend_ended()
            && lost.phase == Phase::Lost
            && lost.cancel().is_none()
            && lost.retry().is_none()
    );
    assert!(
        lost.dismiss().is_none() && lost.phase == Phase::Gone,
        "a lost send leaves without a request"
    );
}

mod words;
