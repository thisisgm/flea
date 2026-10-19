use flea_gui::{
    ipc::server::Server,
    palette::Theme,
    window::{
        self,
        locale::Service,
        startup::{self, Arguments, Launch},
    },
};
use gpui::App;

fn main() {
    let (service, initial) = Service::start(flea::i18n::Paths::default()).unwrap_or_else(|error| {
        eprintln!("flea-gui: {error}");
        std::process::exit(2);
    });
    let locale = initial
        .recv_blocking()
        .unwrap_or_else(|_| std::process::exit(2));
    let mut args = Arguments::parse(std::env::args().skip(1)).unwrap_or_else(|error| {
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
                Theme::load(),
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
    let (launch, server, theme) = worker
        .join()
        .unwrap_or_else(|_| std::process::exit(2))
        .unwrap_or_else(|(key, error)| {
            eprintln!("flea-gui: {}", locale.catalog.tr_args(key, &[&error]));
            std::process::exit(2);
        });
    let events = server.events.clone();
    gpui_platform::application().run(move |cx: &mut App| {
        cx.set_global(theme);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        match window::open(launch, locale.clone(), events, cx) {
            Ok(window) => {
                let _ = window.update(cx, |root, _, cx| {
                    root.shell
                        .update(cx, |shell, cx| shell.follow_locale(&service, cx))
                });
                cx.activate(true);
            }
            Err(error) => {
                eprintln!(
                    "flea-gui: {}",
                    locale.catalog.tr_args("window.open_failed", &[&error])
                );
                cx.quit();
            }
        }
    });
}
