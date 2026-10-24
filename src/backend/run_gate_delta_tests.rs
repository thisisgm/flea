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
