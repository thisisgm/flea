//! What the host reports about one mounted window: its identity, socket and draws, and a raster capture.
use super::{
    super::wire::{capabilities, number, object, text},
    Host,
};
use flea::jsondoc::Json;
use flea_gui::window::root::Root;
use gpui::{VisualTestContext, WindowHandle};
use std::{
    fs,
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    path::PathBuf,
};

impl Host {
    pub(super) fn describe(&mut self, identity: &str) -> Result<Json, String> {
        let mounted = self
            .windows
            .get(identity)
            .ok_or("foreign or closed window")?;
        let metadata = fs::symlink_metadata(&mounted.socket).map_err(|e| e.to_string())?;
        let active = self
            .cx
            .update(|cx| mounted.window.is_active(cx))
            .ok_or("closed window")?;
        Ok(object([
            ("window", text(&mounted.identity)),
            ("active", Json::Bool(active)),
            ("gpui_window", number(mounted.window.window_id().as_u64())),
            ("pid", number(std::process::id())),
            ("root_type", text(std::any::type_name::<Root>())),
            ("socket", text(mounted.socket.display())),
            (
                "socket_identity",
                Json::Arr(vec![number(metadata.dev()), number(metadata.ino())]),
            ),
            (
                "runtime",
                text(format!(
                    "/proc/{}/fd/{}/runtime",
                    std::process::id(),
                    self.directory.as_raw_fd()
                )),
            ),
            ("draw_calls", number(mounted.draws)),
        ]))
    }

    // The next raster file of one window, named by the window and its own capture count so none is overwritten.
    fn capture_file(&mut self, identity: &str) -> PathBuf {
        let mounted = self.windows.get_mut(identity).expect("a mounted window");
        mounted.captures += 1;
        self.root.join(format!(
            "capture-{}-{}.rgba",
            mounted.window.window_id().as_u64(),
            mounted.captures
        ))
    }

    // One raster of the target window, in a file no earlier capture used.
    pub(super) fn capture(&mut self, identity: &str, window: WindowHandle<Root>) -> Json {
        let file = self.capture_file(identity);
        let mut visual = VisualTestContext::from_window(window.into(), &self.cx);
        let raster = visual.update(|window, _| match window.render_to_image() {
            Ok(image) => match fs::write(&file, image.as_raw()) {
                Ok(()) => object([
                    ("status", text("available")),
                    ("path", text(file.display())),
                    ("width", number(image.width())),
                    ("height", number(image.height())),
                ]),
                Err(error) => object([("status", text("unsupported")), ("reason", text(error))]),
            },
            Err(error) => object([("status", text("unsupported")), ("reason", text(error))]),
        });
        capabilities(raster)
    }
}

#[test]
fn uish_driver_each_capture_of_a_window_takes_its_own_name() {
    let sandbox = super::super::Sandbox::new("capture-names");
    let fixture = sandbox.0.join("fixture");
    fs::create_dir(&fixture).unwrap();
    let mut host = Host::at(&sandbox.0);
    let opened = host
        .command(&object([
            ("op", text("open")),
            ("path", text(fixture.display())),
        ]))
        .unwrap();
    let identity = opened
        .get("window")
        .and_then(Json::as_str)
        .unwrap()
        .to_owned();
    let window_id = identity.rsplit(':').next().unwrap().to_owned();
    let capture = object([("op", text("capabilities")), ("window", text(&identity))]);
    let paths: Vec<_> = (0..2)
        .map(|_| {
            let reply = host.command(&capture).unwrap();
            let raster = reply.get("raster").unwrap().clone();
            raster.get("path").and_then(Json::as_str).map(PathBuf::from)
        })
        .collect();
    assert_eq!(
        host.capture_file(&identity),
        host.root.join(format!("capture-{window_id}-3.rgba")),
        "each capabilities op takes the next capture name"
    );
    assert_capture_pair(&paths);
    host.teardown().unwrap();
}

fn assert_capture_pair(paths: &[Option<PathBuf>]) {
    match paths {
        [Some(first), Some(second)] => {
            assert!(first.exists() && second.exists() && first != second);
        }
        [None, None] => {}
        other => panic!("one capture of two came back without a file: {other:?}"),
    }
}

#[test]
#[should_panic(expected = "one capture of two came back without a file")]
fn uish_driver_rejects_a_missing_second_capture() {
    assert_capture_pair(&[Some(PathBuf::from("capture-1-1.rgba")), None]);
}

#[test]
#[should_panic(expected = "one capture of two came back without a file")]
fn uish_driver_rejects_a_missing_first_capture() {
    assert_capture_pair(&[None, Some(PathBuf::from("capture-1-2.rgba"))]);
}
