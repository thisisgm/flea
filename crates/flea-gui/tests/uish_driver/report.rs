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

    // One raster of the target window, named after that window so two captures never collide.
    pub(super) fn capture(&mut self, window: WindowHandle<Root>) -> Json {
        let file = self
            .root
            .join(format!("capture-{}.rgba", window.window_id().as_u64()));
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
