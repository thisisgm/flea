// Real children: a stalled read is killed and reaped, a stale partial line dropped, a missing binary still answers.
use super::*;
use std::time::Instant;

fn shell(script: &str) -> PickerProcess {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script);
    PickerProcess::start(command)
}

// Every line up to and including Stopped, or a failure when the child is not reaped within the bound.
fn drain(lines: &Receiver<Line>, bound: Duration) -> Vec<Line> {
    let deadline = Instant::now() + bound;
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        match lines.try_recv() {
            Ok(line) => {
                let stopped = line == Line::Stopped;
                seen.push(line);
                if stopped {
                    return seen;
                }
            }
            Err(_) => thread::sleep(Duration::from_millis(2)),
        }
    }
    panic!("the child was not reaped within {bound:?}; saw {seen:?}");
}

fn wait_for(lines: &Receiver<Line>, wanted: &Line) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match lines.try_recv() {
            Ok(line) if &line == wanted => return,
            Ok(line) => panic!("expected {wanted:?}, got {line:?}"),
            Err(_) => thread::sleep(Duration::from_millis(2)),
        }
    }
    panic!("{wanted:?} never arrived");
}

#[test]
fn a_picker_line_reaches_the_child_and_its_answer_comes_back_whole() {
    let child = shell(r#"while IFS= read -r line; do printf '%s\n' "$line"; done"#);
    let line = r#"{"c":"picker","op":"validate","id":1}"#;
    assert!(child.send(line));
    wait_for(&child.lines(), &Line::Reply(line.into()));
    assert!(
        !child.send(r#"{"c":"transfer","rows":[0],"dest":"/not-allowed"}"#),
        "a chooser never sends an operation"
    );
    assert!(!child.send("not json"));
    child.quit();
    assert_eq!(
        drain(&child.lines(), Duration::from_secs(5)),
        [Line::Stopped],
        "a quit child is no failure"
    );
    assert!(!child.send(line), "nothing is sent after the quit");
}

#[test]
fn a_stalled_child_is_killed_and_its_partial_line_dropped() {
    let child = shell(r#"printf '{"t":"listed"}\npartial'; exec sleep 30"#);
    wait_for(&child.lines(), &Line::Reply(r#"{"t":"listed"}"#.into()));
    let began = Instant::now();
    child.quit();
    assert_eq!(
        drain(&child.lines(), Duration::from_secs(5)),
        [Line::Stopped]
    );
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "the stall waited {:?}",
        began.elapsed()
    );
}

#[test]
fn a_child_that_cannot_start_still_fails_and_stops_once() {
    let child = PickerProcess::start(Command::new("/nonexistent/flea-picker-helper"));
    let seen = drain(&child.lines(), Duration::from_secs(1));
    assert!(
        matches!(seen.as_slice(), [Line::Failed(_), Line::Stopped]),
        "{seen:?}"
    );
    assert!(!child.send(r#"{"c":"picker","op":"close"}"#));
    child.quit();
    assert!(
        child.lines().try_recv().is_err(),
        "a quit after the stop answers nothing more"
    );
}

#[test]
fn a_child_that_dies_unasked_is_a_failure_before_its_stop() {
    let child = shell(r#"printf '{"t":"picker","id":1,"ok":true}\n'; exit 3"#);
    let seen = drain(&child.lines(), Duration::from_secs(5));
    assert_eq!(
        seen,
        [
            Line::Reply(r#"{"t":"picker","id":1,"ok":true}"#.into()),
            Line::Failed("exited".into()),
            Line::Stopped
        ]
    );
}

#[test]
fn a_child_that_closes_its_output_but_lives_is_reaped_by_the_quit() {
    let child = shell("exec 1>&-; exec sleep 30");
    thread::sleep(Duration::from_millis(50));
    child.quit();
    assert_eq!(
        drain(&child.lines(), Duration::from_secs(5)),
        [Line::Stopped]
    );
}

#[test]
fn dropping_the_handle_kills_the_child() {
    let child = shell("exec sleep 30");
    let lines = child.lines();
    drop(child);
    assert_eq!(drain(&lines, Duration::from_secs(5)), [Line::Stopped]);
}
