//! Native window host and composition interfaces shared by later surfaces.
pub mod columns;
pub mod composition;
pub(crate) mod dispatch;
mod events;
pub(crate) mod filter;
pub(crate) mod frame_feedback;
pub mod glass_tag;
pub mod input;
pub mod lifecycle;
#[path = "../list/join.rs"]
mod list_join;
pub mod locale;
#[cfg(test)]
mod locale_tests;
pub mod navigate;
pub mod network;
pub(crate) mod output;
pub mod overlay;
pub(crate) mod pane;
pub(crate) mod path_host;
mod presentation;
pub mod rail;
mod render;
pub mod selection_preview;
mod preview_text;
mod preview_facts;
mod preview_input;
pub mod root;
pub(crate) mod search;
pub mod settings;
pub mod startup;
pub(crate) mod status;
pub(crate) mod surfaces;
mod transitions;

#[cfg(test)]
mod u07_chrome_tests;
#[cfg(test)]
mod u07_motion_tests;
#[cfg(test)]
mod u07_tests;

use crate::{ipc::server::Event, state::Shell};
use gpui::{px, size, App, AppContext, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions};
use root::Root;
use startup::Launch;

/// The app id, which Hyprland reports as the class of this app's windows.
pub const APP_ID: &str = "flea";

pub fn open(
    launch: Launch,
    locale: locale::Snapshot,
    events: async_channel::Receiver<Event>,
    cx: &mut App,
) -> Result<WindowHandle<Root>, String> {
    let title = locale.catalog.tr("app.name");
    let options = WindowOptions {
        app_id: Some(APP_ID.into()),
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
        window.set_rem_size(crate::list::Typeface::omarchy_default().body);
        let shell = cx.new(|cx| Shell::new(launch, locale, events, window, cx));
        shell.update(cx, |shell, cx| shell.follow_settings(window, cx));
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
mod columns_tests;
#[cfg(test)]
#[path = "../list/join_tests.rs"]
mod list_tests;
#[cfg(test)]
mod ipc_cold_tests;
#[cfg(test)]
mod modal_tests;
#[cfg(test)]
mod native_reader_tests;
#[cfg(test)]
mod omi_views_tests;
#[cfg(test)]
mod overlay_escape_tests;
#[cfg(test)]
mod output_tests;
#[cfg(test)]
mod overlay_tests;
#[cfg(test)]
mod presentation_fault_tests;
#[cfg(test)]
mod presentation_tests;
#[cfg(test)]
mod rate_atomic_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod status_tests;
#[cfg(test)]
mod surfaces_tests;
#[cfg(test)]
mod text_size_keys_tests;
#[cfg(test)]
mod transition_tests;

#[cfg(test)]
mod dual_join_tests;

#[cfg(test)]
mod gcomp_tests;
#[cfg(test)]
mod view_tests;
#[cfg(test)]
#[rustfmt::skip]
mod u07_drag_review_tests;

#[cfg(test)]
mod u07_tap_tests;

#[cfg(test)]
mod native31_tests;

#[cfg(test)]
mod caption_loop_tests;
#[cfg(test)]
mod glass_tag_tests;
