use flea::ui_logic_pv3::frame::{Command, Event};
use flea_gui::g2::pv3::{
    engine::{Backend, Incoming, Request},
    sandbox::Sandbox,
};
#[path = "pv3_helper/probe.rs"]
mod probe;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
#[test]
fn preview_quality_paused_video_resizes_without_reopening() {
    if probe::skipped() {
        return;
    }
    let sandbox = Sandbox {
        executable: env!("CARGO_BIN_EXE_flea-gui").into(),
        bwrap: "bwrap".into(),
    };
    let request = Request {
        path: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pv3_helper/clip.mp4"),
        box_w: 16,
        box_h: 12,
        audio: false,
        autoplay: false,
    };
    let (mut decoder, feed) = sandbox.start(&request).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut resized = false;
    while Instant::now() < deadline {
        match feed.incoming.try_recv() {
            Ok(Incoming::Event(Event::Playing)) => panic!("resize must preserve pause"),
            Ok(Incoming::Event(Event::Failed(error))) => panic!("helper failed: {error:?}"),
            Ok(Incoming::Closed) => panic!("helper closed before refinement"),
            Ok(Incoming::Frame) => {
                if let Some(image) = feed.frames.take() {
                    let size = image.size(0);
                    let got = (size.width.0, size.height.0);
                    if !resized {
                        assert_eq!(got, (16, 12));
                        decoder.send(Command::parse("resize 32 24").unwrap());
                        resized = true;
                    } else if got == (32, 24) {
                        return;
                    }
                }
            }
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    panic!("a paused video must repaint at the new physical size");
}
