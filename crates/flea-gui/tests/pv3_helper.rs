//! The sandboxed helper on real libmpv, started the way `MediaPlayer::open` starts it.
use flea::ui_logic_pv3::frame::{Command, Event};
use flea_gui::g2::pv3::{
    engine::{Backend, Incoming, Request},
    sandbox::Sandbox,
};
#[path = "pv3_helper/probe.rs"]
mod probe;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Mean per-byte distance past which two pictures differ visibly; clip.mp4's last frame measured 8.8 from its first.
const VISIBLY_OTHER: f64 = 4.;

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
    steps: Vec<(&'static str, Command)>,
) -> Vec<String> {
    run_for(path, at_once, steps, Duration::from_secs(4)).0
}

/// `run` for `listen`, from the helper's start, with each picture's pixels and how many words came before it.
fn run_for(
    path: PathBuf,
    at_once: Option<Command>,
    mut steps: Vec<(&'static str, Command)>,
    listen: Duration,
) -> (Vec<String>, Vec<(usize, Vec<u8>)>) {
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
    let mut pictures = Vec::new();
    if let Some(command) = at_once {
        seen.push(format!("sent {}", command.render()));
        decoder.send(command);
    }
    steps.reverse();
    let deadline = Instant::now() + listen;
    while Instant::now() < deadline {
        let word = match feed.incoming.try_recv() {
            Ok(Incoming::Closed) => {
                seen.push("closed".into());
                break;
            }
            // Taken as the player takes it, so the slot announces the next picture too.
            Ok(Incoming::Frame) => {
                if let Some(image) = feed.frames.take() {
                    let pixels = image.as_bytes(0).unwrap_or_default().to_vec();
                    pictures.push((seen.len(), pixels));
                }
                "frame".to_string()
            }
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
    (seen, pictures)
}

// Control: a seek once the first frame is out lands, and the clip plays on.
#[test]
fn pv3_a_seek_after_the_first_frame_plays_on() {
    if probe::skipped() {
        return;
    }
    let seen = run(fixture("long.mp4"), None, vec![("frame", Command::Seek(0))]);
    assert!(seen.iter().any(|s| s == "seeked 0"), "{seen:?}");
    assert!(!seen.iter().any(|s| s.starts_with("failed")), "{seen:?}");
}

// mpv refuses a seek until the clip loads. A seek sent while loading must not end the helper, so the clip still plays.
#[test]
fn pv3_a_seek_while_loading_keeps_the_clip_playable() {
    if probe::skipped() {
        return;
    }
    for _ in 0..3 {
        let seen = run(fixture("long.mp4"), Some(Command::Seek(0)), vec![]);
        assert!(seen.iter().any(|s| s == "seeked 0"), "{seen:?}");
        assert!(!seen.iter().any(|s| s.starts_with("failed")), "{seen:?}");
    }
}

// The seek sent while loading waits for the load, then lands at its own target.
#[test]
fn pv3_a_seek_sent_while_loading_lands_once_the_clip_loads() {
    if probe::skipped() {
        return;
    }
    let seen = run(fixture("long.mp4"), Some(Command::Seek(5000)), vec![]);
    assert!(seen.iter().any(|s| s == "seeked 5000"), "{seen:?}");
    assert!(!seen.iter().any(|s| s.starts_with("failed")), "{seen:?}");
}

// keep-open pauses the clip at its end. The helper reports the end first, so the GUI never sees a Paused on the way.
#[test]
fn pv3_a_clip_that_ends_never_reports_paused() {
    if probe::skipped() {
        return;
    }
    let seen = run(fixture("clip.mp4"), None, vec![]);
    assert!(seen.iter().any(|s| s == "ended"), "{seen:?}");
    assert!(
        !seen.iter().any(|s| s == "paused"),
        "the end reported a pause: {seen:?}"
    );
}

// The end pauses on the last frame; a Stop then rewinds and draws the first frame again, which the ended preview shows as its poster.
#[test]
fn pv3_a_stop_after_the_end_draws_the_first_frame_again() {
    if probe::skipped() {
        return;
    }
    let (seen, pictures) = run_for(
        fixture("clip.mp4"),
        None,
        vec![("ended", Command::Stop)],
        Duration::from_secs(10),
    );
    assert!(!seen.iter().any(|s| s.starts_with("failed")), "{seen:?}");
    let ended = seen
        .iter()
        .position(|s| s == "ended")
        .unwrap_or_else(|| panic!("the clip never ended: {seen:?}"));
    let sent = seen.iter().position(|s| s == "sent stop").expect("stop");
    assert!(seen[sent..].iter().any(|s| s == "stopped"), "{seen:?}");
    let first = &pictures.first().expect("a first picture").1;
    let held = &pictures
        .iter()
        .rfind(|(at, _)| *at <= ended)
        .expect("a picture before the end")
        .1;
    assert!(
        distance(held, first) > VISIBLY_OTHER,
        "clip.mp4 ends on a picture visibly other than its first: {}",
        distance(held, first)
    );
    let rewound = &pictures
        .iter()
        .rfind(|(at, _)| *at > sent)
        .unwrap_or_else(|| panic!("the rewind drew no picture: {seen:?}"))
        .1;
    // A second decode of frame 0 is not byte-identical, so the rewind is judged by which end it is nearer.
    assert!(
        distance(rewound, first) < distance(rewound, held),
        "the rewind still shows the last picture: {} to the first, {} to the last",
        distance(rewound, first),
        distance(rewound, held)
    );
}

/// Mean absolute difference per byte between two same-sized pictures.
fn distance(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len(), "pictures of one size");
    let sum: u64 = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as u64).sum();
    sum as f64 / a.len().max(1) as f64
}
