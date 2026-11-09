//! Native window host and composition interfaces shared by later surfaces.
pub(crate) mod chrome_move;
#[cfg(test)]
mod chrome_move_tests;
pub mod columns;
pub mod composition;
pub(crate) mod devices;
pub(crate) mod dispatch;
mod dual_edge;
#[cfg(test)]
mod dual_edge_tests;
mod events;
pub(crate) mod external_action;
pub(crate) mod external_action_ipc;
pub(crate) mod filter;
pub(crate) mod frame_feedback;
mod frame_style;
pub(crate) mod glass;
pub mod glass_tag;
mod gpu_report;
pub mod input;
pub(crate) mod key_delivery;
pub mod lifecycle;
#[path = "../list/join.rs"]
mod list_join;
pub mod locale;
#[cfg(test)]
mod locale_tests;
pub mod navigate;
pub mod network;
pub(crate) mod open_with;
pub(crate) mod outline;
pub(crate) mod output;
pub mod overlay;
pub(crate) mod pane;
pub(crate) mod path_host;
pub(crate) mod permissions;
#[cfg(test)]
mod permissions_tests;
mod presentation;
mod preview_facts;
mod preview_input;
mod preview_locale;
pub(crate) mod properties;
pub(crate) mod quick_look;
pub mod rail;
pub(crate) mod recent;
pub(crate) mod rename;
mod render;
pub(crate) mod retry;
pub mod root;
pub(crate) mod run_card;
pub(crate) mod search;
pub mod selection_preview;
pub mod settings;
pub(crate) mod sidebar_edge;
mod sniff;
pub mod startup;
pub(crate) mod status;
pub(crate) mod surfaces;
#[path = "../state/text_input.rs"]
pub(crate) mod text_input;
mod transitions;
mod unlock;
pub(crate) mod user_keys;
#[cfg(test)]
mod user_keys_tests;
#[cfg(test)]
mod vf6_tests;

#[cfg(test)]
pub(crate) mod fixchrome_tests;
#[cfg(test)]
mod fixreaders_cohesion_tests;
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
pub const APP_ID: &str = "com.thisisgm.flea";

pub fn open(
    launch: Launch,
    locale: locale::Snapshot,
    events: async_channel::Receiver<Event>,
    cx: &mut App,
) -> Result<WindowHandle<Root>, String> {
    let title = locale.catalog.tr("app.name");
    // Every empty hero reads Settings > View's two empty folder rows from the first frame.
    crate::g2::status_feedback::hero::EmptyLook::apply(&launch.settings.data, cx);
    let appearance = cx
        .try_global::<crate::palette::Theme>()
        .copied()
        .unwrap_or_default()
        .glass
        .appearance();
    let options = WindowOptions {
        window_background: appearance,
        app_id: Some(APP_ID.into()),
        window_bounds: Some(WindowBounds::centered(size(px(900.), px(600.)), cx)),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        glass::mount(window, appearance, cx);
        // Wayland holds its callbacks through draw, so the first presented-frame hook must be taken before any frame.
        window.observe_frame_presented(cx, |_, _| {}).detach();
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
        // Every menu's key hints draw the active keyboard group from the first frame.
        crate::g2::context_menus::keycaps::watch(cx);
        let shell = cx.new(|cx| Shell::new(launch, locale, events, window, cx));
        shell.update(cx, |shell, cx| {
            shell.follow_settings(window, cx);
            shell.follow_media_volume(window, cx);
            shell.follow_file_drags(window, cx);
            shell.start_update_polling(window, cx);
        });
        gpu_report::start(&shell, window, cx);
        let weak = shell.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |shell, cx| {
                shell.forget_file_drags(window, cx);
                shell.carry_window_ended(window, cx);
                shell.close(window, cx);
            })
            .is_err()
        });
        cx.new(|_| Root { shell })
    })
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod columns_hover_uish_tests;
#[cfg(test)]
mod columns_key_repeat_tests;
#[cfg(test)]
pub(crate) mod columns_tests;
#[cfg(test)]
mod columns_uish_tests;
#[cfg(test)]
mod cursor_focus_uish_tests;
#[cfg(test)]
mod i50_ime_tests;
#[cfg(test)]
mod ipc_cold_tests;
#[cfg(test)]
#[path = "../list/join_tests.rs"]
mod list_tests;
#[cfg(test)]
mod media_volume_tests;
#[cfg(test)]
mod modal_tests;
#[cfg(test)]
mod native_reader_tests;
#[cfg(test)]
mod omi_views_tests;
#[cfg(test)]
mod output_tests;
#[cfg(test)]
mod overlay_escape_tests;
#[cfg(test)]
mod overlay_tests;
#[cfg(test)]
mod presentation_fault_tests;
#[cfg(test)]
mod presentation_tests;
#[cfg(test)]
mod rate_atomic_tests;
#[cfg(test)]
mod scrollkeys_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod status_selection_tests;
#[cfg(test)]
mod status_tests;
#[cfg(test)]
mod surfaces_tests;
#[cfg(test)]
mod text_size_keys_tests;
#[cfg(test)]
mod transition_tests;
#[cfg(test)]
mod trash_monitor_status_tests;

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
mod ey200_long_press_tests;
#[cfg(test)]
mod u07_tap_tests;

#[cfg(test)]
mod native31_tests;

#[cfg(test)]
mod caption_loop_tests;
#[cfg(test)]
mod glass_rule_tests;
#[cfg(test)]
mod glass_tag_tests;

#[cfg(test)]
mod glass_tests;
#[cfg(test)]
mod pb_tags_tests;

pub(crate) mod git_status;
mod preview_text_selection;
