use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn stderr_and_exit_status_survive_a_tool_refusal() {
    let d = TestDir::new("print-tool-refusal");
    let stub = d.script("refuse", "#!/bin/sh\nprintf '%s\\n' 'first line' 'printer is paused' >&2\nexit 1\n");
    let error = run(&[stub.to_string_lossy().into_owned()], &AtomicBool::new(false), QUERY_WAIT).unwrap_err();
    assert_eq!(error.message, "printer is paused");
    assert!(!error.lingering);
}

#[test]
fn print_tool_missing_encoder_diagnostic_is_not_masked_by_fallback() {
    let d = TestDir::new("print-tool-missing-encoder");
    let stub = d.script("refuse", "#!/bin/sh\nprintf '%s\\n' \"magick: no encode delegate for this image format AVIF\" \"magick: WriteBlob Failed avif:/work/output.avif @ error/png.c\" >&2\nexit 1\n");
    let error = run(&[stub.to_string_lossy().into_owned()], &AtomicBool::new(false), QUERY_WAIT).unwrap_err();
    assert_eq!(error.message, "magick: no encode delegate for this image format AVIF");
    assert!(!error.lingering);
}

#[test]
fn a_tool_deadline_kills_the_whole_process_group() {
    let d = TestDir::new("print-tool-timeout");
    let stub = d.script("wait", "#!/bin/sh\nsleep 30 &\nwait\n");
    let start = Instant::now();
    let error = run(&[stub.to_string_lossy().into_owned()], &AtomicBool::new(false), Duration::from_millis(80)).unwrap_err();
    assert!(error.message.contains("took longer"), "{}", error.message);
    assert!(start.elapsed() < Duration::from_secs(4));
    assert!(!error.lingering, "the group's pipe writers exited");
}

#[test]
fn output_is_drained_but_never_kept_without_a_bound() {
    let d = TestDir::new("print-tool-output");
    let stub = d.script("loud", "#!/bin/sh\nhead -c 100000 /dev/zero\n");
    let error = run(&[stub.to_string_lossy().into_owned()], &AtomicBool::new(false), QUERY_WAIT).unwrap_err();
    assert_eq!(error.message, "the tool returned too much output");
}

#[test]
fn publication_never_overwrites_a_source_or_a_new_collision() {
    let d = TestDir::new("print-convert-publish");
    let source = d.file("source.png", "original");
    let output = d.join("staged.png");
    d.assert_contains(&source);
    d.assert_contains(&output);
    stage(&source, &output).unwrap();
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "original");
    assert!(publish(&output, &source).is_err());
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "original");
    let dest = d.join("result.png");
    d.assert_contains(&dest);
    publish(&output, &dest).unwrap();
    assert!(!output.exists());
    assert_eq!(std::fs::read_to_string(dest).unwrap(), "original");
    assert!(inputs(&[PathBuf::from("relative.png")]).is_err());
    assert!(inputs(&[source.clone(), source]).is_err());
    assert!(inputs(&[d.dir("directory")]).is_err());
}

// A print tool the system lacks says it is not installed; a missing sandbox launcher keeps its own sentence.
#[test]
fn a_missing_print_tool_is_named_as_not_installed() {
    let cancel = AtomicBool::new(false);
    let missing = run(&["/nonexistent/flea-lpstat".into(), "-e".into()], &cancel, Duration::from_secs(1)).unwrap_err();
    assert_eq!(missing.message, "/nonexistent/flea-lpstat is not installed");
    let gone = std::io::Error::from(std::io::ErrorKind::NotFound);
    assert_eq!(spawn_message(&["lpstat".into()], &gone), "lpstat is not installed");
    assert_eq!(spawn_message(&["bwrap".into()], &gone), "bwrap: item not found");
    assert_eq!(spawn_message(&["/usr/bin/prlimit".into()], &gone), "/usr/bin/prlimit: item not found");
}
