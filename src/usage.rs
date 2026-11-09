use std::process::exit;

// Every invocation, as a usage error prints it to stderr and --help prints it to stdout.
const LINES: &[&str] = &[
    "usage: flea [--tui|--gui] [--select <uri|path>] [path]",
    "       flea --tui [--cwd-file PATH] [--chooser-file PATH] [--tui-keymap PRESET] [path]",
    "       flea --default [off]",
    "       flea --picker [off]",
    "       flea --ui-state [<json patch>]",
    "       flea --update [check]",
    "       flea --run <path> <dev> <ino> <size> <mtime_s> <mtime_ns>",
    "       flea --run-terminal <path> <dev> <ino> <size> <mtime_s> <mtime_ns>",
    "       flea --open-text <path>",
    "       flea --open-handler <path> <desktop id> <dev> <ino> <size> <mtime_s> <mtime_ns>",
    "       flea --clip get|set copy|cut|clear TOKEN",
    "       flea flash choose [start]",
    "       flea plugin list [--json] | add <git url|folder> | enable|disable|update|remove <id> | validate <folder>",
    "       flea --action <id> [--window <pid>] | --actions [--window <pid>]",
    "       flea ipc [--pid N|--any] call <target> <function> [args...] | show",
    "       flea --version | -V",
    "       flea --help | -h",
];

pub(crate) fn usage(message: &str) -> ! {
    eprintln!("flea: {}", message);
    for line in LINES {
        eprintln!("{line}");
    }
    exit(2)
}

// Asked for, so it goes to stdout and exits 0.
pub(crate) fn help() -> ! {
    for line in LINES {
        println!("{line}");
    }
    exit(0)
}
