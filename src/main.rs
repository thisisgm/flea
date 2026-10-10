mod backend;
mod chooser;
mod clip;
mod defaults;
mod error;
mod gui;
mod heap;
mod hyprkeys;
mod json;
mod jsondoc;
mod jsonstring;
mod launcher;
mod oflags;
mod open;
mod paths;
mod prefetch;
mod qsregistry;
mod tearoff;
mod terminal;
mod tui;
mod thp;
mod uischema;
mod uimigrate;
mod uistate;
mod favourites;
mod figurebuild;
mod figurecache;
mod figurehelper;
mod figurestore;
mod gvfsprefetch;
mod captures;
mod shelf;
mod shelfcli;
mod shelfdrag;
mod shelfops;
mod shelfplaces;
mod shelfplugin;
mod shelfthumb;
mod shelfundo;
mod shelfzip;
mod summon;
mod uistore;
mod update;
mod userfile;
mod vulkan;

use crate::backend::proto::error_line;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::exit;

// --default owns both per-user steps, because a user updating from 0.1.3 has no picker routing yet.
fn claim_both() -> i32 {
    let handler = defaults::claim();
    // A refused handler claim has claimed nothing, so the picker half must not write either.
    if handler != 0 {
        return handler;
    }
    // A source build has no flea.portal to prefer, which is the picker's precondition, not a failure here.
    let status = if chooser::backend_installed() {
        chooser::claim()
    } else {
        eprintln!("flea: no portal backend is installed, so the file chooser step was skipped");
        0
    };
    // The one undo line this invocation ends on: --default off releases both halves it just claimed.
    println!("undo both with: flea --default off");
    status
}

// flea --picker on its own, so it owns the undo line --default must not print for it.
fn claim_picker() -> i32 {
    let installed = chooser::backend_installed();
    let status = chooser::claim();
    if installed {
        println!("undo both with: flea --picker off");
    }
    status
}

fn release_both() -> i32 {
    let handler = defaults::release();
    let picker = chooser::release();
    handler.max(picker)
}

fn usage(message: &str) -> ! {
    eprintln!("flea: {}", message);
    eprintln!("usage: flea [--tui|--gui] [--select <uri|path>] [path]");
    eprintln!("       flea --default [off]");
    eprintln!("       flea --picker [off]");
    eprintln!("       flea --ui-state [<json patch>]");
    eprintln!("       flea --update [check]");
    eprintln!("       flea --clip get|set copy|cut|clear TOKEN");
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

// flea --ui-state, the one path both front ends reach the state file through: no argument reads it,
// one JSON object merges that patch through the lock. Either way the resulting document is printed.
fn ui_state(args: &[String]) -> i32 {
    let store = match uistore::Store::user() {
        Ok(store) => store,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    if args.len() > 3 {
        usage("--ui-state takes nothing, or one JSON object");
    }
    let before = store.read();
    let was = shelf_enabled(&before);
    let state = match args.get(2) {
        None => before,
        Some(patch) => {
            let merged = jsondoc::parse(patch)
                .map_err(|e| format!("the ui.json patch is not JSON ({})", e))
                .and_then(|p| store.update(&p));
            match merged {
                Ok(next) => next,
                Err(e) => {
                    eprintln!("flea: {}", e);
                    return 2;
                }
            }
        }
    };
    // B1: the Settings switch is the only thing that installs the shelf plugin, and every front end
    // reaches it through this one path, so the bar follows the switch without a second act.
    let now = shelf_enabled(&state);
    if now != was {
        if let Err(e) = shelfplugin::sync(now) {
            eprintln!("flea: the shelf plugin was not {} ({})", if now { "enabled" } else { "disabled" }, e);
        }
    }
    print!("{}", jsondoc::render(&state));
    0
}

// Directive 38: the shelf ships off, so anything but a stored true is off.
fn shelf_enabled(state: &jsondoc::Json) -> bool {
    state.get("shelf").and_then(|s| s.get("enabled")).and_then(|v| v.as_bool()) == Some(true)
}

fn main() {
    // args() panics on a non-UTF-8 argument and a Linux filename is any bytes, so refuse instead.
    let args: Vec<String> = match std::env::args_os().map(|a| a.into_string()).collect() {
        Ok(v) => v,
        Err(bad) => {
            eprintln!("flea: {} is not valid UTF-8, and Flea takes text paths", bad.to_string_lossy());
            exit(2);
        }
    };

    // Bare and checked before every other mode, so a script can ask which Flea is installed without parsing.
    if args.len() == 2 && args[1] == "--version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
        exit(0);
    }
    // Anywhere in argv, because this mode is read before the others and must refuse rather than win over one.
    if args.iter().any(|a| a == "--version") {
        usage("--version takes nothing");
    }

    if args.iter().any(|a| a == "--backend") {
        exit(backend::run::run());
    }

    // flea --thumb-worker: only ever started by the backend, inside its sandbox, with a socket on stdin.
    if args.len() == 2 && args[1] == "--thumb-worker" {
        exit(backend::thumbworker::run());
    }

    // flea --figure-helper, --figure-compile and --figure-store: the figure engine's launcher modes; see AGENTS.md "Markdown figures".
    if let Some(code) = figurehelper::dispatch(&args) {
        exit(code);
    }

    // flea --launch-warm <list> <gvfs-path> <gvfs-dest>: one fork for both launch jobs, "-" skips one.
    if args.len() == 5 && args[1] == "--launch-warm" {
        exit(gui::run_launch_warm(&args[2], &args[3], &args[4]));
    }
    if args.get(1).map(String::as_str) == Some("--launch-warm") {
        usage("--launch-warm takes a list, a gvfs path and a destination");
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
    if args.get(1).map(String::as_str) == Some("--prewarm") {
        usage("--prewarm takes a path, a first index and a destination");
    }

    // flea --open <path>...
    if args.len() >= 3 && args[1] == "--open" {
        exit(open::open_all(&args[2..]));
    }
    if args.get(1).map(String::as_str) == Some("--open") {
        usage("--open takes one or more paths");
    }

    // flea --terminal <dir>
    if args.len() == 3 && args[1] == "--terminal" {
        exit(terminal::open_terminal(&args[2]));
    }
    if args.get(1).map(String::as_str) == Some("--terminal") {
        usage("--terminal takes one directory");
    }

    // flea --update [check]: ask the installing source for a newer Flea, or open Omarchy's updater; see src/update.rs.
    if args.len() == 2 && args[1] == "--update" {
        exit(update::launch());
    }
    if args.len() == 3 && args[1] == "--update" && args[2] == "check" {
        exit(update::check());
    }
    if args.get(1).map(String::as_str) == Some("--update") {
        usage("--update takes nothing, or check");
    }

    // flea --default [off]: both per-user steps pacman cannot own, see docs/install.md.
    if args.len() == 2 && args[1] == "--default" {
        exit(claim_both());
    }
    if args.len() == 3 && args[1] == "--default" && args[2] == "off" {
        exit(release_both());
    }
    if args.get(1).map(String::as_str) == Some("--default") {
        usage("--default takes nothing, or off");
    }

    // corner: an undocumented alias for --default off, kept out of usage on purpose.
    if args.len() == 2 && args[1] == "--youleftmeforstrata" {
        exit(release_both());
    }
    if args.get(1).map(String::as_str) == Some("--youleftmeforstrata") {
        usage("--youleftmeforstrata takes nothing");
    }

    // flea --picker [off]: the chooser routing, the other per-user step, see docs/install.md.
    if args.len() == 2 && args[1] == "--picker" {
        exit(claim_picker());
    }
    if args.len() == 3 && args[1] == "--picker" && args[2] == "off" {
        exit(chooser::release());
    }
    if args.get(1).map(String::as_str) == Some("--picker") {
        usage("--picker takes nothing, or off");
    }

    // flea --pick <reply>: one portal request's picker window, opened by tools/flea-portal.
    if args.len() == 3 && args[1] == "--pick" {
        exit(gui::pick(&args[2]));
    }
    if args.get(1).map(String::as_str) == Some("--pick") {
        usage("--pick takes one reply file");
    }

    // flea --clip-own: the detached owner behind one clipboard copy, reading its payload on stdin.
    if args.len() == 2 && args[1] == "--clip-own" {
        exit(clip::own::run());
    }
    if args.get(1).map(String::as_str) == Some("--clip-own") {
        usage("--clip-own takes nothing");
    }

    // flea --clip get|set copy|cut|clear TOKEN: the terminal and test seam over the clipboard.
    if args.len() == 3 && args[1] == "--clip" && args[2] == "get" {
        exit(clip::cli::get());
    }
    if args.len() == 4 && args[1] == "--clip" && args[2] == "set" && (args[3] == "copy" || args[3] == "cut") {
        exit(clip::cli::set(&args[3]));
    }
    if args.len() == 4 && args[1] == "--clip" && args[2] == "clear" {
        exit(clip::cli::clear(&args[3]));
    }
    if args.get(1).map(String::as_str) == Some("--clip") {
        usage("--clip takes get, set copy|cut, or clear TOKEN");
    }

    // flea --ui-state [<json patch>]: the shared ui.json read and update path, see AGENTS.md "The state file".
    if args.get(1).map(String::as_str) == Some("--ui-state") {
        exit(ui_state(&args));
    }

    if args.get(1).map(String::as_str) == Some("--favourites") {
        exit(favourites::command(&args));
    }

    // flea shelf <verb>: the drop shelf's own state, minted for a plugin that is another process.
    if args.get(1).map(String::as_str) == Some("shelf") {
        exit(shelfcli::command(&args));
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

    // Explicit terminal mode never writes escape codes into a pipeline.
    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if want_tui {
        if !interactive {
            eprintln!("flea: the terminal interface needs a terminal on stdin and stdout");
            exit(2);
        }
        exit(tui::run(open_path.as_deref(), select_path.as_deref()));
    }

    if !paths::has_display() {
        eprintln!("flea: there is no graphical session to open a window in");
        exit(2);
    }
    match paths::ui_dir() {
        Some(ui) => {
            // Before the window, so the first paint reads what the settle left, see AGENTS.md "The
            // state file"; it can fail or decline, and the window then opens on a file it did not touch.
            match uistore::Store::user().and_then(|store| store.settle()) {
                Ok(()) => {}
                Err(e) => eprintln!("flea: the view state was not settled ({})", e),
            }
            // An upgrade ships a new plugin under a switch that is already on, and the copy in the
            // user's own plugin directory is the one the bar reads.
            if let Ok(store) = uistore::Store::user() {
                if let Err(e) = shelfplugin::refresh(shelf_enabled(&store.read())) {
                    eprintln!("flea: the shelf plugin was not refreshed ({})", e);
                }
            }
            exit(gui::exec_qs(&ui, open_path.as_deref(), select_path.as_deref()))
        }
        None => {
            eprintln!("{}", paths::missing_ui_message());
            exit(2);
        }
    }
}
