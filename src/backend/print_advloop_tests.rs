use super::*;

const PRINT_CANCEL_WAIT: Duration = Duration::from_secs(5);
const PRINT_CANCEL_POLL: Duration = Duration::from_millis(5);
const THREE_FRAME_MAGICK: &str = r#"#!/bin/sh
set -eu
root=${FLEA_PRINT_TEST_ROOT:?}
case "$root" in /*) test -f "$root/.flea-test-sandbox" ;; *) exit 98 ;; esac
printf '%s\0' magick-stub "$@" >> "$root/calls"
printf '\n' >> "$root/calls"
last=
for last do :; done
last=${last#png:}
case "$last" in "$root"/*.png) ;; *) exit 97 ;; esac
set -C
for frame in 0 1 2; do
 printf 'frame %s\n' "$frame" > "${last%.png}-$frame.png"
done
"#;

#[test]
fn print_r3_cancelled_stderr_is_an_ordinary_refusal() {
    let mut rig = Rig::new(true);
    let first = rig.dir.file("first.pdf", "%PDF first");
    let middle = rig.dir.file("stderr-cancelled.pdf", "%PDF middle");
    let last = rig.dir.file("last.pdf", "%PDF last");
    let command = format!(r#"{{"c":"print","requestId":74,"paths":["{}","{}","{}"],"printer":"Brother"}}"#, first.display(), middle.display(), last.display());
    let replies = rig.ask(&command, "printdone");
    let results: Vec<_> = replies.iter().filter(|line| field_str(line, "t").as_deref() == Some("printed")).collect();
    assert_eq!(results.len(), 3, "a tool's stderr must not cancel the batch: {replies:?}");
    assert_eq!(results.iter().map(|line| field_bool(line, "ok")).collect::<Vec<_>>(), [true, false, true]);
    assert_eq!(field_str(results[1], "path").as_deref(), middle.to_str());
    assert_eq!(field_str(results[2], "path").as_deref(), last.to_str());
    let refusal = "stderr-cancelled.pdf was not printed: cancelled.";
    assert_eq!(field_str(results[1], "message").as_deref(), Some(refusal));
    let done = replies.last().unwrap();
    assert!(!field_bool(done, "ok"));
    assert_eq!(field_usize(done, "sent"), Some(2));
    assert_eq!(field_str(done, "err").as_deref(), Some(refusal));
    let calls = rig.calls();
    let lp: Vec<_> = calls.lines().filter(|line| line.starts_with("lp\0")).collect();
    assert_eq!(lp.len(), 3);
    assert!(lp[2].contains(last.to_str().unwrap()));
}

#[test]
fn print_r3_cancelled_last_file_has_no_refusal() {
    for count in [1, 3] {
        let mut rig = Rig::new(true);
        let paths: Vec<_> = (0..count).map(|n| {
            rig.dir.file(&if n + 1 == count { "hold.pdf".into() } else { format!("file-{n}.pdf") }, "%PDF fixture")
        }).collect();
        let sources: Vec<_> = paths.iter().map(|path| path.display().to_string()).collect();
        rig.send(&format!(r#"{{"c":"print","requestId":75,"paths":{},"printer":"Brother"}}"#, crate::json::string_array(&sources)));
        let entered = rig.dir.join("lp-held");
        let end = Instant::now() + PRINT_CANCEL_WAIT;
        while !entered.is_file() && Instant::now() < end {
            std::thread::sleep(PRINT_CANCEL_POLL);
        }
        assert!(entered.is_file(), "the last file reached lp before cancellation");
        rig.send(r#"{"c":"quit"}"#);
        let replies = rig.until("printdone");
        let done = replies.last().unwrap();
        assert_eq!(field_str(done, "err").as_deref(), Some("cancelled"), "last file of {count}: {replies:?}");
        assert!(!field_bool(done, "ok"));
        assert_eq!(field_usize(done, "sent"), Some(count - 1));
        let results: Vec<_> = replies.iter().filter(|line| field_str(line, "t").as_deref() == Some("printed")).collect();
        assert_eq!(results.len(), count - 1, "a user cancellation must not emit a refusal: {replies:?}");
        assert!(results.iter().all(|line| field_bool(line, "ok")));
        assert!(replies.iter().all(|line| !line.contains("was not printed")), "cancellation is not a refusal: {replies:?}");
        let calls = rig.calls();
        let lp: Vec<_> = calls.lines().filter(|line| line.starts_with("lp\0")).collect();
        assert_eq!(lp.len(), count);
        assert!(lp.last().unwrap().contains(paths.last().unwrap().to_str().unwrap()));
    }
}

#[test]
fn print_cancelled_middle_file_stops_batch() {
    let mut rig = Rig::new(true);
    let first = rig.dir.file("first.pdf", "%PDF first");
    let middle = rig.dir.file("hold.pdf", "%PDF middle");
    let last = rig.dir.file("last.pdf", "%PDF last");
    rig.send(&format!(r#"{{"c":"print","requestId":73,"paths":["{}","{}","{}"],"printer":"Brother"}}"#, first.display(), middle.display(), last.display()));
    let entered = rig.dir.join("lp-held");
    let end = Instant::now() + PRINT_CANCEL_WAIT;
    while !entered.is_file() && Instant::now() < end {
        std::thread::sleep(PRINT_CANCEL_POLL);
    }
    assert!(entered.is_file(), "the second file reached lp before cancellation");
    rig.send(r#"{"c":"quit"}"#);
    let replies = rig.until("printdone");
    let done = replies.last().unwrap();
    assert_eq!(field_str(done, "err").as_deref(), Some("cancelled"), "{replies:?}");
    assert!(!field_bool(done, "ok"));
    assert_eq!(field_usize(done, "sent"), Some(1));
    let results: Vec<_> = replies.iter().filter(|line| field_str(line, "t").as_deref() == Some("printed")).collect();
    assert_eq!(results.len(), 1, "cancellation must produce no refusal or receipt for the third file");
    assert_eq!(field_str(results[0], "path").as_deref(), first.to_str());
    assert!(field_bool(results[0], "ok"));
    assert!(replies.iter().all(|line| !line.contains("was not printed")), "cancellation is not a refusal");
    let calls = rig.calls();
    let lp: Vec<_> = calls.lines().filter(|line| line.starts_with("lp\0")).collect();
    assert_eq!(lp.len(), 2);
    assert!(lp[0].contains(first.to_str().unwrap()));
    assert!(lp[1].contains(middle.to_str().unwrap()));
    assert!(lp.iter().all(|line| !line.contains(last.to_str().unwrap())), "the third file never reaches lp");
}

#[test]
fn adv_e_r1_print_reports_every_file_after_a_middle_refusal() {
    let mut rig = Rig::new(true);
    let first = rig.dir.file("first.pdf", "%PDF first");
    let refused = rig.dir.file("refuse.pdf", "%PDF middle");
    let last = rig.dir.file("last.pdf", "%PDF last");
    let command = format!(r#"{{"c":"print","requestId":71,"paths":["{}","{}","{}"],"printer":"Brother"}}"#, first.display(), refused.display(), last.display());
    let replies = rig.ask(&command, "printdone");
    let results: Vec<_> = replies.iter().filter(|line| field_str(line, "t").as_deref() == Some("printed")).collect();
    assert_eq!(results.len(), 3, "each selected file needs a result: {replies:?}");
    for (line, path) in results.iter().zip([&first, &refused, &last]) {
        assert_eq!(field_str(line, "path").as_deref(), path.to_str());
    }
    assert_eq!(results.iter().map(|line| field_bool(line, "ok")).collect::<Vec<_>>(), [true, false, true]);
    assert_eq!(field_str(results[1], "message").as_deref(), Some("refuse.pdf was not printed: printer is paused."));
    let done = replies.last().unwrap();
    assert!(!field_bool(done, "ok"));
    assert_eq!(field_usize(done, "sent"), Some(2));
    assert!(field_str(done, "err").unwrap().contains("printer is paused"));
    let calls = rig.calls();
    let lp: Vec<_> = calls.lines().filter(|line| line.starts_with("lp\0")).collect();
    assert_eq!(lp.len(), 3);
    assert!(lp[2].contains(last.to_str().unwrap()));
}

#[test]
fn adv_e_r1_three_frame_tiff_keeps_one_progress_total() {
    let mut rig = Rig::with_magick(THREE_FRAME_MAGICK);
    let source = rig.dir.file("three.tiff", "three-frame TIFF stub input\n");
    let replies = rig.ask(&format!(r#"{{"c":"convertbatch","requestId":72,"path":"{}","format":"png"}}"#, source.display()), "convertdone");
    let done = replies.last().unwrap();
    assert!(field_bool(done, "ok"), "{replies:?}");
    let outputs = crate::json::field_str_array(done, "paths");
    assert_eq!(outputs.len(), 3, "every TIFF frame is published");
    for (frame, path) in outputs.iter().enumerate() {
        rig.dir.assert_contains(std::path::Path::new(path));
        assert_eq!(std::fs::read_to_string(path).unwrap(), format!("frame {frame}\n"));
    }
    assert!(rig.calls().lines().any(|line| line.starts_with("magick-stub\0")), "the private Magick stub must run");
    let progress: Vec<_> = replies.iter().filter(|line| field_str(line, "t").as_deref() == Some("convertprogress"))
        .map(|line| (field_usize(line, "done").unwrap(), field_usize(line, "total").unwrap())).collect();
    println!("three-frame TIFF progress (done,total): {progress:?}");
    assert!(progress.len() >= 3, "conversion and publication both report progress");
    let total = progress[0].1;
    assert!(total > 0 && progress.iter().all(|(done, count)| *count == total && *done <= total), "one fixed total: {progress:?}");
    assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0), "progress never regresses");
    assert_eq!(progress.last().unwrap().0, total, "successful publication completes the job");
}
