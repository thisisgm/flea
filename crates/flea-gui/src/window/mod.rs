//! Native window host and composition interfaces shared by later surfaces.
pub mod composition;
mod events;
pub mod input;
pub mod lifecycle;
pub mod locale;
pub mod navigate;
pub mod overlay;
pub(crate) mod pane;
mod render;
pub mod root;
pub mod startup;
pub(crate) mod surfaces;
mod transitions;

use crate::{ipc::server::Event, state::Shell};
use gpui::{px, size, App, AppContext, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions};
use root::Root;
use startup::Launch;

pub fn open(
    launch: Launch,
    locale: locale::Snapshot,
    events: async_channel::Receiver<Event>,
    cx: &mut App,
) -> Result<WindowHandle<Root>, String> {
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
        if std::env::var_os("FLEA_GUI_RENDER_DIAGNOSTICS").is_some() {
            if let Some(specs) = window.gpu_specs() {
                eprintln!(
                    "flea-render: device={:?} driver={:?} info={:?} software={}",
                    specs.device_name,
                    specs.driver_name,
                    specs.driver_info,
                    specs.is_software_emulated
                );
            }
        }
        let shell = cx.new(|cx| Shell::new(launch, locale, events, window, cx));
        let weak = shell.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |shell, cx| {
                shell.close(window, cx);
            })
            .is_err()
        });
        cx.new(|_| Root { shell })
    })
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod modal_tests;
#[cfg(test)]
mod overlay_escape_tests;
#[cfg(test)]
mod overlay_tests;
#[cfg(test)]
mod surfaces_tests;
#[cfg(test)]
mod transition_tests;
