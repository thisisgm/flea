mod backend;
mod defaults;
mod error;
mod gui;
mod heap;
mod hyprkeys;
mod json;
mod launcher;
mod open;
mod paths;
mod thp;
mod userfile;
mod wol;

use crate::backend::proto::error_line;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::exit;

fn usage(message: &str) -> ! {
    eprintln!("flea: {}", message);
    eprintln!("usage: flea [--tui|--gui] [--select <uri|path>] [path]");
    eprintln!("       flea --default [off]");
    eprintln!("       flea --version");
    exit(2)
}

// A reveal names a file; the window opens on its parent with that entry selected.
fn select_target(raw: &str) -> Option<(PathBuf, PathBuf)> {
    let path = if let Some(rest) = raw.strip_prefix("file://") {
        PathBuf::from(paths::percent_decode(rest))
    } else {
        PathBuf::from(raw)
    };
    let parent = path.parent()?.to_path_buf();
    Some((parent, path))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Bare, so a script can read it without parsing. Checked before every other mode: the only
    // way to tell which Flea is installed is to ask it, and updates here are a manual git pull.
    if args.iter().any(|a| a == "--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        exit(0);
    }

    if args.iter().any(|a| a == "--backend") {
        exit(backend::run::run());
    }

    // flea --prewarm <path> <count> <dest>
    if args.len() == 5 && args[1] == "--prewarm" {
        let first: usize = args[3].parse().unwrap_or(0);
        match launcher::prewarm::write_prewarm(&args[2], first, &PathBuf::from(&args[4])) {
            Ok(()) => exit(0),
            Err(e) => {
                eprintln!("{}", error_line(&e));
                exit(1);
            }
        }
    }

    // flea --open <path>
    if args.len() == 3 && args[1] == "--open" {
        exit(open::open(&args[2]));
    }

    // flea --wake <mac>: internal GUI action, kept argv-direct so a hostile value never reaches a shell.
    if args.len() == 3 && args[1] == "--wake" {
        match wol::wake(&args[2]) {
            Ok(()) => exit(0),
            Err(message) => {
                eprintln!("flea: {message}");
                exit(1);
            }
        }
    }

    // flea --default [off]: the one per-user step pacman cannot own, see docs/install.md.
    if args.len() == 2 && args[1] == "--default" {
        exit(defaults::claim());
    }
    if args.len() == 3 && args[1] == "--default" && args[2] == "off" {
        exit(defaults::release());
    }
    if args.get(1).map(String::as_str) == Some("--default") {
        usage("--default takes nothing, or off");
    }

    let mut want_tui = false;
    let mut want_gui = false;
    let mut print_target = false;
    let mut select_raw: Option<String> = None;
    let mut start: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--tui" => want_tui = true,
            "--gui" => want_gui = true,
            "--print-target" => print_target = true,
            "--select" => {
                i += 1;
                match args.get(i) {
                    Some(v) => select_raw = Some(v.clone()),
                    None => usage("--select needs a target"),
                }
            }
            a if a.starts_with("--") => usage(&format!("unknown flag {}", a)),
            a => start = start.or_else(|| Some(a.to_string())),
        }
        i += 1;
    }
    if want_tui && want_gui {
        usage("--tui and --gui are mutually exclusive");
    }

    // Test-only: prints the resolved (parent, target) pair and exits, so a reveal is testable without a window.
    if print_target {
        match select_raw.as_deref().and_then(select_target) {
            Some((parent, target)) => {
                println!("{} {}", parent.display(), target.display());
                exit(0);
            }
            None => usage("--print-target needs --select <uri|path> naming a target with a parent"),
        }
    }

    // A reveal opens the target's directory; a missing target still opens it, with nothing selected.
    let (open_path, select_path) = match select_raw.as_deref().and_then(select_target) {
        Some((parent, target)) => (Some(parent.to_string_lossy().into_owned()), Some(target.to_string_lossy().into_owned())),
        None => (start, None),
    };

    // Both handles must be a tty, so a pipeline never receives the terminal interface.
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let tui = if want_tui { true } else if want_gui { false } else { interactive };

    if tui {
        if !interactive {
            eprintln!("flea: the terminal interface needs a terminal on stdin and stdout");
            exit(2);
        }
        eprintln!("flea: the terminal interface is not built yet, use --gui");
        exit(2);
    }

    if !paths::has_display() {
        eprintln!("flea: there is no graphical session to open a window in");
        exit(2);
    }
    match paths::ui_dir() {
        Some(ui) => exit(gui::exec_qs(&ui, open_path.as_deref(), select_path.as_deref())),
        None => {
            eprintln!("flea: the shell config is missing, set FLEA_UI or install /usr/share/flea/ui");
            exit(2);
        }
    }
}
