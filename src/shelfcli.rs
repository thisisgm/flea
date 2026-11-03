// The shelf's verbs as a command, because the plugin is another process: `flea shelf <verb>` is the
// only way in, and every mutation of the pile is on this side of it.
use crate::captures;
use crate::error::io_message;
use crate::shelf::{now_ms, size_of, Shelf};
use crate::shelfops;
use crate::shelfthumb;
use crate::shelfzip;
use crate::summon;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

pub fn command(args: &[String]) -> i32 {
    match args.get(2).map(String::as_str) {
        Some("drag-begin") => drag_begin(&args[3..]),
        Some("size") => size(&args[3..]),
        Some("forget") => forget(&args[3..]),
        Some("add") => add(&args[3..]),
        Some("pin") => pin(&args[3..], true),
        Some("unpin") => pin(&args[3..], false),
        Some("order") => order(&args[3..]),
        Some("captures") => captures::command(&args[3..]),
        Some("thumb") => shelfthumb::command(&args[3..]),
        Some("open") => open(&args[3..]),
        Some("move") => shelfops::transfer(true, &args[3..]),
        Some("copy") => shelfops::transfer(false, &args[3..]),
        Some("cancel") => shelfops::cancel(),
        Some("zip") => shelfzip::zip(&args[3..]),
        Some("paths") => shelfops::paths(&args[3..]),
        Some("clip") => shelfops::clip(&args[3..]),
        Some("places") => crate::shelfplaces::command(),
        Some("choose") => crate::shelfplaces::choose(&args[3..]),
        Some("send") => shelfops::send(&args[3..]),
        Some("peers") => shelfops::peers(),
        Some("clear") => summon::clear(),
        Some("restore") => summon::restore(&args[3..]),
        Some("undo") => crate::shelfundo::undo(),
        Some("piles") => summon::piles(),
        Some("toggle") => summon::toggle(),
        Some("bind") => summon::bind(),
        _ => {
            eprintln!("flea: shelf takes drag-begin, size, add, open, move, copy, pin, unpin, order, zip, send, peers, paths, clip, places, choose, cancel, forget, captures, thumb, clear, restore, undo, piles, toggle or bind");
            2
        }
    }
}

// SettingsRest rule 4: `flea shelf order <path> <index>` puts a pin at that place among the pins.
fn order(rest: &[String]) -> i32 {
    let (path, to) = match (rest.first(), rest.get(1).and_then(|n| n.parse::<usize>().ok())) {
        (Some(path), Some(to)) => (path, to),
        _ => {
            eprintln!("flea: shelf order takes a path and a position");
            return 2;
        }
    };
    let shelf = match Shelf::user() {
        Ok(shelf) => shelf,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    match shelf.order(path, to) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("flea: {}", e);
            2
        }
    }
}

fn size(rest: &[String]) -> i32 {
    let path = match rest.first() {
        Some(path) => path,
        None => {
            eprintln!("flea: shelf size takes one path");
            return 2;
        }
    };
    match size_of(path) {
        Ok((bytes, partial)) => {
            println!("{} {}", bytes, u8::from(partial));
            0
        }
        Err(e) => {
            eprintln!("flea: {}", e);
            2
        }
    }
}

fn add(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("flea: shelf add takes at least one path");
        return 2;
    }
    let shelf = match Shelf::user() {
        Ok(shelf) => shelf,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    match shelf.add(rest) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("flea: {}", e);
            2
        }
    }
}

// Keys: enter opens the file with its own handler, or reveals the folder in Flea. A folder does not
// go to gio, which would hand it straight back to Flea's own desktop entry and lose the path.
fn open(rest: &[String]) -> i32 {
    let path = match rest.first() {
        Some(path) => path,
        None => {
            eprintln!("flea: shelf open takes one path");
            return 2;
        }
    };
    let code = crate::open::open(path);
    if code != crate::open::IS_DIRECTORY {
        return code;
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            eprintln!("flea: {} could not be shown: {}", path, io_message(&e));
            return 2;
        }
    };
    // Detached, because the card's own process is not the window's parent: it asked for it and goes.
    match Command::new(exe)
        .arg("--gui")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
    {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("flea: {} could not be shown: {}", path, io_message(&e));
            2
        }
    }
}

// Main rule 10: `p` on a row, and Flea's own menu row, both land here.
fn pin(rest: &[String], pinned: bool) -> i32 {
    if rest.is_empty() {
        eprintln!("flea: shelf pin and unpin take at least one path");
        return 2;
    }
    let shelf = match Shelf::user() {
        Ok(shelf) => shelf,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    match shelf.pin(rest, pinned) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("flea: {}", e);
            2
        }
    }
}

fn forget(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("flea: shelf forget takes at least one path");
        return 2;
    }
    let shelf = match Shelf::user() {
        Ok(shelf) => shelf,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    // Main rule 6: the row's own x is the same edit a completed move makes, and it never touches the file.
    match shelf.settle(rest) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("flea: {}", e);
            2
        }
    }
}

fn drag_begin(rest: &[String]) -> i32 {
    let moving = match rest.first().map(String::as_str) {
        Some("move") => true,
        Some("copy") => false,
        _ => {
            eprintln!("flea: shelf drag-begin takes move or copy, then the entries");
            return 2;
        }
    };
    let paths: Vec<String> = rest[1..].to_vec();
    let shelf = match Shelf::user() {
        Ok(shelf) => shelf,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    match shelf.drag_begin(moving, &paths, now_ms()) {
        Ok(token) => {
            println!("{}", token);
            0
        }
        Err(e) => {
            eprintln!("flea: {}", e);
            2
        }
    }
}

