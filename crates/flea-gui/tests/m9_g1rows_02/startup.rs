//! Startup observations and their existing measurement controls.
use super::helpers::{prior_count, PRIOR_ARGV};
use super::probe::{self, probe, probed};
use super::process::{
    child_argv, children, cpu_ticks, sandboxed, threads, Observer, Process, SETTLE,
};
use super::{FAR_COUNT, FAR_START};
use flea::protocol::{parse_reply, Reply};
use flea_gui::backend::list_line;
use flea_gui::navigation::Listing;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct Startup {
    pub(crate) total: usize,
    pub(crate) start: usize,
    pub(crate) rows: usize,
    pub(crate) stats: usize,
    pub(crate) spawns: usize,
    pub(crate) named_opens: usize,
    pub(crate) children: usize,
    pub(crate) child_argv: Vec<String>,
    pub(crate) late_rows: usize,
    pub(crate) threads: usize,
    pub(crate) cpu_ticks: u64,
    pub(crate) listed: String,
    pub(crate) rows_ms: String,
    pub(crate) spawn_argv: String,
    pub(crate) timing: super::timing::Timing,
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

pub(crate) fn startup(root: &Path, dir: &Path, listing: &Listing) -> Startup {
    sample(root, dir, listing, SETTLE, |_| {}, || {}, || {})
}

pub(crate) fn sample(
    root: &Path,
    dir: &Path,
    listing: &Listing,
    settle: Duration,
    configure: impl FnOnce(&mut Command),
    ready: impl FnOnce(),
    finished: impl FnOnce(),
) -> Startup {
    let log = root.join(format!("probe-{}.log", listing.first));
    fs::write(&log, b"").unwrap();
    let mut observer = Observer::watch(dir);
    assert_eq!(
        observer.named(),
        0,
        "the fixture is quiet before the backend starts"
    );
    let mut process = Process::start_with(root, dir, &log, configure);
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
    let late = process.late_for(settle);
    ready();
    let late_rows = late
        .iter()
        .filter(|line| matches!(parse_reply(line), Ok(Reply::Rows { .. })))
        .count();
    let kids = children(pid);
    let found = Startup {
        total,
        start,
        rows: rows.len(),
        stats: probed(&log, b's'),
        spawns: probed(&log, b'p'),
        named_opens: observer.named(),
        children: kids.len(),
        child_argv: kids.into_iter().map(child_argv).collect(),
        late_rows,
        threads: threads(pid),
        cpu_ticks: cpu_ticks(pid),
        listed: listed.clone(),
        rows_ms: rows_ms(&rows_line),
        spawn_argv: probe::spawn_argv(&log),
        timing: super::timing::read(&log),
    };
    finished();
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
    shell
        .arg("-c")
        .arg("(/bin/true \"$1\"); /bin/true")
        .arg("spawn-probe")
        .arg("space \"quote\"\nline");
    sandboxed(&mut shell, root);
    let before = probed(&log, b'p');
    assert!(shell
        .env("LD_PRELOAD", probe())
        .env("FLEA_PROBE_LOG", &log)
        .env("FLEA_PROBE_SPAWNS", log.with_extension("spawns"))
        .status()
        .unwrap()
        .success());
    assert!(probed(&log, b'p') > before, "the probe counts a libc spawn");
    assert!(
        probe::spawn_argv(&log).contains(r#""/bin/true" "/bin/true" "space \"quote\"\x0aline""#),
        "the probe preserves argv boundaries and bytes after the helper exits"
    );
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

impl Startup {
    pub(crate) fn assert_helpers(&self, label: &str) {
        let prior = prior_count(&self.spawn_argv).unwrap_or_else(|reason| {
            panic!("{label}: {reason}; child path/argv: {}", self.spawn_argv)
        });
        assert!(
            self.spawns <= prior,
            "{label}: no other spawn is allowed: spawns={} prior={prior}; child path/argv: {}",
            self.spawns,
            self.spawn_argv
        );
        assert!(
            self.children <= prior,
            "{label}: no other child is allowed: children={} prior={prior}; child path/argv: {}",
            self.children,
            self.spawn_argv
        );
        // An empty cmdline is a child that already exited, so only a live child can be named.
        for argv in self.child_argv.iter().filter(|argv| !argv.is_empty()) {
            assert!(
                argv == PRIOR_ARGV,
                "{label}: unexpected child {argv}; child path/argv: {}",
                self.spawn_argv
            );
        }
        if prior != 0 {
            self.timing.assert_prior_after_grace(&self.spawn_argv);
        }
    }
}
