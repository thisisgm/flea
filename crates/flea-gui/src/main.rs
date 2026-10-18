use flea_gui::{palette::Theme, state};
use gpui::{prelude::*, px, size, App, TitlebarOptions, WindowBounds, WindowOptions};
use std::path::PathBuf;

fn arguments() -> Result<(String, Option<String>), String> {
    parse_arguments(std::env::args().skip(1))
}

fn parse_arguments(
    mut args: impl Iterator<Item = String>,
) -> Result<(String, Option<String>), String> {
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
    let path = match path {
        Some(path) => PathBuf::from(path),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    let absolute = if path.is_absolute() {
        path
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
    run(path, select, Theme::load());
}

fn run(path: String, select: Option<String>, theme: Theme) {
    gpui_platform::application().run(move |cx: &mut App| {
        cx.set_global(theme);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adv_e_r1_argv_ignores_parent_path_and_selection() {
        const CHILD: &str = "FLEA_TEST_ARGV_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tests::adv_e_r1_argv_ignores_parent_path_and_selection",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("FLEA_PATH", "/stale-parent/path")
                .env("FLEA_SELECT", "/stale-parent/selection")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let explicit =
            parse_arguments(["--", "/explicit/path"].map(str::to_owned).into_iter()).unwrap();
        assert_eq!(explicit, ("/explicit/path".into(), None));
        let bare = parse_arguments(std::iter::empty()).unwrap();
        assert_eq!(
            bare,
            (
                std::env::current_dir()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                None
            )
        );
        let selected = parse_arguments(
            ["--select", "/explicit/file", "--", "/explicit"]
                .map(str::to_owned)
                .into_iter(),
        )
        .unwrap();
        assert_eq!(
            selected,
            ("/explicit".into(), Some("/explicit/file".into()))
        );
    }
}
