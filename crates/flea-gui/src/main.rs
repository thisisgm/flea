use flea_gui::state;
use gpui::{prelude::*, px, size, App, TitlebarOptions, WindowBounds, WindowOptions};
use std::path::PathBuf;

fn arguments() -> Result<(String, Option<String>), String> {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut select = None;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--select" => select = Some(args.next().ok_or("--select needs a target")?),
            "--" => {
                path = args.next();
                if args.next().is_some() {
                    return Err("only one start directory is accepted".into());
                }
                break;
            }
            flag if flag.starts_with('-') => return Err(format!("unknown flag {flag}")),
            _ if path.is_none() => path = Some(argument),
            _ => return Err("only one start directory is accepted".into()),
        }
    }
    let path = path
        .or_else(|| {
            std::env::var("FLEA_PATH")
                .ok()
                .filter(|value| !value.is_empty())
        })
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_else(|| "/".into());
    let select = select.or_else(|| {
        std::env::var("FLEA_SELECT")
            .ok()
            .filter(|value| !value.is_empty())
    });
    let absolute = if PathBuf::from(&path).is_absolute() {
        PathBuf::from(path)
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    Ok((absolute.to_string_lossy().into_owned(), select))
}

fn main() {
    let (path, select) = arguments().unwrap_or_else(|error| {
        eprintln!("flea-gui: {error}");
        std::process::exit(2);
    });
    gpui_platform::application().run(move |cx: &mut App| {
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let options = WindowOptions {
            app_id: Some("flea".into()),
            window_bounds: Some(WindowBounds::centered(size(px(900.), px(640.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("Flea".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        if let Err(error) = cx.open_window(options, |window, cx| {
            cx.new(|cx| state::Shell::new(path, select, window, cx))
        }) {
            eprintln!("flea-gui: the window could not open: {error}");
            cx.quit();
        }
        cx.activate(true);
    });
}
