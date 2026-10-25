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
fn uish_driver_second_capture_of_a_window_keeps_the_first() {
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
    let identity = opened.get("window").and_then(Json::as_str).unwrap();
    let (first, second) = (host.capture_file(identity), host.capture_file(identity));
    assert_ne!(first, second, "a second capture overwrote the first");
    assert!(first.starts_with(&host.root) && second.starts_with(&host.root));
    host.teardown().unwrap();
}
