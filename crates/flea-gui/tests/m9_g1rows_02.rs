//! A real backend process lists a marked 100k folder: its first answer is one viewport window, and the pane asks nothing more.
#[path = "m9_g1rows_02/probe.rs"]
mod probe;
#[path = "m9_g1rows_02/process.rs"]
mod process;

use flea::protocol::{parse_reply, Reply};
use flea_gui::backend::{list_line, Backend, Event};
use flea_gui::list::pane::Pane;
use flea_gui::navigation::folder_state::{Folders, Writer};
use flea_gui::navigation::{Listing, Navigator};
use probe::{probe, probed};
use process::{
    binary, children, cpu_ticks, fixture, marked_root, sandboxed, threads, Observer, Process,
    DEADLINE, SETTLE,
};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
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

struct Startup {
    total: usize,
    start: usize,
    rows: usize,
    stats: usize,
    spawns: usize,
    named_opens: usize,
    children: usize,
    late_rows: usize,
    threads: usize,
    cpu_ticks: u64,
    listed: String,
    rows_ms: String,
}

// Sample input: {"t":"rows","start":0,"listing":2,"ms":0.389,"rows":[...]}
fn rows_ms(line: &str) -> String {
    match flea::jsondoc::parse(line)
        .ok()
        .and_then(|document| document.get("ms").cloned())
    {
        Some(flea::jsondoc::Json::Num(value)) => value,
        _ => "absent".into(),
    }
}

fn startup(root: &Path, dir: &Path, listing: &Listing) -> Startup {
    let log = root.join(format!("probe-{}.log", listing.first));
    fs::write(&log, b"").unwrap();
    let mut observer = Observer::watch(dir);
    assert_eq!(
        observer.named(),
        0,
        "the fixture is quiet before the backend starts"
    );
    let mut process = Process::start(root, dir, &log);
    let pid = process.child.id();
    process.send(&list_line(listing));
    let listed = process.next();
    let Ok(Reply::Listed { path, total }) = parse_reply(&listed) else {
        panic!("the first answer is listed: {listed}")
    };
    assert_eq!(path, dir.to_string_lossy());
    let rows_line = process.next();
    let Ok(Reply::Rows { start, rows, .. }) = parse_reply(&rows_line) else {
        panic!("the second answer is rows: {rows_line}")
    };
    let visible: BTreeSet<_> = rows.iter().map(|row| dir.join(&row.name)).collect();
    let late = process.late();
    let late_rows = late
        .iter()
        .filter(|line| matches!(parse_reply(line), Ok(Reply::Rows { .. })))
        .count();
    let found = Startup {
        total,
        start,
        rows: rows.len(),
        stats: probed(&log, b's'),
        spawns: probed(&log, b'p'),
        named_opens: observer.named(),
        children: children(pid),
        late_rows,
        threads: threads(pid),
        cpu_ticks: cpu_ticks(pid),
        listed: listed.clone(),
        rows_ms: rows_ms(&rows_line),
    };
    let targets = probe::stat_targets(dir, &log);
    assert_eq!(
        targets.len(),
        found.stats,
        "every stat has an identity record"
    );
    assert_eq!(
        probe::offscreen(&targets, &visible),
        0,
        "every stat targets a returned row"
    );
    println!("M9_G1ROWS_02 stat_identities={} offscreen=0", targets.len());
    // Negative control: an offscreen window adds its own stats, so the probe counts the backend's real stat path.
    process.send(&format!(
        "{{\"c\":\"window\",\"start\":{},\"count\":{FAR_COUNT}}}",
        FAR_START.min(total.saturating_sub(FAR_COUNT))
    ));
    let far = process.next();
    assert!(
        matches!(parse_reply(&far), Ok(Reply::Rows { ref rows, .. }) if rows.len() == FAR_COUNT),
        "the far window answers: {far}"
    );
    assert!(
        probed(&log, b's') >= found.stats + FAR_COUNT,
        "the probe sees the backend stat the far window"
    );
    let all = probe::stat_targets(dir, &log);
    let mut mixed = targets[..targets.len() - FAR_COUNT].to_vec();
    mixed.extend_from_slice(&all[targets.len()..targets.len() + FAR_COUNT]);
    assert_eq!(
        mixed.len(),
        found.stats,
        "the aggregate count cannot detect this control"
    );
    assert!(
        probe::offscreen(&mixed, &visible) > 0,
        "same-count offscreen identities are rejected"
    );
    process.close();
    let dropped_pid = {
        let abandoned = Process::start(root, dir, &log);
        abandoned.child.id()
    };
    assert!(
        !Path::new(&format!("/proc/{dropped_pid}")).exists(),
        "Drop kills and reaps an unfinished backend"
    );
    // Negative control: a subshell needs a real fork (dash runs simple commands through vfork), so a zero spawn count above is a measurement.
    let mut shell = Command::new("/bin/sh");
    shell.arg("-c").arg("(/bin/true); /bin/true");
    sandboxed(&mut shell, root);
    let before = probed(&log, b'p');
    assert!(shell
        .env("LD_PRELOAD", probe())
        .env("FLEA_PROBE_LOG", &log)
        .status()
        .unwrap()
        .success());
    assert!(probed(&log, b'p') > before, "the probe counts a libc spawn");
    // Negative control: the observer does see a named read, so its zero above is a measurement.
    let mut probe = File::open(
        fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .next()
            .unwrap()
            .path(),
    )
    .unwrap();
    probe.read_to_end(&mut Vec::new()).unwrap();
    assert!(
        observer.named() > 0,
        "the inotify observer counts a named open"
    );
    found
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
        assert_eq!(
            found.spawns, 0,
            "{label}: no thumbnail or helper process is spawned"
        );
        assert_eq!(
            found.named_opens, 0,
            "{label}: no entry is opened or read for MIME or a thumbnail at startup"
        );
        assert_eq!(
            found.children, 0,
            "{label}: no thumbnail or helper process starts"
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
