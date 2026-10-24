use flea_gui::{
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

mod renderer;

#[cfg(test)]
mod renderer_tests;

fn finish_opening<T>(opened: Result<T, String>) -> T {
    opened.unwrap_or_else(|error| {
        renderer::unavailable(&error);
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        // The startup callback precedes the event loop, which clears an earlier quit request.
        std::process::exit(renderer::UNAVAILABLE);
    })
}

fn main() {
    let words: Vec<String> = std::env::args().skip(1).collect();
    if let Some(status) = flea_gui::g2::pv3::dispatch(&words) {
        std::process::exit(status);
    }
    let (service, initial) = Service::start(flea::i18n::Paths::default()).unwrap_or_else(|error| {
        eprintln!("flea-gui: {error}");
        std::process::exit(2);
    });
    let locale = initial
        .recv_blocking()
        .unwrap_or_else(|_| std::process::exit(2));
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("--pick") {
        let reply = if argv.len() == 2 {
            argv[1].as_str()
        } else {
            ""
        };
        std::process::exit(flea_gui::g2::pk1::launch::main(reply, service, locale));
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
            Ok::<_, (&str, String)>((
                Launch::resolve(settings, args, &home),
                server,
                presentation::Service::start(),
            ))
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
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let window = finish_opening(window::open(launch, locale, events, cx));
        let _ = window.update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.follow_locale(&service, cx);
                shell.follow_presentation(&presentation_service, window, cx);
            })
        });
        cx.activate(true);
    });
}
