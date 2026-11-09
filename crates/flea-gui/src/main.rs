use flea_gui::{
    driver_notice::DriverNotice,
    ipc::server::Server,
    presentation,
    window::{
        self,
        locale::Service,
        startup::{self, Arguments, Launch},
    },
};
use gpui::App;
use std::io::Write;

mod backend_log;
mod renderer;

#[cfg(test)]
mod renderer_tests;

#[cfg(test)]
mod backend_log_tests;

fn finish_opening<T>(opened: Result<T, String>, notice: &DriverNotice) -> T {
    opened.unwrap_or_else(|error| {
        renderer::unavailable(&error, &backend_log::take_refusals(), notice);
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        // The startup callback precedes the event loop, which clears an earlier quit request.
        std::process::exit(renderer::UNAVAILABLE);
    })
}

fn main() {
    let unlock_device = std::env::var("FLEA_UNLOCK")
        .ok()
        .filter(|value| !value.is_empty());
    // Consume the one-window request before any service starts child processes or threads.
    std::env::remove_var("FLEA_UNLOCK");
    let words: Vec<String> = std::env::args().skip(1).collect();
    if let Some(status) = flea_gui::g2::media_preview::dispatch(&words) {
        std::process::exit(status);
    }
    if let Err(error) = backend_log::install() {
        eprintln!("flea: graphics backend reporting could not start: {error}");
    }
    #[cfg(test)]
    if backend_log_tests::emit_if_requested() {
        return;
    }
    // Vulkan instance creation is the slowest step of opening a window; overlap it with the startup below.
    gpui_platform::prewarm_gpu();
    let (service, locale) = Service::launch(flea::i18n::Paths::default()).unwrap_or_else(|error| {
        eprintln!("flea-gui: {error}");
        std::process::exit(2);
    });
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("--pick") {
        let reply = if argv.len() == 2 {
            argv[1].as_str()
        } else {
            ""
        };
        std::process::exit(flea_gui::g2::picker_portal::launch::main(
            reply, service, locale,
        ));
    }
    let mut args = Arguments::parse(argv.into_iter()).unwrap_or_else(|error| {
        eprintln!(
            "flea-gui: {}",
            locale.catalog.tr_args(error.key, &[&error.argument])
        );
        std::process::exit(2);
    });
    let worker = std::thread::Builder::new()
        .name("flea-startup".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let devices = flea_gui::g2::device_rail::source::seed::start();
            let settings =
                startup::load_settings().map_err(|error| ("window.settings_failed", error))?;
            let cwd = std::env::current_dir()
                .map_err(|error| ("window.settings_failed", error.to_string()))?;
            args.absolutize(&cwd);
            let home = std::env::var("HOME")
                .ok()
                .filter(|home| !home.is_empty())
                .unwrap_or_else(|| "/".into());
            let server = flea::ipc::endpoint::runtime()
                .and_then(|runtime| Server::bind(&runtime))
                .map_err(|error| ("ipc.start_failed", error.to_string()))?;
            let mut launch = Launch::resolve(settings, args, &home);
            launch.unlock = unlock_device;
            flea_gui::g2::device_rail::source::seed::finish(devices, started);
            Ok::<_, (&str, String)>((launch, server, presentation::Service::start()))
        })
        .unwrap_or_else(|error| {
            eprintln!(
                "flea-gui: {}",
                locale
                    .catalog
                    .tr_args("window.settings_failed", &[&error.to_string()])
            );
            std::process::exit(2);
        });
    let (launch, server, (presentation_service, presentation)) = worker
        .join()
        .unwrap_or_else(|_| std::process::exit(2))
        .unwrap_or_else(|(key, error)| {
            eprintln!("flea-gui: {}", locale.catalog.tr_args(key, &[&error]));
            std::process::exit(2);
        });
    let events = server.events.clone();
    gpui_platform::application().run(move |cx: &mut App| {
        cx.set_text_rendering_mode(gpui::TextRenderingMode::Grayscale);
        presentation.install(cx);
        presentation::epoch::launch(&locale.catalog, &presentation.fonts.family, cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let notice = DriverNotice::new(&locale.catalog);
        let window = finish_opening(window::open(launch, locale, events, cx), &notice);
        window::glass_tag::start(presentation.theme.glass);
        let _ = window.update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.follow_locale(&service, cx);
                shell.follow_presentation(&presentation_service, window, cx);
            })
        });
        cx.activate(true);
    });
    window::glass_tag::stop();
    flea_gui::g2::doc_preview::reap();
    // A quit can end the event loop before any window drops, so owly's runs give Wi-Fi back here, within the bound.
    use flea_gui::g2::send_share::airdrop_pids;
    airdrop_pids::teardown(airdrop_pids::TEARDOWN);
}
