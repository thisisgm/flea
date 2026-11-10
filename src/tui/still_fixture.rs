use super::*;

pub(super) fn key(path: &Path, generation: usize) -> Key {
    Key {
        path: path.into(),
        generation,
        page: (0, 0, 0),
        cells: (1, 1),
        colors: ColorMode::Truecolor,
    }
}

pub(super) fn still(fixture: &Fixture) -> Still {
    let mut still = Still::default();
    let spool = fixture.path("spool");
    if !spool.exists() {
        std::fs::create_dir(&spool).unwrap();
    }
    still.places = Places {
        search: Some(fixture.path("bin").into_os_string()),
        spool: Some(spool),
    };
    still
}

pub(super) fn settle(still: &mut Still) {
    let started = Instant::now();
    while still.running() {
        still.poll();
        assert!(
            started.elapsed() < SETTLE,
            "the half-block decode did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

// Shells running the stub script, by their command line.
pub(super) fn shells(marker: &Path) -> Vec<i32> {
    use std::os::unix::ffi::OsStrExt;
    std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse::<i32>().ok()?;
            let cmdline = std::fs::read(entry.path().join("cmdline")).ok()?;
            cmdline
                .split(|b| *b == 0)
                .any(|arg| arg == marker.as_os_str().as_bytes())
                .then_some(pid)
        })
        .collect()
}
