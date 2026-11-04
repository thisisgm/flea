// A recursive watch delta held behind a session's folder scan, replayed with the requests that waited, driven through the same Loop as run_gate_tests.rs.
use super::*;
use crate::backend::searchjobs::Gate as Jobs;

// One event as run() takes it: the execution gate first, then the loop's own dispatch.
fn step(l: &mut Loop, jobs: &mut Jobs, buffered: &mut VecDeque<Event>, event: Event) {
    let holds = l.gate.holds();
    if let Some(event) = jobs.route(
        event, &mut l.st, &mut l.out, &l.tb, &l.pool, &l.tx, buffered, holds,
    ) {
        assert!(
            dispatch_inner(
                event,
                &mut l.out,
                &mut l.st,
                &l.tb,
                &l.pool,
                &l.cache,
                &mut l.ops,
                &mut l.flash,
                &mut l.watch,
                &mut l.peeks,
                &mut l.fsinfo,
                &l.poller,
                &l.tx,
                &mut l.gate,
                Some(jobs)
            ) == Control::Continue
        );
    }
}

#[test]
fn a_delta_held_behind_the_folder_scan_patches_the_index_before_the_query_that_waited() {
    let mut l = Loop::new("rungate-helddelta");
    l.dir.dir("scope");
    for i in 0..4 {
        l.dir.file(&format!("scope/alpha{i}.txt"), "a");
    }
    let scope = text(&l.dir.join("scope"));
    let mut session = Session::open_async(&scope, &scope, false, l.tx.clone());
    session.join_walk();
    session.query(1, "alpha", 10).expect("gen 1 answers");
    l.st.search_session = Some(session);
    // The joined walk has sent all it will, and the loop met it before this point.
    while l.rx.try_recv().is_ok() {}
    let (mut jobs, mut buffered) = (Jobs::default(), VecDeque::new());
    let line = r#"{"c":"searchquery","query":"","gen":1,"count":10}"#.to_string();
    l.gate.begin(listwork::Job {
        path: scope.clone(),
        first: 10,
        line,
        hidden: false,
        want_changed: false,
        session: Some(listwork::ForSession {
            gen: 1,
            walking: false,
        }),
    });
    let gone = l.dir.join("scope/alpha0.txt");
    l.dir.assert_contains(&gone);
    std::fs::remove_file(&gone).expect("the matching file is removed");
    let delta = l.wait(|event| matches!(event, Event::SearchDelta(_)));
    step(&mut l, &mut jobs, &mut buffered, delta);
    step(
        &mut l,
        &mut jobs,
        &mut buffered,
        Event::Request(r#"{"c":"searchquery","query":"alpha","gen":2,"count":10}"#.into()),
    );
    assert!(
        !jobs.busy() && l.out.is_empty(),
        "the delta and the query both wait for the folder scan"
    );
    let done = listwork::Done {
        result: Err(crate::backend::iomount::ListErr {
            error: crate::backend::iomount::internal_failure("list", &l.dir.join("scope")),
            mode: 0,
            watch_wd: -1,
        }),
    };
    step(&mut l, &mut jobs, &mut buffered, Event::List(done));
    // The loop's search steps, until the execution gate is idle with nothing buffered.
    loop {
        let event = match buffered.pop_front() {
            Some(event) => event,
            None if !jobs.busy() => break,
            None => {
                l.rx.recv_timeout(EVENT_DEADLINE)
                    .expect("the execution worker answers")
            }
        };
        step(&mut l, &mut jobs, &mut buffered, event);
    }
    let said = l.said();
    let searched: Vec<&str> = said
        .lines()
        .filter(|line| field_str(line, "t").as_deref() == Some("searched"))
        .collect();
    assert_eq!(
        searched.len(),
        1,
        "the waiting query answers once, on the patched index: {said}"
    );
    assert_eq!(
        crate::json::field_usize(searched[0], "gen"),
        Some(2),
        "{said}"
    );
    assert_eq!(
        crate::json::field_usize(searched[0], "n"),
        Some(3),
        "{said}"
    );
    assert!(
        !said.contains("alpha0.txt"),
        "no answer names the removed file: {said}"
    );
    l.st.search_session
        .take()
        .expect("the session stands")
        .close();
}

// Steps every event the loop meets until `done` holds with the execution gate idle, so the test reads what run() would publish.
fn run_until(l: &mut Loop, jobs: &mut Jobs, buffered: &mut VecDeque<Event>, done: &dyn Fn(&Loop, &Jobs) -> bool) {
    let deadline = std::time::Instant::now() + EVENT_DEADLINE;
    while !(buffered.is_empty() && !jobs.busy() && done(l, jobs)) {
        let event = match buffered.pop_front() {
            Some(event) => event,
            None => l.rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())).expect("the loop meets its next event"),
        };
        step(l, jobs, buffered, event);
    }
}

// Walk progress, a newer generation and a watch change met mid-walk all pass the production execution gate; only the newest generation answers after the landing, with the change in it.
#[test]
fn m9_integ_stream_progress_and_watch_landing_preserve_generation() {
    let mut l = Loop::new("m9integ-streamgen");
    l.dir.dir("scope/a");
    l.dir.dir("scope/b");
    for i in 0..4 {
        l.dir.file(&format!("scope/a/alpha{i}.txt"), "a");
    }
    l.dir.file("scope/b/alpha_b.txt", "a");
    let scope = text(&l.dir.join("scope"));
    let probes = Arc::new(crate::backend::searchwalk::WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_in("b");
    let session = Session::open_async_with_probes(&scope, &scope, false, l.tx.clone(), probes);
    let walk_done = Arc::clone(&session.walk_done);
    l.st.search_session = Some(session);
    parked.recv_timeout(EVENT_DEADLINE).expect("the walk parks in b");
    let (mut jobs, mut buffered) = (Jobs::default(), VecDeque::new());
    let query = |gen: usize| Event::Request(format!(r#"{{"c":"searchquery","query":"alpha","gen":{gen},"count":20}}"#));
    step(&mut l, &mut jobs, &mut buffered, query(1));
    run_until(&mut l, &mut jobs, &mut buffered, &|l, _| l.st.search_session.as_ref().is_some_and(|s| s.last_answered_gen == 1));
    // Progress shares the landing event; it reaches the gate while the walk is still parked.
    let progress = l.wait(|event| matches!(event, Event::SearchWalked));
    assert!(!walk_done.load(std::sync::atomic::Ordering::Acquire), "the walk is still parked, so this SearchWalked is progress");
    step(&mut l, &mut jobs, &mut buffered, progress);
    run_until(&mut l, &mut jobs, &mut buffered, &|_, _| true);
    let walking = l.said();
    let first: Vec<&str> = walking.lines().filter(|line| field_str(line, "t").as_deref() == Some("searched")).collect();
    assert!(!first.is_empty(), "gen 1 answers through the gate while walking: {walking}");
    assert!(first.iter().all(|line| crate::json::field_usize(line, "gen") == Some(1) && line.contains(r#""walking":true"#)), "{walking}");
    step(&mut l, &mut jobs, &mut buffered, query(2));
    l.dir.file("scope/alpha_new.txt", "a");
    let _ = release.send(());
    run_until(&mut l, &mut jobs, &mut buffered, &|l, _| {
        walk_done.load(std::sync::atomic::Ordering::Acquire)
            && l.st.search_session.as_ref().is_some_and(|s| s.announced && s.last_answered_gen == 2 && !s.standing_while_walking)
    });
    let said = l.said();
    let searched: Vec<&str> = said.lines().filter(|line| field_str(line, "t").as_deref() == Some("searched")).collect();
    assert!(searched.iter().all(|line| crate::json::field_usize(line, "gen") == Some(2)), "no stale gen 1 publishes after gen 2 is asked: {said}");
    let last = searched.last().unwrap_or_else(|| panic!("gen 2 answers: {said}"));
    assert!(last.contains(r#""walking":false"#), "the landing answers gen 2 settled: {said}");
    assert_eq!(crate::json::field_usize(last, "n"), Some(6), "the landed answer holds every walked row and the mid-walk change: {said}");
    l.st.search_session.take().expect("the session stands").close();
}

#[test]
fn a_refresh_held_behind_the_folder_scan_answers_once_the_scan_lands() {
    let mut l = Loop::new("rungate-heldrefresh");
    l.dir.dir("scope");
    for i in 0..3 {
        l.dir.file(&format!("scope/alpha{i}.txt"), "a");
    }
    let scope = text(&l.dir.join("scope"));
    let mut session = Session::open_async(&scope, &scope, false, l.tx.clone());
    session.join_walk();
    session.query(2, "alpha", 10).expect("gen 2 answers");
    l.st.search_session = Some(session);
    while l.rx.try_recv().is_ok() {}
    let (mut jobs, mut buffered) = (Jobs::default(), VecDeque::new());
    // The scan was claimed by gen 1 while gen 2 stands, so its failed Done owes no answer and the refresh alone answers gen 2.
    let line = r#"{"c":"searchquery","query":"","gen":1,"count":10}"#.to_string();
    l.gate.begin(listwork::Job {
        path: scope.clone(),
        first: 10,
        line,
        hidden: false,
        want_changed: false,
        session: Some(listwork::ForSession {
            gen: 1,
            walking: false,
        }),
    });
    step(&mut l, &mut jobs, &mut buffered, Event::SearchRefresh);
    assert!(
        !jobs.busy() && l.out.is_empty(),
        "the refresh waits for the folder scan"
    );
    let done = listwork::Done {
        result: Err(crate::backend::iomount::ListErr {
            error: crate::backend::iomount::internal_failure("list", &l.dir.join("scope")),
            mode: 0,
            watch_wd: -1,
        }),
    };
    step(&mut l, &mut jobs, &mut buffered, Event::List(done));
    loop {
        let event = match buffered.pop_front() {
            Some(event) => event,
            None if !jobs.busy() => break,
            None => {
                l.rx.recv_timeout(EVENT_DEADLINE)
                    .expect("the execution worker answers")
            }
        };
        step(&mut l, &mut jobs, &mut buffered, event);
    }
    let said = l.said();
    let searched: Vec<&str> = said
        .lines()
        .filter(|line| field_str(line, "t").as_deref() == Some("searched"))
        .collect();
    assert_eq!(
        searched.len(),
        1,
        "the held refresh re-answers the standing query once the scan lands: {said}"
    );
    assert_eq!(
        crate::json::field_usize(searched[0], "gen"),
        Some(2),
        "the searched line is the refresh's standing gen, never the stale scan's gen 1: {said}"
    );
    assert_eq!(
        crate::json::field_usize(searched[0], "n"),
        Some(3),
        "{said}"
    );
    assert!(
        !said.contains(r#""t":"error""#),
        "the stale scan's failed Done says nothing: {said}"
    );
    l.st.search_session
        .take()
        .expect("the session stands")
        .close();
}
