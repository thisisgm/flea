use super::test_support::{sandboxed, Fixture};
use super::*;
use std::path::Path;
use std::time::{Duration, Instant};

const START_TIMEOUT: Duration = Duration::from_secs(5);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(4);
const OVERFLOW_TIMEOUT: Duration = Duration::from_secs(8);
const PROCESS_POLL: Duration = Duration::from_millis(10);
const SIGUSR1: i32 = 10;
// A decoder that ignores SIGTERM and starts a descendant that does too, so only a kill ends them.
const TERM_IGNORING_DECODER: &str = "trap '' TERM\n/bin/sh -c 'trap \"\" TERM; while :; do /usr/bin/sleep 60; done' \"$0.descendant\" &\n";

#[test]
fn rejects_decoder_failure_even_after_valid_output() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    fixture.stub("printf '▀'; exit 3");
    assert!(fixture
        .start(2, 2, ColorMode::Truecolor)
        .result
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .is_err());
}

#[test]
fn output_cap_kills_decoder_before_it_can_hang() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    // USR1 releases output only after the decoder and its descendant are both observable.
    fixture.stub(&format!(
        "trap '/usr/bin/head -c {} /dev/zero' USR1\n{TERM_IGNORING_DECODER}wait || :\nwait",
        OUTPUT_LIMIT + 1,
    ));
    let input = File::open(fixture.file("image", b"held\n")).unwrap();
    let executable = fixture.path("bin/chafa");
    let cancel = Arc::new(AtomicBool::new(false));
    let stopped = cancel.clone();
    let (tx, result) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let started = Instant::now();
    let worker = std::thread::spawn(move || {
        let _ = tx.send(process::run(
            input,
            &executable,
            (2, 2),
            ColorMode::Truecolor,
            &stopped,
        ));
        // Holding the spawning thread keeps bwrap's --die-with-parent from hiding a missing kill.
        let _ = held.recv();
    });
    let mut job = Job {
        result,
        cancel,
        worker: Some(worker),
    };
    let processes = wait_for_decoder(&fixture);
    crate::procsignal::signal_process(processes[0], SIGUSR1).unwrap();
    assert!(job.result.recv_timeout(OVERFLOW_TIMEOUT).unwrap().is_err());
    assert_processes_gone(&fixture, processes, started, OVERFLOW_TIMEOUT);
    release.send(()).unwrap();
    job.cancel();
    assert!(job.worker.is_none());
    assert!(started.elapsed() < OVERFLOW_TIMEOUT);
}

#[test]
fn hanging_decoder_hits_existing_ten_second_deadline_and_joins() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    fixture.stub("/usr/bin/sleep 60");
    let started = Instant::now();
    let mut job = fixture.start(2, 2, ColorMode::Truecolor);
    assert!(job
        .result
        .recv_timeout(Duration::from_secs(15))
        .unwrap()
        .is_err());
    job.cancel();
    assert!(job.worker.is_none());
    assert!(
        started.elapsed() >= Duration::from_secs(10) && started.elapsed() < Duration::from_secs(15)
    );
}

// Sample input: b"/bin/sh\0/path/to/chafa\0--format=symbols\0".
fn shells_named(marker: &Path) -> Vec<i32> {
    use std::os::unix::ffi::OsStrExt;
    std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse::<i32>().ok()?;
            let cmdline = std::fs::read(entry.path().join("cmdline")).ok()?;
            let args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
            (args.first() == Some(&b"/bin/sh".as_slice())
                && args.contains(&marker.as_os_str().as_bytes()))
            .then_some(pid)
        })
        .collect()
}

fn wait_for_decoder(fixture: &Fixture) -> [i32; 2] {
    let started = Instant::now();
    let decoder = fixture.path("bin/chafa");
    let descendant = fixture.path("bin/chafa.descendant");
    loop {
        if let (Some(decoder), Some(descendant)) = (
            shells_named(&decoder).first(),
            shells_named(&descendant).first(),
        ) {
            return [*decoder, *descendant];
        }
        assert!(
            started.elapsed() < START_TIMEOUT,
            "decoder and descendant did not both start"
        );
        std::thread::sleep(PROCESS_POLL);
    }
}

// Polls until the observed pids and every shell still naming either marker are gone, failing at the bound.
fn assert_processes_gone(
    fixture: &Fixture,
    processes: [i32; 2],
    started: Instant,
    timeout: Duration,
) {
    let markers = [
        fixture.path("bin/chafa"),
        fixture.path("bin/chafa.descendant"),
    ];
    loop {
        let alive: Vec<i32> = processes
            .into_iter()
            .filter(|pid| crate::procsignal::signal_process(*pid, 0).is_ok())
            .chain(markers.iter().flat_map(|marker| shells_named(marker)))
            .collect();
        if alive.is_empty() {
            return;
        }
        assert!(
            started.elapsed() < timeout,
            "decoder processes remain alive: {alive:?} of {processes:?}"
        );
        std::thread::sleep(PROCESS_POLL);
    }
}

#[test]
fn cancel_kills_term_ignoring_decoder_and_descendant_then_joins() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    fixture.stub(&format!("{TERM_IGNORING_DECODER}wait"));
    let mut job = fixture.start(2, 2, ColorMode::Truecolor);
    let marker = fixture.path("bin/chafa.descendant");
    let started = Instant::now();
    let descendant = loop {
        if let Some(pid) = shells_named(&marker).first() {
            break *pid;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "decoder descendant did not start"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    crate::procsignal::signal_process(descendant, 15).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    assert!(
        shells_named(&marker).contains(&descendant),
        "test descendant must ignore SIGTERM"
    );
    let started = Instant::now();
    job.cancel();
    assert!(job.worker.is_none());
    assert!(job.result.recv().unwrap().is_err());
    assert!(shells_named(&marker).is_empty());
    assert!(shells_named(&fixture.path("bin/chafa")).is_empty());
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[test]
fn dropping_job_cancels_and_eventually_joins_pending_decoder() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    fixture.stub(&format!("{TERM_IGNORING_DECODER}wait"));
    let job = fixture.start(2, 2, ColorMode::Truecolor);
    let processes = wait_for_decoder(&fixture);
    let started = Instant::now();
    drop(job);
    assert!(started.elapsed() < CLEANUP_TIMEOUT);
    assert_processes_gone(&fixture, processes, started, CLEANUP_TIMEOUT);
}

#[test]
fn small_and_odd_rectangles_reach_decoder_without_rounding_or_overflow() {
    if !sandboxed() {
        return;
    }
    let fixture = Fixture::new();
    fixture.stub(
        r#"
size=; colors=; symbols=; ratio=
for arg do
    case $arg in
        --size=*) size=${arg#*=};;
        --colors=*) colors=${arg#*=};;
        --symbols=*) symbols=${arg#*=};;
        --font-ratio=*) ratio=${arg#*=};;
    esac
done
[ "$symbols" = none,u2580,u2584 ]
[ -n "$ratio" ]
case $colors in
    full) printf '\033[38;2;1;2;3m';;
    256) printf '\033[38;5;42m';;
    *) exit 4;;
esac
[ ! -w /input ]
width=${size%x*}; height=${size#*x}; row=0
while [ $row -lt $height ]; do
    col=0
    while [ $col -lt $width ]; do
        printf '▀'
        col=$((col+1))
    done
    printf '\n'
    row=$((row+1))
done
"#,
    );
    for colors in [ColorMode::Truecolor, ColorMode::Indexed] {
        for (columns, rows) in [(1, 1), (1, 3), (3, 1), (3, 5), (7, 3), (48, 14)] {
            let grid = fixture
                .start(columns, rows, colors)
                .result
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            assert_eq!(
                (grid.columns, grid.rows, grid.cells.len()),
                (columns, rows, columns * rows)
            );
            assert_eq!(
                grid.cells[0].foreground,
                if colors == ColorMode::Truecolor {
                    Color::Rgb(1, 2, 3)
                } else {
                    Color::Indexed(42)
                }
            );
        }
    }
    for (columns, rows) in [
        (0, 1),
        (1, 0),
        (4097, 1),
        (1, 2049),
        (usize::MAX, usize::MAX),
    ] {
        assert!(geometry(columns, rows).is_err());
    }
}
