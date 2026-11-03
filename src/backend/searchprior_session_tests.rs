use super::*;
use crate::backend::searchsession::Session;
use crate::backend::searchwalk::WalkProbes;
use crate::backend::testdir::TestDir;
use std::sync::Arc;

// Two equal-score rows and desktop history naming only abc2, with a provider taking `delay` seconds.
fn history_fixture(delay: &str) -> (TestDir, &'static Load) {
    let d = TestDir::new("searchprior-session");
    for dir in ["tree/abc1", "tree/abc2", "data/zoxide"] {
        d.dir(dir);
    }
    d.file("tree/abc1/report.txt", "");
    d.file("tree/abc2/report.txt", "");
    let tree = d.join("tree");
    let data = d.join("data");
    let history = d.file("data/recently-used.xbel", &format!(
        "<xbel><bookmark href='file://{}/abc2/report.txt' modified='2026-10-25T10:00:00Z'/></xbel>", tree.display()));
    d.file(
        "data/zoxide/db",
        &format!("  9.0 {}/abc2\n", tree.display()),
    );
    let zoxide = d.script("zoxide", &format!(
        "#!/bin/sh\n[ \"$*\" = 'query --list --all --score' ] || exit 9\nsleep {delay}\nexec cat '{}/zoxide/db'\n", data.display()));
    (d, Load::leaked(Some(history), zoxide))
}

fn open(d: &TestDir, load: &'static Load) -> Session {
    let root = d.join("tree");
    let root = root.to_str().expect("utf-8");
    let probes = Arc::new(WalkProbes::new());
    *probes.prior_load.lock().unwrap() = Some(load);
    let (walked, _events) = crate::backend::events::channel();
    let mut session = Session::open_async_with_probes(root, root, false, walked, probes);
    session.join_walk();
    session
}

// The test alone waits for the load, as a caller that needs it finished must.
fn finished(load: &'static Load) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while load.loaded().is_none() {
        assert!(Instant::now() < deadline, "the load finishes");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn first(session: &mut Session, gen: usize) -> String {
    session
        .query(gen, "report", 10)
        .expect("a newer generation answers")
        .rows[0]
        .0
        .clone()
}

#[test]
fn searchprior_session_open_before_the_load_never_waits_and_keeps_no_priors() {
    let (d, load) = history_fixture("1");
    let started = Instant::now();
    let mut session = open(&d, load);
    assert!(
        first(&mut session, 1).ends_with("abc1/report.txt"),
        "an open before the load ranks without priors"
    );
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "neither the open nor its answer waits on the load: {:?}",
        started.elapsed()
    );
    finished(load);
    assert!(
        first(&mut session, 2).ends_with("abc1/report.txt"),
        "a later load never reorders the same open"
    );
    assert!(session.priors.is_none());
}

#[test]
fn searchprior_session_open_after_the_load_ranks_history_from_the_first_answer() {
    let (d, load) = history_fixture("0");
    finished(load);
    let mut session = open(&d, load);
    assert!(
        first(&mut session, 1).ends_with("abc2/report.txt"),
        "the first answer of an open after the load ranks history first"
    );
    assert!(first(&mut session, 2).ends_with("abc2/report.txt"));
    assert!(session.priors.is_some_and(|priors| !priors.is_empty()));
}
