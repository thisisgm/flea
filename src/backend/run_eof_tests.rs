// A piped client closes while a stale folder scan holds a standing, partial query's landing.
use super::*;
use crate::backend::searchjobs::Gate as Jobs;
use std::process::{Command, Stdio};

const CHILD: &str = "FLEA_TEST_SEARCH_EOF_CHILD";

#[test]
fn search_fix4_piped_eof_answers_the_deferred_walk_before_exit() {
    if std::env::var_os(CHILD).is_some() {
        piped_backend();
        return;
    }
    let root = TestDir::new("search-piped-eof");
    root.file("alpha.txt", "a");
    let named = crate::json::escape(root.path().to_str().unwrap());
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "backend::run::gate_tests::eof::search_fix4_piped_eof_answers_the_deferred_walk_before_exit", "--nocapture"])
        .env(CHILD, "1")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    writeln!(
        input,
        r#"{{"c":"searchopen","path":"{named}","folder":"{named}"}}"#
    )
    .unwrap();
    for _ in 0..2 {
        writeln!(
            input,
            r#"{{"c":"searchquery","query":"alpha","gen":1,"count":10}}"#
        )
        .unwrap();
    }
    drop(input);
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let searched: Vec<_> = stdout
        .lines()
        .filter(|line| field_str(line, "t").as_deref() == Some("searched"))
        .collect();
    assert_eq!(
        searched.len(),
        2,
        "a partial answer and its final landing: {stdout}"
    );
    assert!(searched[0].contains(r#""walking":true"#), "{stdout}");
    assert!(searched[1].contains(r#""walking":false"#), "{stdout}");
    assert_eq!(crate::json::field_usize(searched[1], "gen"), Some(1));
    assert_eq!(crate::json::field_usize(searched[1], "n"), Some(1));
}

// The real pipe reader feeds the real execution gate and dispatch; holds make the EOF ordering deterministic.
fn piped_backend() {
    let mut l = Loop::new("search-eof-loop");
    // The real stdin inbox reads the pipe on its own channel; the loop's channel stays the session's.
    let (input_tx, input_rx, pending) =
        crate::backend::inbox::open().expect("the stdin inbox opens");
    let mut inbox = pending.start(
        crate::backend::inbox::STDIN,
        input_tx,
        Arc::clone(&l.ops.live),
    );
    // Inbox::next has no deadline, so a watchdog fails the child if the requests and EOF never arrive.
    let read = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watched = Arc::clone(&read);
    std::thread::spawn(move || {
        std::thread::sleep(EVENT_DEADLINE);
        if !watched.load(std::sync::atomic::Ordering::SeqCst) {
            eprintln!("the piped request or EOF arrives");
            std::process::exit(101);
        }
    });
    let mut requests = VecDeque::new();
    loop {
        let event = inbox
            .next(&input_rx)
            .expect("the piped request or EOF arrives");
        let closed = matches!(event, Event::Closed);
        requests.push_back(event);
        if closed {
            read.store(true, std::sync::atomic::Ordering::SeqCst);
            break;
        }
    }
    let Event::Request(open) = requests.pop_front().unwrap() else {
        panic!("searchopen first")
    };
    let Request::SearchOpen {
        path,
        folder,
        hidden,
    } = parse_request(&open)
    else {
        panic!("searchopen first")
    };
    let _body = crate::backend::iomount::test_hold_body(remote_body(Path::new(&path)));
    let probes = Arc::new(crate::backend::searchwalk::WalkProbes::new());
    let (parked, release_walk) = probes.test_hold_walk_at(1);
    // The normal searchopen constructor with this session's walk hold armed before spawn.
    let mut session = Session::open_async_with_probes(&path, &folder, hidden, l.tx.clone(), probes);
    parked
        .recv_timeout(EVENT_DEADLINE)
        .expect("the walk is parked");
    session.hold_until = Some(Instant::now());
    l.st.search_session = Some(session);
    say(&mut l.out, &searchopened_line(&path));
    let (mut jobs, mut buffered) = (Jobs::default(), VecDeque::new());
    assert!(
        feed(
            &mut l,
            &mut jobs,
            &mut buffered,
            requests.pop_front().unwrap()
        ) == Control::Continue
    );
    let ready = l.wait(|event| matches!(event, Event::SearchReady));
    feed(&mut l, &mut jobs, &mut buffered, ready);
    assert!(l.st.search_session.as_ref().unwrap().standing_while_walking);
    // A folder scan claimed by the older generation must not replace this query when it lands.
    let hold = scanhold::hold(&folder, Stage::Read);
    let job = listwork::Job {
        path: folder,
        first: 10,
        line: r#"{"c":"searchquery","query":"","gen":0}"#.into(),
        hidden,
        want_changed: false,
        session: Some(listwork::ForSession {
            gen: 0,
            walking: true,
        }),
    };
    assert!(matches!(
        listwork::start(
            &mut l.out,
            &mut l.st,
            &l.pool,
            &mut l.watch,
            &mut l.gate,
            job,
            Arc::clone(&l.tb.mime)
        ),
        listwork::Started::Remote
    ));
    hold.entered
        .recv_timeout(EVENT_DEADLINE)
        .expect("the folder scan is out");
    // The non-empty retry and EOF both reach dispatch while the folder scan is still out.
    while let Some(event) = requests.pop_front() {
        assert!(feed(&mut l, &mut jobs, &mut buffered, event) == Control::Continue);
    }
    assert!(l.gate.closed() && l.gate.holds());
    release_walk.send(()).unwrap();
    let landed = l.landing();
    assert!(feed(&mut l, &mut jobs, &mut buffered, landed) == Control::Continue);
    hold.release();
    let listed = l.wait(is_list);
    let mut control = feed(&mut l, &mut jobs, &mut buffered, listed);
    while control == Control::Continue {
        let event = buffered.pop_front().unwrap_or_else(|| {
            l.rx.recv_timeout(EVENT_DEADLINE)
                .expect("the deferred landing or close arrives")
        });
        control = feed(&mut l, &mut jobs, &mut buffered, event);
    }
    print!("{}", l.said());
    l.st.search_session.take().unwrap().close();
}

fn feed(l: &mut Loop, jobs: &mut Jobs, buffered: &mut VecDeque<Event>, event: Event) -> Control {
    let Some(event) = jobs.route(
        event,
        &mut l.st,
        &mut l.out,
        &l.tb,
        &l.pool,
        &l.tx,
        buffered,
        l.gate.holds(),
    ) else {
        return Control::Continue;
    };
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
        Some(jobs),
    )
}
