use super::fake::Root;
use crate::g2::media_preview::engine::{feed, Backend, Incoming, Request};
use crate::g2::media_preview::helper::Options;
use crate::g2::media_preview::sandbox::{arguments, read, Sandbox};
use flea::ui_logic_media_preview::frame::{header, Event, Failure, Record};
use std::path::Path;

fn request() -> Request {
    Request {
        path: "/m/clip.mp4".into(),
        box_w: 1920,
        box_h: 1080,
        audio: true,
        autoplay: true,
    }
}

fn drained(bytes: &[u8]) -> Vec<Incoming> {
    let (sender, feed) = feed();
    read(bytes, &sender);
    std::iter::from_fn(|| feed.incoming.try_recv().ok()).collect()
}

// The helper runs with no network, no home and no environment; only its input and its own binary are mounted.
#[test]
fn pv3_the_sandbox_shares_nothing_but_one_read_only_input() {
    let words = arguments(&request(), Path::new("/usr/bin/flea-gui"), 7, None);
    for wanted in [
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--clearenv",
    ] {
        assert!(words.iter().any(|word| word == wanted), "{wanted}");
    }
    let joined = words.join(" ");
    assert!(joined.contains("--cap-drop ALL"));
    assert!(joined.contains("--ro-bind-fd 7 /input"), "{joined}");
    assert!(joined.contains("--ro-bind /usr/bin/flea-gui /flea-media"));
    assert!(
        joined.ends_with("-- /flea-media --media-helper 1920 1080 0 1"),
        "{joined}"
    );
    assert!(
        !joined.contains("PULSE_SERVER"),
        "no socket, no sound server"
    );
    assert!(
        !words.iter().any(|word| word.starts_with("/home")),
        "no home is mounted"
    );
    assert!(
        !joined.contains("/run/user"),
        "no runtime path without a socket"
    );
}

// EY-461: fontconfig reads its cache read-only, only where the box has one, so a video preview logs no cache error.
#[test]
fn pv3_the_sandbox_binds_the_fontconfig_cache_read_only_when_present() {
    let words = arguments(&request(), Path::new("/b"), 3, None);
    let at = words
        .iter()
        .position(|word| word == "/var/cache/fontconfig")
        .expect("the fontconfig cache is named");
    assert_eq!(
        (words[at - 1].as_str(), words[at + 1].as_str()),
        ("--ro-bind-try", "/var/cache/fontconfig"),
        "a read-only bind that bwrap skips when the path is absent"
    );
    assert_eq!(
        words.iter().filter(|word| word.starts_with("/var")).count(),
        2,
        "nothing else of /var is mounted: {words:?}"
    );
}

// mpv plays into the sound server through exactly its socket; nothing else of the runtime directory is mounted.
#[test]
fn pv3_the_sandbox_binds_only_the_pulse_socket_from_the_runtime_dir() {
    let words = arguments(
        &request(),
        Path::new("/b"),
        3,
        Some(Path::new("/run/user/1000/pulse/native")),
    );
    let runtime: Vec<usize> = words
        .iter()
        .enumerate()
        .filter(|(_, word)| word.contains("/run/user"))
        .map(|(at, _)| at)
        .collect();
    assert_eq!(runtime.len(), 1, "one runtime path in {words:?}");
    let at = runtime[0];
    assert_eq!(words[at], "/run/user/1000/pulse/native");
    assert_eq!(
        (words[at - 1].as_str(), words[at + 1].as_str()),
        ("--ro-bind", "/pulse-native"),
        "a read-only bind of the socket alone"
    );
    for wide in ["--bind", "--bind-try", "--dev-bind", "--share-net"] {
        assert!(!words.iter().any(|word| word == wide), "{wide}");
    }
    let joined = words.join(" ");
    assert!(
        joined.contains("--setenv PULSE_SERVER unix:/pulse-native"),
        "{joined}"
    );
    assert!(joined.ends_with("--media-helper 1920 1080 1 1"), "{joined}");
}

#[test]
fn pv3_helper_arguments_round_trip_and_malformed_ones_are_refused() {
    let words: Vec<String> = ["640", "360", "1", "0"].map(String::from).into();
    let options = Options::parse(&words).expect("valid words");
    assert_eq!(options.words().to_vec(), words);
    let refused: [&[&str]; 5] = [
        &["640", "360", "1"],
        &["mpv", "640", "360", "1", "0"],
        &["0", "360", "1", "0"],
        &["640", "360", "yes", "0"],
        &["-1", "360", "1", "0"],
    ];
    for bad in refused {
        let bad: Vec<String> = bad.iter().map(|word| word.to_string()).collect();
        assert!(Options::parse(&bad).is_none(), "{bad:?}");
    }
}

fn record(record: Record, body: &[u8]) -> Vec<u8> {
    let mut bytes = header(record).to_vec();
    bytes.extend_from_slice(body);
    bytes
}

#[test]
fn pv3_the_reader_delivers_whole_records() {
    let line = b"loaded 1 2000";
    let mut bytes = record(
        Record::Event {
            length: line.len() as u32,
        },
        line,
    );
    bytes.extend(record(
        Record::Frame {
            width: 1,
            height: 1,
        },
        &[1, 2, 3, 255],
    ));
    assert_eq!(
        drained(&bytes),
        [
            Incoming::Event(Event::Loaded {
                video: true,
                duration_ms: 2000
            }),
            Incoming::Frame,
            Incoming::Closed
        ]
    );
}

// A corrupt or oversized header ends the helper before anything is allocated for its body.
#[test]
fn pv3_the_reader_refuses_corrupt_records() {
    let helper = Incoming::Event(Event::Failed(Failure::Helper));
    let mut magic = record(
        Record::Frame {
            width: 1,
            height: 1,
        },
        &[0; 4],
    );
    magic[0] = b'X';
    assert_eq!(drained(&magic), [helper, Incoming::Closed]);
    let mut huge = header(Record::Frame {
        width: 1,
        height: 1,
    });
    huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        drained(&huge),
        [helper, Incoming::Closed],
        "oversized refused"
    );
    let unknown = record(Record::Event { length: 7 }, b"rewound");
    assert_eq!(
        drained(&unknown),
        [helper, Incoming::Closed],
        "an unknown event ends the helper"
    );
    let invalid = record(Record::Event { length: 2 }, &[0xff, 0xfe]);
    assert_eq!(
        drained(&invalid),
        [helper, Incoming::Closed],
        "invalid text ends the helper"
    );
    let short = &record(
        Record::Frame {
            width: 2,
            height: 2,
        },
        &[0; 16],
    )[..20];
    assert_eq!(
        drained(short),
        [Incoming::Closed],
        "a truncated picture delivers nothing"
    );
    assert_eq!(drained(&[]), [Incoming::Closed]);
}

extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
}
const F_DUPFD: i32 = 0;
const F_SETFD: i32 = 2;

// bubblewrap hands on every descriptor it inherits, so one the GUI holds without close-on-exec must never reach the decoder.
#[test]
fn pv3_the_helper_inherits_no_descriptor_of_the_gui() {
    let root = Root::new();
    let input = root.0.join("clip.mp4");
    std::fs::write(&input, b"not a clip\n").expect("input");
    let held = std::fs::File::open(&input).expect("held");
    let fd = std::os::fd::AsRawFd::as_raw_fd(&held);
    // SAFETY: a copy at 10 or above, where the probe's shell keeps its script, on a descriptor this test owns.
    let high = unsafe { fcntl(fd, F_DUPFD, 10) };
    assert!(high >= 10, "a high copy of the held descriptor");
    // SAFETY: F_DUPFD returned a fresh descriptor owned exclusively by this File.
    let high_file = unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(high) };
    // SAFETY: clears close-on-exec on descriptors this test owns, as an inherited one arrives.
    assert!(unsafe { fcntl(fd, F_SETFD, 0) } >= 0);
    assert!(unsafe { fcntl(high, F_SETFD, 0) } >= 0);
    let probe = root.0.join("probe.sh");
    // A shell can reuse the closed GUI descriptor for its script, so the probe compares file identity.
    let mut body = String::from("#!/bin/sh\n");
    for each in [fd, high] {
        body.push_str(&format!(
            "if [ /proc/$$/fd/{each} -ef /input ]; then printf 'LEAKED-DESCRIPTOR-{each:04}-INSIDE'; fi\n"
        ));
    }
    std::fs::write(&probe, body).expect("probe");
    std::fs::set_permissions(&probe, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("chmod");
    let sandbox = Sandbox {
        executable: probe,
        bwrap: "bwrap".into(),
    };
    let request = Request {
        path: input,
        audio: false,
        ..request()
    };
    let (_decoder, feed) = sandbox.start(&request).expect("start");
    let mut seen = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        match feed.incoming.try_recv() {
            Ok(item) => {
                seen.push(item);
                if item == Incoming::Closed {
                    break;
                }
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
        }
    }
    drop(held);
    drop(high_file);
    assert_eq!(
        seen,
        [Incoming::Closed],
        "fd {fd} or {high} reached the sandbox"
    );
}
