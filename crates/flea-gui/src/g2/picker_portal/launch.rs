//! flea --pick under the GPUI frontend: one portal request, one window, its reply and its exit.
use super::drawn::Drawn;
use super::geometry::Live;
use super::picker::{Children, Picker};
use super::process::PickerProcess;
use crate::ipc::server::{Event, Server};
use crate::window::locale::{Service, Snapshot};
use flea::ui_logic_picker_portal::{self as logic, Request};
use gpui::{
    px, size, App, AppContext, Context, Task, TitlebarOptions, WindowBounds, WindowHandle,
    WindowOptions,
};
use std::path::PathBuf;

// The app id one Hyprland rule floats, the one flea --picker writes.
pub const APP_ID: &str = "com.thisisgm.flea.picker";
// Theme.space(640) times the dialog width ratio 9/8, and Theme.space(440), at the base text size.
const WIDTH: f32 = 720.;
const HEIGHT: f32 = 440.;

fn finish_opening<T>(
    opened: Result<T, String>,
    catalog: &flea::i18n::I18n,
    cx: &mut App,
) -> Result<T, ()> {
    opened.map_err(|error| {
        eprintln!(
            "flea-gui: {}",
            catalog.tr_args("window.open_failed", &[&error])
        );
        crate::driver_notice::DriverNotice::new(catalog).send();
        cx.spawn(async move |cx| {
            cx.update(|cx| cx.quit());
        })
        .detach();
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct PickLaunch {
    pub req: Request,
    pub reply: PathBuf,
    pub home: String,
}

impl PickLaunch {
    // Empty is absent, the rule src/gui.rs pick() applies; the returned key names the refusal.
    pub fn resolve(
        picker: Option<&str>,
        reply: &str,
        home: Option<&str>,
    ) -> Result<Self, &'static str> {
        let picker = picker
            .filter(|text| !text.is_empty())
            .ok_or("picker.portal.no_request")?;
        if reply.is_empty() {
            return Err("picker.portal.no_reply");
        }
        Ok(Self {
            req: logic::request(picker),
            reply: PathBuf::from(reply),
            home: home
                .filter(|home| !home.is_empty())
                .unwrap_or("/")
                .to_owned(),
        })
    }
}

pub fn open(
    launch: PickLaunch,
    locale: Snapshot,
    children: Children,
    cx: &mut App,
) -> Result<WindowHandle<Picker>, String> {
    let title = super::labels::Labels::new(&locale.catalog, &launch.req).title;
    let options = WindowOptions {
        app_id: Some(APP_ID.into()),
        window_bounds: Some(WindowBounds::centered(size(px(WIDTH), px(HEIGHT)), cx)),
        titlebar: Some(TitlebarOptions {
            title: Some(title.clone()),
            ..Default::default()
        }),
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        // Set again on the live window, so every platform's toplevel carries the caller's title.
        window.set_window_title(&title);
        // The picker's menus draw the active keyboard group's keycaps from the first frame.
        crate::g2::context_menus::keycaps::watch(cx);
        let picker =
            cx.new(|cx| Picker::new(launch.req, launch.reply, &launch.home, locale, children, cx));
        let weak = picker.downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            weak.update(cx, |picker, cx| picker.window_closed(cx))
                .unwrap_or(true)
        });
        let focus = picker.read(cx).focus.clone();
        window.focus(&focus, cx);
        picker
    })
    .map_err(|error| error.to_string())
}

// The rounding reader and the corners it publishes; the service stops when this is dropped.
pub(super) type CornerSource = (
    Option<crate::compositor::Service>,
    async_channel::Receiver<f32>,
);

// The chooser is its own process, so it reads Hyprland's rounding and follows reloads as the Shell's settings service does.
pub(super) fn corner_source(socket: Option<PathBuf>, command: PathBuf) -> CornerSource {
    let (send, corners) = async_channel::unbounded();
    let shown = command.display().to_string();
    let service = crate::compositor::Service::at(socket, command, move |corner| {
        send.send_blocking(corner).is_ok()
    })
    .map_err(|error| {
        eprintln!("flea: the chooser's rounding reader for {shown} did not start: {error}")
    })
    .ok();
    (service, corners)
}

// Each rounding the source reads becomes the chooser's corner global; the reader holds the service for the window's life.
pub(super) fn follow_corner((service, corners): CornerSource, cx: &mut App) -> Task<()> {
    cx.spawn(async move |cx| {
        let _service = service;
        while let Ok(corner) = corners.recv().await {
            cx.update(|cx| {
                cx.set_global(crate::compositor::Corner(corner));
                cx.refresh_windows();
            });
        }
    })
}

// A language change reaches the open window's words without a frame-time catalog read.
pub fn follow_locale(service: &Service, cx: &mut Context<Picker>) -> Task<()> {
    let updates = service.subscribe();
    cx.spawn(async move |this, cx| {
        while let Ok(snapshot) = updates.recv().await {
            if this
                .update(cx, |picker, cx| picker.set_locale(snapshot, cx))
                .is_err()
            {
                break;
            }
        }
    })
}

// The fleapicker target tests/picker.sh drives; a quit call closes the way the window manager's close does.
pub fn serve_ipc(
    events: async_channel::Receiver<Event>,
    keep: impl Sized + 'static,
    cx: &mut Context<Picker>,
) -> Task<()> {
    cx.spawn(async move |this, cx| {
        let _keep = keep;
        while let Ok(event) = events.recv().await {
            let served = this.update(cx, |picker, cx| match event {
                Event::Call { request, answer } => {
                    let drawn = picker.drawn.clone();
                    if Drawn::reads_frame(&request) {
                        drawn.arm();
                        let weak = cx.entity().downgrade();
                        drawn.when_painted(move |cx| {
                            let _ = weak.update(cx, |picker, cx| {
                                let geometry = Live { picker, cx };
                                let _ = answer.try_send(picker.ipc(request, Some(&geometry)));
                            });
                        });
                        cx.notify();
                    } else {
                        let _ = answer.try_send(picker.ipc(request, None));
                    }
                }
                Event::Quit => {
                    picker.window_closed(cx);
                }
            });
            if served.is_err() {
                break;
            }
        }
    })
}

// The listing worker draws the chooser's folder and answers its typed paths, reaped under the same lifetime as the checks.
pub fn children() -> Children {
    Children {
        checks: Box::new(PickerProcess::checks()),
        listing: Box::new(PickerProcess::listing()),
    }
}

// Sample argv: flea-gui --pick /run/user/1000/flea-picker-x/reply.json, with FLEA_PICKER set by tools/flea-portal.
pub fn main(reply: &str, service: Service, locale: Snapshot) -> i32 {
    let picker = std::env::var("FLEA_PICKER").ok();
    let home = std::env::var("HOME").ok();
    let launch = match PickLaunch::resolve(picker.as_deref(), reply, home.as_deref()) {
        Ok(launch) => launch,
        Err(key) => {
            eprintln!("flea-gui: {}", locale.catalog.tr(key));
            return 2;
        }
    };
    run(
        launch,
        service,
        locale,
        gpui_platform::application(),
        |launch, locale, cx| open(launch, locale, children(), cx),
    )
}

fn run(
    launch: PickLaunch,
    service: Service,
    locale: Snapshot,
    application: gpui::Application,
    opener: impl FnOnce(PickLaunch, Snapshot, &mut App) -> Result<WindowHandle<Picker>, String>
        + 'static,
) -> i32 {
    // Bound before the event loop; a chooser without its test endpoint still answers its caller.
    let server = flea::ipc::endpoint::runtime()
        .and_then(|runtime| Server::bind_picker(&runtime))
        .map_err(|error| {
            eprintln!(
                "flea-gui: {}",
                locale
                    .catalog
                    .tr_args("ipc.start_failed", &[&error.to_string()])
            )
        })
        .ok();
    let failed = std::rc::Rc::new(std::cell::Cell::new(false));
    let failure = failed.clone();
    let settings = crate::window::startup::load_settings()
        .map(|settings| settings.data)
        .unwrap_or_else(|_| flea::uistate::from_file("{}"));
    // The chooser draws in the system font as the window does, so it resolves fontconfig's family before its first frame.
    let (fonts, presentation) = crate::presentation::Service::start();
    application.run(move |cx: &mut App| {
        presentation.install(cx);
        crate::presentation::epoch::launch(&locale.catalog, &presentation.fonts.family, cx);
        cx.set_global(presentation.fonts.typeface(&settings));
        cx.set_global(super::settings::HiddenSeed::read(&settings));
        follow_corner(
            corner_source(crate::compositor::socket(), "hyprctl".into()),
            cx,
        )
        .detach();
        let updates = fonts.subscribe();
        let settled = settings.clone();
        cx.spawn(async move |cx| {
            // The service watches only while it is held, so the reader keeps it for the window's life.
            let _watching = fonts;
            while let Ok(snapshot) = updates.recv().await {
                let face = snapshot.fonts.typeface(&settled);
                cx.update(|cx| {
                    snapshot.install(cx);
                    cx.set_global(face);
                    cx.refresh_windows();
                });
            }
        })
        .detach();
        match finish_opening(opener(launch, locale.clone(), cx), &locale.catalog, cx) {
            Ok(window) => {
                let _ = window.update(cx, |picker, _, cx| {
                    picker.apply_settings(&settings, cx);
                    picker.start_places(&settings, cx);
                    picker.follow_settings(cx);
                    let reader = follow_locale(&service, cx);
                    picker.keep(reader);
                    if let Some(server) = server {
                        let events = server.events.clone();
                        let served = serve_ipc(events, server, cx);
                        picker.keep(served);
                    }
                });
                cx.activate(true);
            }
            Err(()) => {
                // No window is a load that failed: no reply is written, and the portal answers the caller with a fault.
                failure.set(true);
            }
        }
    });
    i32::from(failed.get())
}

#[cfg(test)]
#[path = "launch_gap64_tests.rs"]
mod gap64_tests;
