//! The native picture must reuse the window's faces without a process-wide SVG font scan, and its code must be read from disk before the first drag.
use super::{raster, REHEARSED, REHEARSES_IN_TESTS, SETTLE};
use crate::{g2::file_clipboard::carry, list::bench, list::Palette, row::marks::Mark};
use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

/// The `<text x>` raster draws the name at; the marks end left of it.
const NAME_LEFT: usize = 47;

/// Runs the named test of this module alone in a process of `binary` with `variable` set; its output once it passes.
fn alone(binary: &Path, name: &str, variable: &str, value: &str) -> String {
    let test = format!("{}::{name}", module_path!());
    let output = std::process::Command::new(binary)
        .args([
            "--exact",
            test.strip_prefix("flea_gui::").unwrap(),
            "--nocapture",
        ])
        .env(variable, value)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fresh-process drag test failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Runs the named test of this module alone in a process of its own; true inside that process, which prints `done` when it passes.
fn fresh_process(name: &str, child: &str, done: &str) -> bool {
    if std::env::var_os(child).is_some() {
        return true;
    }
    let stdout = alone(&std::env::current_exe().unwrap(), name, child, "1");
    let line = stdout.lines().find(|line| line.contains(done));
    println!(
        "{}",
        line.expect("the fresh process must report that it passed")
    );
    false
}

/// This thread's major page faults so far, each one a read from disk it waited for: proc(5) stat, the tenth field after the name.
// Sample input: "4242 (flea_gui-0abde) R 4100 4242 4100 0 -1 4194560 3950 0 52 0 ..." gives 52.
fn major_faults() -> u64 {
    let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
    let fields = stat.rsplit_once(')').unwrap().1;
    fields.split_whitespace().nth(9).unwrap().parse().unwrap()
}

/// Drops this test binary's pages no process has mapped from the page cache, as a reboot leaves them.
fn forget_own_code() {
    use std::os::unix::io::AsRawFd;
    extern "C" {
        fn posix_fadvise(fd: i32, offset: i64, len: i64, advice: i32) -> i32;
    }
    // posix_fadvise(2) POSIX_FADV_DONTNEED; dirty pages stay, so a binary just written is synced first.
    const POSIX_FADV_DONTNEED: i32 = 4;
    let binary = std::fs::File::open(std::env::current_exe().unwrap()).unwrap();
    binary.sync_all().unwrap();
    assert_eq!(
        unsafe { posix_fadvise(binary.as_raw_fd(), 0, 0, POSIX_FADV_DONTNEED) },
        0
    );
}

/// A copy of this test binary in a marked `mktemp -d` root beside it: a file no other process has mapped, so every page of its drag code can be dropped.
struct PrivateCopy(PathBuf);

impl PrivateCopy {
    fn new() -> Self {
        let own = std::env::current_exe().unwrap();
        let beside = own.parent().unwrap();
        let made = std::process::Command::new("mktemp")
            .arg("-d")
            .arg(beside.join("flea-first-drag.XXXXXXXX"))
            .output()
            .expect("mktemp runs");
        assert!(made.status.success(), "mktemp -d failed");
        let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim_end());
        assert!(root.is_absolute() && root.starts_with(beside) && root != beside);
        std::fs::write(root.join(".flea-test-root"), b"").unwrap();
        let copy = Self(root);
        // cp holds the write descriptor, so no child a sibling test forks inherits it and the exec cannot meet ETXTBSY.
        let copied = std::process::Command::new("cp")
            .arg(&own)
            .arg(copy.binary())
            .status()
            .expect("cp runs");
        assert!(copied.success(), "cp of the test binary failed");
        copy
    }

    fn binary(&self) -> PathBuf {
        self.0.join("flea_gui")
    }
}

impl Drop for PrivateCopy {
    fn drop(&mut self) {
        let beside = std::env::current_exe().unwrap();
        let owned = self.0.is_absolute()
            && beside
                .parent()
                .is_some_and(|beside| self.0.starts_with(beside) && self.0 != beside)
            && self.0.join(".flea-test-root").is_file();
        if owned {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// The count a child printed after `label`.
fn reported(output: &str, label: &str) -> u64 {
    let (_, count) = output
        .lines()
        .find_map(|line| line.split_once(label))
        .expect("the fresh process must report its count");
    println!("{label}{count}");
    count.trim().parse().unwrap()
}

const COLD: &str = "first_drag_reads_no_code_from_disk_on_the_ui_thread";
const COLD_CHILD: &str = "FLEA_FIRST_DRAG_COLD_TEST";
const UNREHEARSED: &str = "FIRST_DRAG_UNREHEARSED raster_major_faults=";
const REHEARSED_OK: &str = "FIRST_DRAG_COLD_OK rehearsal_major_faults=";

/// One cold process: drops its own code, optionally lets the window rehearse as the product schedules it, then draws the first drag picture.
fn cold_drag(rehearsed: bool) {
    forget_own_code();
    let mut cx = bench::real_cx();
    let root = crate::g2::drag_drop::mounted_tests::Root::new("first-drag-cold");
    let (handle, _) = crate::g2::drag_drop::mounted_tests::open(&mut cx, root.src());
    let before = major_faults();
    if rehearsed {
        REHEARSES_IN_TESTS.store(true, Ordering::Relaxed);
        handle
            .update(&mut cx, |shell, window, cx| {
                shell.follow_file_drags(window, cx)
            })
            .unwrap();
        cx.run_until_parked();
        assert!(
            !REHEARSED.load(Ordering::Relaxed),
            "a window that has not settled must not rehearse"
        );
        cx.executor().advance_clock(SETTLE);
        cx.run_until_parked();
        assert!(
            REHEARSED.load(Ordering::Relaxed),
            "a settled window must have rehearsed its drag picture"
        );
    }
    let rehearsal = major_faults() - before;
    let drawing = handle
        .update(&mut cx, |shell, window, cx| {
            let font = shell.active_typeface().font;
            let palette =
                Palette::inherited(&window.text_style(), crate::palette::Theme::default());
            let flock = carry::Flock {
                marks: vec![Mark::Image],
                name: "photo_2026-11-06_23-43-51.jpg".into(),
            };
            let before = major_faults();
            raster(&flock, &font, palette, window, cx).unwrap();
            major_faults() - before
        })
        .unwrap();
    if !rehearsed {
        println!("{UNREHEARSED}{drawing}");
        return;
    }
    assert_eq!(
        drawing, 0,
        "the first drag picture must not wait for code to be read from disk"
    );
    println!("{REHEARSED_OK}{rehearsal}");
}

#[test]
fn first_drag_reads_no_code_from_disk_on_the_ui_thread() {
    if let Some(mode) = std::env::var_os(COLD_CHILD) {
        return cold_drag(mode == "rehearsed");
    }
    // Both children run a private copy: pages another process has mapped stay in the page cache, and every process of this binary maps them.
    let copy = PrivateCopy::new();
    let unrehearsed = reported(
        &alone(&copy.binary(), COLD, COLD_CHILD, "unrehearsed"),
        UNREHEARSED,
    );
    assert!(
        unrehearsed > 0,
        "an unrehearsed first drag read no code from disk: this test binary's pages cannot be dropped from the page cache here (a target directory on tmpfs keeps them), so the test cannot tell a cold drag from a warm one"
    );
    let rehearsal = reported(
        &alone(&copy.binary(), COLD, COLD_CHILD, "rehearsed"),
        REHEARSED_OK,
    );
    assert!(
        rehearsal > 0,
        "the rehearsal read no code from disk, yet an unrehearsed drag waited for {unrehearsed} reads"
    );
}

#[test]
fn first_drag_never_loads_the_svg_system_font_database() {
    if !fresh_process(
        "first_drag_never_loads_the_svg_system_font_database",
        "FLEA_FIRST_DRAG_FONT_TEST",
        "FIRST_DRAG_FONTS_OK",
    ) {
        return;
    }
    let mut cx = bench::real_cx();
    let root = crate::g2::drag_drop::mounted_tests::Root::new("first-drag-fonts");
    let (handle, _) = crate::g2::drag_drop::mounted_tests::open(&mut cx, root.src());
    handle
        .update(&mut cx, |shell, window, cx| {
            let font = shell.active_typeface().font;
            let palette =
                Palette::inherited(&window.text_style(), crate::palette::Theme::default());
            let renderer = cx.svg_renderer();
            assert_eq!(renderer.system_font_database_load_count(), 0);
            for name in ["alpha.txt", "日本語.txt", "مرحبا.txt", "café.txt"] {
                let flock = carry::Flock {
                    marks: vec![Mark::FileText],
                    name: name.into(),
                };
                for _ in 0..2 {
                    let image = raster(&flock, &font, palette, window, cx).unwrap();
                    let width = image.width() as usize;
                    let left = NAME_LEFT * window.scale_factor().ceil() as usize;
                    let mut pixels = image.rgba().as_chunks::<4>().0.iter().enumerate();
                    assert!(
                        pixels.any(|(i, p)| i % width >= left && p[3] > 0),
                        "{name} must draw name glyphs right of the marks"
                    );
                    assert_eq!(
                        renderer.system_font_database_load_count(),
                        0,
                        "drawing {name} must not scan system fonts, even on its first drag"
                    );
                }
            }
        })
        .unwrap();
    println!("FIRST_DRAG_FONTS_OK");
}
