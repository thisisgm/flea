use std::process::exit;

pub(crate) fn usage(message: &str) -> ! {
    eprintln!("flea: {}", message);
    eprintln!("usage: flea [--tui|--gui] [--select <uri|path>] [path]");
    eprintln!(
        "       flea --tui [--cwd-file PATH] [--chooser-file PATH] [--tui-keymap PRESET] [path]"
    );
    eprintln!("       flea --default [off]");
    eprintln!("       flea --picker [off]");
    eprintln!("       flea --ui-state [<json patch>]");
    eprintln!("       flea --update [check]");
    eprintln!("       flea --run <path> <dev> <ino> <size> <mtime_s> <mtime_ns>");
    eprintln!("       flea --run-terminal <path> <dev> <ino> <size> <mtime_s> <mtime_ns>");
    eprintln!("       flea --open-text <path>");
    eprintln!(
        "       flea --open-handler <path> <desktop id> <dev> <ino> <size> <mtime_s> <mtime_ns>"
    );
    eprintln!("       flea --clip get|set copy|cut|clear TOKEN");
    eprintln!("       flea flash choose [start]");
    eprintln!("       flea plugin list [--json] | add <git url|folder> | enable|disable|update|remove <id> | validate <folder>");
    eprintln!("       flea --action <id> [--window <pid>] | --actions [--window <pid>]");
    eprintln!("       flea ipc [--pid N|--any] call <target> <function> [args...] | show");
    eprintln!("       flea --version");
    exit(2)
}
