//! The sandboxed helper on real libmpv, started the way `MediaPlayer::open` starts it.
use flea::ui_logic_pv3::frame::{Command, Event};
use flea_gui::g2::pv3::{
    engine::{Backend, Incoming, Request},
    sandbox::Sandbox,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/pv3_helper")
        .join(name)
}

fn gui() -> PathBuf {
    env!("CARGO_BIN_EXE_flea-gui").into()
}

/// Starts one helper on `path` and lists what it reports for four seconds.
///
/// `at_once` goes out before the first event, while the clip loads. Each step's command goes out when its trigger word arrives.
fn run(
    path: PathBuf,
    at_once: Option<Command>,
    mut steps: Vec<(&'static str, Command)>,
) -> Vec<String> {
    let sandbox = Sandbox {
        executable: gui(),
        bwrap: "bwrap".into(),
    };
    let request = Request {
        path,
        box_w: 320,
        box_h: 180,
        audio: false,
        autoplay: true,
    };
    let (mut decoder, feed) = sandbox.start(&request).expect("start");
    let mut seen = Vec::new();
    if let Some(command) = at_once {
        seen.push(format!("sent {}", command.render()));
        decoder.send(command);
    }
    steps.reverse();
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        let word = match feed.incoming.try_recv() {
            Ok(Incoming::Closed) => {
                seen.push("closed".into());
                break;
            }
            Ok(Incoming::Frame) => "frame".to_string(),
            Ok(Incoming::Event(Event::Position(_))) => continue,
            Ok(Incoming::Event(event)) => event.render(),
            Err(_) => {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
        };
        if word != "frame" || seen.last().map(String::as_str) != Some("frame") {
            seen.push(word.clone());
        }
        if steps.last().is_some_and(|(trigger, _)| *trigger == word) {
            let (_, command) = steps.pop().expect("step");
            seen.push(format!("sent {}", command.render()));
            decoder.send(command);
        }
    }
    seen
}

// Control: a seek once the first frame is out lands, and the clip plays on.
#[test]
fn pv3_a_seek_after_the_first_frame_plays_on() {
    let seen = run(fixture("long.mp4"), None, vec![("frame", Command::Seek(0))]);
    assert!(seen.iter().any(|s| s == "seeked 0"), "{seen:?}");
    assert!(!seen.iter().any(|s| s.starts_with("failed")), "{seen:?}");
}

// mpv refuses a seek until the clip loads. A seek sent while loading must not end the helper, so the clip still plays.
#[test]
fn pv3_a_seek_while_loading_keeps_the_clip_playable() {
    for _ in 0..3 {
        let seen = run(fixture("long.mp4"), Some(Command::Seek(0)), vec![]);
        assert!(seen.iter().any(|s| s == "seeked 0"), "{seen:?}");
        assert!(!seen.iter().any(|s| s.starts_with("failed")), "{seen:?}");
    }
}

// The seek sent while loading waits for the load, then lands at its own target.
#[test]
fn pv3_a_seek_sent_while_loading_lands_once_the_clip_loads() {
    let seen = run(fixture("long.mp4"), Some(Command::Seek(5000)), vec![]);
    assert!(seen.iter().any(|s| s == "seeked 5000"), "{seen:?}");
    assert!(!seen.iter().any(|s| s.starts_with("failed")), "{seen:?}");
}

// keep-open pauses the clip at its end. The helper reports the end first, so the GUI never sees a Paused on the way.
#[test]
fn pv3_a_clip_that_ends_never_reports_paused() {
    let seen = run(fixture("clip.mp4"), None, vec![]);
    assert!(seen.iter().any(|s| s == "ended"), "{seen:?}");
    assert!(
        !seen.iter().any(|s| s == "paused"),
        "the end reported a pause: {seen:?}"
    );
}
