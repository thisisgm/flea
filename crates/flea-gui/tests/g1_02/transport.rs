use super::*;
use flea_gui::backend::{Backend, Event};
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

pub(super) fn root() -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g1-02");
    std::fs::create_dir_all(&parent).unwrap();
    let output = Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("test.XXXXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g1-02\n").unwrap();
    root
}

#[test]
fn pipes_delayed_40ms_and_2500ms_keep_order_and_preserve_captured_row_generations() {
    for delay in [40, 2500] {
        let root = root();
        let script = r#"import json,pathlib,sys,time
root=pathlib.Path(sys.argv[1])
for line in sys.stdin:
 request=json.loads(line)
 with (root/'requests').open('a') as output: output.write(line)
 if request['c']=='list':
  time.sleep(int(sys.argv[2])/1000)
  print(json.dumps(dict(t='listed',path=request['path'],n=0)),flush=True)
  print(json.dumps(dict(t='rows',start=0,rows=[],listing=1)),flush=True)
 if request['c']=='quit': break
"#;
        let mut command = Command::new("python3");
        command
            .args(["-u", "-c", script])
            .arg(&root)
            .arg(delay.to_string());
        let backend = Backend::start_command(command, "/fixture/empty".into(), None, 320).unwrap();
        let started = Instant::now();
        let deadline = started + Duration::from_secs(10);
        let mut replies = Vec::new();
        while replies.len() < 2 {
            assert!(Instant::now() < deadline, "delayed pipe did not answer");
            match backend.replies.try_recv() {
                Ok(Event::Reply(reply)) => replies.push(reply),
                Ok(other) => panic!("unexpected event: {other:?}"),
                Err(async_channel::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("closed pipe: {error}"),
            }
        }
        let elapsed = started.elapsed().as_millis();
        assert!(elapsed >= delay as u128);
        assert!(elapsed < delay as u128 + 2000);
        // The pane runs on a virtual clock, so the pair lands at the scripted delay whatever the host load.
        let mut pane = Pane::new("/fixture/empty".into(), 620.);
        let mut paired = false;
        for now in 0..=delay + 2 * flea_gui::pane::swap::MIN_SHOW_MS {
            if pane.tick(now) {
                if !pane.busy() {
                    paired = true;
                    break;
                }
                pane.frame(now, true);
            }
            if now == delay {
                for reply in replies.drain(..) {
                    paired |= pane.receive_at(reply, now).unwrap();
                }
                if paired {
                    break;
                }
            }
        }
        assert!(paired, "the delayed pair never landed");
        backend
            .rows(
                "paths",
                flea_gui::pane::requests::RowAction {
                    listing: 7,
                    rows: vec![0, 4],
                },
            )
            .unwrap();
        backend
            .list(
                "/fixture/next".into(),
                Some("/fixture/next/name".into()),
                320,
            )
            .unwrap();
        backend.quit();
        backend.drained().recv_blocking().unwrap();
        let sent = std::fs::read_to_string(root.join("requests")).unwrap();
        let lines: Vec<_> = sent
            .lines()
            .map(|line| flea::jsondoc::parse(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[1].get("listing").unwrap().as_f64(), Some(7.));
        assert_eq!(
            lines[2].get("select").unwrap().as_str(),
            Some("/fixture/next/name")
        );
        assert_eq!(pane.state.kind, Kind::Empty);
        if delay == 40 {
            assert_eq!(pane.swap.counters.shown_ms, 0);
        } else {
            assert!(pane.swap.counters.shown_ms >= 100);
        }
    }
}

#[test]
fn denied_pipe_keeps_optional_mode_and_never_relabels_a_window_refusal() {
    let mut command = Command::new("python3");
    command.args(["-u", "-c", "import sys\nsys.stdin.readline()\nprint('{\"t\":\"error\",\"where\":\"scan\",\"path\":\"/fixture/Locked\",\"msg\":\"Permission denied\",\"mode\":16384}',flush=True)\nsys.stdin.readline()"]);
    let backend = Backend::start_command(command, "/fixture/Locked".into(), None, 320).unwrap();
    match backend.replies.recv_blocking().unwrap() {
        Event::Refused {
            operation, mode, ..
        } => {
            assert_eq!(operation, "scan");
            assert_eq!(mode, 0o40000);
        }
        other => panic!("wrong refusal: {other:?}"),
    }
    backend.quit();
    backend.drained().recv_blocking().unwrap();
}
