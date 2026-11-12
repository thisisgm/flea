//! A real backend process lists a marked 100k folder: its first answer is one viewport window, and the pane asks nothing more.
#[path = "m9_g1rows_02/helpers.rs"]
mod helpers;
#[path = "m9_g1rows_02/prior.rs"]
mod prior;
#[path = "m9_g1rows_02/probe.rs"]
mod probe;
#[path = "m9_g1rows_02/process.rs"]
mod process;
#[path = "m9_g1rows_02/startup.rs"]
mod startup;
#[path = "m9_g1rows_02/timing.rs"]
mod timing;

use flea_gui::backend::{list_line, Backend, Event};
use flea_gui::list::pane::Pane;
use flea_gui::navigation::folder_state::{Folders, Writer};
use flea_gui::navigation::{Listing, Navigator};
use process::{binary, fixture, marked_root, sandboxed, DEADLINE, SETTLE};
use startup::startup;
use std::cell::RefCell;
use std::fs::{self, File};
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::time::{Duration, Instant};

const ENTRIES: usize = 100_000;
const SMALL: usize = 2_000;
// The pane height the HIG list board draws at 1100x700, so the window is one real viewport.
const HEIGHT: f32 = 700.;
// An offscreen window far past the first one, used to prove the stat counter sees the backend's own stats.
const FAR_START: usize = 50_000;
const FAR_COUNT: usize = 10;

// The listing the GUI's own navigator builds for a fresh install's first pane at the board's pane height.
fn first_listing(pane: &Pane, dir: &Path) -> Listing {
    let folders = Folders::from_settings(&flea::jsondoc::Json::Obj(Vec::new()), Vec::new());
    let mut navigator = Navigator::new(
        Rc::new(RefCell::new(folders)),
        Rc::new(RefCell::new(Writer::default())),
    );
    navigator.listing(pane, &dir.to_string_lossy(), false)
}

#[test]
fn m9_g1rows_02_startup_work_is_viewport_bounded() {
    let root = marked_root("bounded");
    let (large_dir, small_dir) = (fixture(&root, ENTRIES), fixture(&root, SMALL));
    let large_pane = Pane::new(large_dir.to_string_lossy().into_owned(), HEIGHT);
    let first = large_pane.first_count();
    assert!(
        first < SMALL,
        "one window is smaller than the small fixture"
    );
    let large = startup(&root, &large_dir, &first_listing(&large_pane, &large_dir));
    let small = startup(
        &root,
        &small_dir,
        &first_listing(
            &Pane::new(small_dir.to_string_lossy().into_owned(), HEIGHT),
            &small_dir,
        ),
    );
    for (label, found, entries) in [("100k", &large, ENTRIES), ("2k", &small, SMALL)] {
        println!(
            "M9_G1ROWS_02 fixture={label} entries={entries} total={} first={first} start={} rows={} stats={} spawns={} named_opens={} children={} late_rows={} threads={} cpu_ticks={} rows_ms={} listed={}",
            found.total, found.start, found.rows, found.stats, found.spawns, found.named_opens, found.children, found.late_rows, found.threads, found.cpu_ticks, found.rows_ms, found.listed
        );
        assert_eq!(
            found.total, entries,
            "{label}: listed carries the integer count"
        );
        assert_eq!(
            (found.start, found.rows),
            (0, first),
            "{label}: the first answer is exactly one window"
        );
        assert!(
            found.stats <= found.rows,
            "{label}: at most one stat per drawn row, none offscreen"
        );
        found.assert_helpers(label);
        assert_eq!(
            found.named_opens, 0,
            "{label}: no entry is opened or read for MIME or a thumbnail at startup"
        );

        assert_eq!(
            found.late_rows, 0,
            "{label}: no further rows are pushed unasked"
        );
    }
    assert_eq!(
        (large.rows, large.stats),
        (small.rows, small.stats),
        "the first window and its stats do not grow with the folder"
    );
}

// A shell records every line the backend is sent, then hands stdin on unchanged.
fn recorded_command(root: &Path, log: &Path) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg("tee -a \"$0\" | \"$1\" --backend")
        .arg(log)
        .arg(binary());
    sandboxed(&mut command, root);
    command
}

fn wait_for(label: &str, deadline: Instant, mut done: impl FnMut() -> bool) {
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn m9_g1rows_02_first_rows_need_no_second_request() {
    let root = marked_root("second");
    let dir = fixture(&root, ENTRIES);
    let path = dir.to_string_lossy().into_owned();
    let mut pane = Pane::new(path.clone(), HEIGHT);
    let first = pane.first_count();
    assert_eq!(
        pane.list_requests, 1,
        "the pane starts with its one listing"
    );
    let listing = first_listing(&pane, &dir);
    assert_eq!(
        listing.first, first,
        "the navigator asks for the pane's first window"
    );
    let log = root.join("requests.log");
    File::create(&log).unwrap();
    let backend = Backend::start_with(recorded_command(&root, &log), list_line(&listing)).unwrap();
    let deadline = Instant::now() + DEADLINE;
    let mut first_at = None;
    let mut seen = Vec::new();
    wait_for(
        "the first window and settled frames",
        deadline + SETTLE,
        || {
            while let Ok(event) = backend.replies.try_recv() {
                seen.push(format!("{event:?}").chars().take(160).collect::<String>());
                match event {
                    Event::Reply(reply) => {
                        let now = pane.now();
                        pane.receive_at(reply, now).unwrap();
                    }
                    Event::Failed(failure) => {
                        panic!("the backend failed: {} {}", failure.key, failure.detail)
                    }
                    _ => {}
                }
            }
            pane.tick(pane.now());
            // The window host's own step: whatever the pane wants goes down the same pipe, so a second request would reach the log.
            if let Some((start, count)) = pane.wanted_window() {
                backend.window(start, count).unwrap();
            }
            let landed = pane.swap.pending.is_none() && !pane.held.rows.is_empty();
            if Instant::now() + Duration::from_millis(10) >= deadline {
                eprintln!(
                    "M9_G1ROWS_02 events={seen:?} pending={:?}",
                    pane.swap
                        .pending
                        .as_ref()
                        .map(|pending| (pending.total, pending.ready.is_some()))
                );
            }
            landed && first_at.get_or_insert_with(Instant::now).elapsed() >= SETTLE
        },
    );
    assert!(
        !pane.swap.fell_back,
        "the first window landed without a fallback"
    );
    assert_eq!(
        pane.held.total, ENTRIES,
        "the held listing carries the integer count"
    );
    assert_eq!(
        (pane.held.start, pane.held.rows.len()),
        (0, first),
        "one window landed"
    );
    assert_eq!(
        pane.wanted_window(),
        None,
        "the landed first window covers the viewport"
    );
    assert_eq!(
        pane.requests.sent, 0,
        "no window request followed the listing"
    );
    assert_eq!(pane.list_requests, 1, "no second listing");
    let requests: Vec<String> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    println!(
        "M9_G1ROWS_02 first={first} total={} held_rows={} window_requests={} backend_lines={}",
        pane.held.total,
        pane.held.rows.len(),
        pane.requests.sent,
        requests.len()
    );
    assert_eq!(
        requests,
        vec![list_line(&listing)],
        "the backend received the one list request"
    );
    // Negative control: a viewport past the held window asks, counts and reaches the log, so the zeros above are measurements.
    pane.scroll_to(FAR_START as f32 * pane.row_height());
    let (start, count) = pane
        .wanted_window()
        .expect("a scroll past the first window asks for rows");
    backend.window(start, count).unwrap();
    assert_eq!(
        pane.requests.sent, 1,
        "the pane counts the window it asked for"
    );
    let logged = Instant::now() + DEADLINE;
    wait_for("the far window in the log", logged, || {
        fs::read_to_string(&log).unwrap().lines().count() == 2
    });
    backend.quit();
    let drained = backend.drained();
    wait_for("the drained backend", Instant::now() + DEADLINE, || {
        drained.try_recv().is_ok()
    });
    drop(backend);
}
