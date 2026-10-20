use super::*;
use crate::tui::chafa::test_support::{sandboxed, Fixture};
use crate::tui::chafa::Cell;
use std::path::Path;
use std::time::Instant;

const SETTLE: Duration = Duration::from_secs(15);
// A decoder that reads the held input and draws one coloured upper half block.
const ONE_CELL: &str = "read -r line < /input\n[ \"$line\" = held ]\nprintf '\\033[38;2;1;2;3m\\033[48;2;4;5;6m\\342\\226\\200\\033[0m\\n'";

fn key(path: &Path, generation: usize) -> Key {
    Key { path: path.into(), generation, page: (0, 0, 0), cells: (1, 1), colors: ColorMode::Truecolor }
}

fn still(fixture: &Fixture) -> Still {
    let mut still = Still::default();
    let spool = fixture.path("spool");
    if !spool.exists() {
        std::fs::create_dir(&spool).unwrap();
    }
    still.places = Places { search: Some(fixture.path("bin").into_os_string()), spool: Some(spool) };
    still
}

fn settle(still: &mut Still) {
    let started = Instant::now();
    while still.running() {
        still.poll();
        assert!(started.elapsed() < SETTLE, "the half-block decode did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

// Shells running the stub script, by their command line.
fn shells(marker: &Path) -> Vec<i32> {
    use std::os::unix::ffi::OsStrExt;
    std::fs::read_dir("/proc").unwrap().flatten().filter_map(|entry| {
        let pid = entry.file_name().to_str()?.parse::<i32>().ok()?;
        let cmdline = std::fs::read(entry.path().join("cmdline")).ok()?;
        cmdline.split(|b| *b == 0).any(|arg| arg == marker.as_os_str().as_bytes()).then_some(pid)
    }).collect()
}

#[test]
fn colours_follow_the_depth_and_sixteen_colours_draw_no_picture() {
    assert_eq!(colors(Depth::Truecolor), Some(ColorMode::Truecolor));
    assert_eq!(colors(Depth::Indexed), Some(ColorMode::Indexed));
    assert_eq!(colors(Depth::Sixteen), None);
}

#[test]
fn a_row_carries_only_cell_colours_and_pads_with_the_ground() {
    let theme = Theme::from_text("background='#1c120d'\nforeground='#c6b395'");
    let grid = Grid {
        columns: 2,
        rows: 1,
        cells: vec![
            Cell { symbol: '▀', foreground: Color::Rgb(1, 2, 3), background: Color::Indexed(4) },
            Cell { symbol: ' ', foreground: Color::Default, background: Color::Default },
        ],
    };
    let base = theme.base();
    assert_eq!(row(&grid, 0, 4, &theme), format!("{base}\x1b[38;2;1;2;3m\x1b[48;5;4m▀{}{} {base}  ", theme.foreground, theme.background));
    assert_eq!(row(&grid, 0, 1, &theme), format!("{base}\x1b[38;2;1;2;3m\x1b[48;5;4m▀{base}"));
    assert_eq!(row(&grid, 1, 3, &theme), format!("{base}{base}   "));
}

#[test]
fn a_missing_chafa_draws_nothing_and_says_nothing() {
    let fixture = Fixture::new();
    let image = fixture.file("image.png", b"held\n");
    let mut still = still(&fixture);
    still.want(key(&image, 1), || Some(Source::File(image.clone())));
    settle(&mut still);
    assert!(still.grid(&key(&image, 1)).is_none());
    assert_eq!(still.error, "");
}

#[test]
fn a_picture_is_drawn_only_for_the_key_it_was_asked_for() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    fixture.stub(ONE_CELL);
    let image = fixture.file("image.png", b"held\n");
    let mut still = still(&fixture);
    still.want(key(&image, 1), || Some(Source::File(image.clone())));
    settle(&mut still);
    let cell = Cell { symbol: '▀', foreground: Color::Rgb(1, 2, 3), background: Color::Rgb(4, 5, 6) };
    assert_eq!(still.grid(&key(&image, 1)).map(|grid| grid.cells.clone()), Some(vec![cell]), "{}", still.error);
    assert!(still.grid(&key(&image, 2)).is_none(), "a stale picture was offered for the next preview");
    // Asking again for the same key keeps the picture and starts nothing.
    still.want(key(&image, 1), || panic!("the held picture was decoded twice"));
    assert!(!still.running());
    still.want(key(&image, 2), || None);
    assert!(still.grid(&key(&image, 1)).is_none() && still.key().is_none());
    // A decoder's PNG reaches chafa through the same held-descriptor sandbox.
    still.want(key(&image, 3), || Some(Source::Bytes(b"held\n".to_vec())));
    settle(&mut still);
    assert!(still.grid(&key(&image, 3)).is_some(), "{}", still.error);
    assert_eq!(std::fs::read_dir(fixture.path("spool")).unwrap().count(), 0, "a spooled page outlived its decode");
}

#[test]
fn malformed_or_failed_decoder_output_is_an_error_and_draws_nothing() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    let image = fixture.file("image.png", b"held\n");
    for stub in ["printf '\\033]52;c;aGk=\\007'", "printf '\\342\\226\\200\\n'; exit 3"] {
        fixture.stub(stub);
        let mut still = still(&fixture);
        still.want(key(&image, 1), || Some(Source::File(image.clone())));
        settle(&mut still);
        assert!(still.grid(&key(&image, 1)).is_none(), "{stub}");
        assert!(!still.error.is_empty(), "{stub}");
    }
}

#[test]
fn a_swapped_file_is_refused_before_chafa_sees_it() {
    let fixture = Fixture::new();
    fixture.file("image.png", b"held\n");
    let link = fixture.path("link.png");
    std::os::unix::fs::symlink(fixture.path("image.png"), &link).unwrap();
    let mut still = still(&fixture);
    still.want(key(&link, 1), || Some(Source::File(link.clone())));
    settle(&mut still);
    assert!(still.grid(&key(&link, 1)).is_none());
    assert!(!still.error.is_empty(), "a link reached the decoder");
}

#[test]
fn a_cancelled_decode_kills_and_reaps_chafa_before_its_worker_returns() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    fixture.stub("trap '' TERM\nwhile :; do /usr/bin/sleep 60; done");
    let image = fixture.file("image.png", b"held\n");
    let marker = fixture.path("bin/chafa");
    let cancel = Arc::new(AtomicBool::new(false));
    let stopped = cancel.clone();
    let places = Places { search: Some(fixture.path("bin").into_os_string()), spool: None };
    let worker = std::thread::spawn(move || decode(Source::File(image), (2, 2), ColorMode::Truecolor, places, &stopped));
    let started = Instant::now();
    while shells(&marker).is_empty() {
        assert!(started.elapsed() < SETTLE, "chafa did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    cancel.store(true, Ordering::Relaxed);
    let outcome = worker.join().unwrap();
    assert_eq!(outcome.err().map(|error| error.to_string()).as_deref(), Some("Preview cancelled"));
    assert_eq!(shells(&marker), Vec::<i32>::new(), "chafa outlived the worker that cancelled it");
}

#[test]
fn a_poster_job_whose_worker_is_gone_reports_an_error_and_stops_waiting() {
    let mut still = Still::default();
    still.stage = Some(crate::tui::job::Job::orphaned());
    assert!(!still.poll());
    assert!(still.stage.is_none(), "still waiting on a dead job");
    assert_eq!(still.error, UNSHOWN);
}
