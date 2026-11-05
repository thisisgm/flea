mod backend;
mod chooser;
mod claims;
mod clip;
mod defaults;
mod desktopid;
use flea::error;
mod gui;
mod heap;
mod hyprkeys;
// Tests swap in a catalog module that also carries the fixture catalogs.
#[cfg(not(test))]
use flea::i18n;
#[cfg(test)]
mod i18n {
    pub use flea::i18n::{I18n, Locale, Paths};
    #[path = "test_catalogs.rs"]
    pub(crate) mod test_catalogs;
}
mod icdexclude;
mod imageopen;
mod json;
mod jsondoc;
mod jsonmembers;
mod jsonstring;
mod launcher;
mod launchargs;
mod oflags;
mod open;
mod paths;
mod prefetch;
mod procsignal;
#[cfg(test)]
mod qsregistry;
mod tearoff;
mod runfile;
mod runid;
mod terminal;
mod tui;
mod thp;
mod unlock;
mod uischema;
mod uimigrate;
mod uistate;
mod uistatecli;
mod uiplugins;
mod plugincli;
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
mod persistdoc;
mod cachewrite;
mod update;
mod userfile;
mod usage;
mod vulkan;

pub(crate) use usage::usage;
use crate::backend::proto::error_line;
use claims::{claim_both, claim_picker, release_both};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::exit;

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

// Only a flash picker invocation takes the verb: a bare flash is a path like any other, so a folder named flash still opens.
pub(crate) fn flash_picker(args: &[String]) -> bool {
    args.get(1).map(String::as_str) == Some("flash") && args.get(2).map(String::as_str) == Some("choose")
}

#[cfg(test)]
#[path = "copysrc_tests.rs"]
mod copysrc_tests;

mod stl_thumbnail;
/// Dispatch internal worker modes and ordinary Flea commands.
fn main() {
    if std::env::args_os().nth(1).is_some_and(|arg| arg == "--stl-thumbnail-worker") {
        if let Err(error) = stl_thumbnail::run(std::env::args_os().skip(2)) {
            eprintln!("flea STL thumbnail: {error}");
            std::process::exit(1);
        }
        return;
    }
    // args() panics on a non-UTF-8 argument and a Linux filename is any bytes, so refuse instead.
    let args: Vec<String> = match std::env::args_os().map(|a| a.into_string()).collect() {
        Ok(v) => v,
        Err(bad) => {
            eprintln!("flea: {} is not valid UTF-8, and Flea takes text paths", bad.to_string_lossy());
            exit(2);
        }
    };
    if args.get(1).map(String::as_str) == Some("--plugin-host") {
        exit(backend::pluginhost::run());
    }

    // IPC owns its arguments, including strings that spell another mode flag.
    if args.get(1).map(String::as_str) == Some("ipc") {
        exit(flea::ipc::client::run(&args[2..]));
    }

    // flea --action <id> and flea --actions: one request to a running window's IPC socket, never a new window.
    if matches!(args.get(1).map(String::as_str), Some("--action" | "--actions")) {
        exit(flea::ipc::action::run(&args[1..]));
    }

    // A bare version query needs no display or path parsing.
    if args.len() == 2 && args[1] == "--version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
        exit(0);
    }
    // Anywhere in argv, because this mode is read before the others and must refuse rather than win over one.
    if args.iter().any(|a| a == "--version") {
        usage("--version takes nothing");
    }

    if args.iter().any(|a| a == "--backend") {
        let code = backend::run::run();
        backend::pluginruntime::shutdown();
        exit(code);
    }

    if args.get(1).map(String::as_str) == Some("--preview-raster") {
        exit(backend::previewraster::run(&args[2..]));
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

    // flea --open <path> [images...]: the window's entrance; the first target still asks the run card.
    if args.len() >= 3 && args[1] == "--open" {
        exit(open::open_many(&args[2..]));
    }
    if args.get(1).map(String::as_str) == Some("--open") {
        usage("--open <path> [images...]");
    }

    // flea --terminal <dir>
    if args.len() == 3 && args[1] == "--terminal" {
        exit(terminal::open_terminal(&args[2]));
    }
    if args.get(1).map(String::as_str) == Some("--terminal") {
        usage("--terminal takes one directory");
    }

    // flea --unlock <device>: the encrypted-drive dialog, see src/unlock.rs.
    if let Some(code) = unlock::dispatch(&args) {
        exit(code);
    }

    // flea --run, --run-terminal, --open-text and --open-handler: the run card's own launches; see src/runfile.rs.
    if let Some(code) = runfile::dispatch(&args) {
        exit(code);
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
        exit(uistatecli::ui_state(&args));
    }

    // Verbs that parse their own arguments: shelf <verb> serves the shelf plugin, flash choose [start] the flasher's picker.
    match args.get(1).map(String::as_str) {
        Some("--favourites") => exit(favourites::command(&args)),
        Some("shelf") => exit(shelfcli::command(&args)),
        Some("plugin") if plugincli::is_command(&args) => exit(plugincli::command(&args)),
        Some("flash") if flash_picker(&args) => exit(shelfplaces::flash_command(&args)),
        _ => {}
    }
    let launchargs::Options { want_tui, print_target, tui_options, select_raw, start } =
        launchargs::Options::parse(&args).unwrap_or_else(|error| usage(&error));
    // A relative start or reveal is resolved here, because the window has no launch directory to read it against.
    let start = start.map(|s| paths::commandline_path(&s));
    let select_raw = select_raw.map(|s| paths::commandline_path(&s));

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
        exit(tui::run(open_path.as_deref(), select_path.as_deref(), &tui_options));
    }

    if gui::reject_legacy_frontend() {
        exit(2);
    }
    if !paths::has_display() {
        eprintln!("flea: there is no graphical session to open a window in");
        exit(2);
    }

    exit(gui::exec_gpui(open_path.as_deref(), select_path.as_deref()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_named_flash_still_opens() {
        let bare = ["flea".to_string(), "flash".to_string()];
        assert!(!flash_picker(&bare), "a bare flash is a path, not the picker");
        let picker = ["flea".to_string(), "flash".to_string(), "choose".to_string()];
        assert!(flash_picker(&picker), "flash choose runs the picker");
    }
}
