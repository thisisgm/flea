//! Native window host and composition interfaces shared by later surfaces.
pub mod composition;
mod events;
mod input;
pub mod lifecycle;
pub mod locale;
pub(crate) mod pane;
mod render;
pub mod startup;

use crate::{ipc::server::Event, state::Shell};
use gpui::{px, size, App, AppContext, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions};
use startup::Launch;

pub fn open(
    launch: Launch,
    locale: locale::Snapshot,
    events: async_channel::Receiver<Event>,
    cx: &mut App,
) -> Result<WindowHandle<Shell>, String> {
    let title = locale.catalog.tr("app.name");
    let options = WindowOptions {
        app_id: Some("flea".into()),
        window_bounds: Some(WindowBounds::centered(size(px(900.), px(600.)), cx)),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        let shell = cx.new(|cx| Shell::new(launch, locale, events, window, cx));
        let weak = shell.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |shell, cx| {
                shell.close(window, cx);
            })
            .is_err()
        });
        shell
    })
    .map_err(|error| error.to_string())
}
