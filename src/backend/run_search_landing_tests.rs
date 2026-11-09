use super::*;
use crate::backend::testdir::TestDir;
use crate::json::{field_str, field_usize};

const DEADLINE: Duration = Duration::from_secs(30);

struct Wire {
    tb: Tables,
    events: crate::backend::events::Sender<Event>,
    receiver: Receiver<Event>,
    pending: VecDeque<Event>,
    st: State,
    pool: Pool,
    cache: Cache,
    ops: Ops,
    flash: flashslot::Ctx,
    watch: Watch,
    peeks: PeekWatch,
    fsinfo: FsInfo,
    poller: super::super::watchpoll::Poller,
    gate: Gate,
    release: std::sync::mpsc::Sender<()>,
}

impl Wire {
    fn parked(d: &TestDir) -> Self {
        d.dir("docs");
        // A sibling fork holding this write open would land its IN_CLOSE_WRITE in the session armed below, after the landing.
        external_file(d, "bench.txt", "");
        let root = d.path().to_str().expect("utf-8 sandbox path");
        let (events, receiver) = crate::backend::events::channel();
        let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
        let probes = Arc::new(crate::backend::searchwalk::WalkProbes::new());
        let (parked, release) = probes.test_hold_walk_in("docs");
        st.search_session = Some(Session::open_async_with_probes(root, root, false, events.clone(), probes));
        parked.recv_timeout(DEADLINE).expect("the walk parks before reading docs");
        Self {
            tb: Tables::load(),
            watch: Watch::start(events.clone()),
            peeks: PeekWatch::start(events.clone()),
            fsinfo: FsInfo::new(events.clone()),
            poller: super::super::watchpoll::Poller::new(events.clone()),
            gate: Gate::new(events.clone()),
            events,
            receiver,
            pending: VecDeque::new(),
            st,
            pool: Pool::idle(),
            cache: Cache::at(d.join("cache")),
            ops: Ops::new(channel().0),
            flash: flashslot::Ctx::new(channel().0),
            release,
        }
    }

    fn event(&mut self, event: Event) -> Vec<String> {
        let mut out = Vec::new();
        assert!(dispatch(event, &mut out, &mut self.st, &self.tb, &self.pool, &self.cache,
            &mut self.ops, &mut self.flash, &mut self.watch, &mut self.peeks, &mut self.fsinfo,
            &self.poller, &self.events, &mut self.gate) == Control::Continue);
        String::from_utf8(out).expect("utf-8 protocol").lines().map(str::to_string).collect()
    }

    fn ask(&mut self, request: &str) -> Vec<String> {
        self.event(Event::Request(request.to_string()))
    }

    fn next(&mut self, kind: fn(&Event) -> bool) -> Event {
        if let Some(at) = self.pending.iter().position(kind) {
            return self.pending.remove(at).unwrap();
        }
        let deadline = Instant::now() + DEADLINE;
        loop {
            let event = self.receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("the production event arrives");
            if kind(&event) { return event; }
            self.pending.push_back(event);
        }
    }

    fn delta_queue_is_empty(&mut self) -> bool {
        let core = &self.st.search_session.as_ref().unwrap().watch.as_ref().unwrap().core;
        core.flush();
        let delta = core.drain();
        self.pending.extend(self.receiver.try_iter());
        delta.dirs.is_empty() && delta.resets.is_empty() && !delta.failed && !delta.overflow
            && !self.pending.iter().any(|event| matches!(event, Event::SearchDelta(_)))
    }
}

fn check_answer(lines: &[String], n: usize) {
    let kinds: Vec<_> = lines.iter().map(|line| field_str(line, "t").unwrap_or_default()).collect();
    assert_eq!(kinds, ["listed", "rows", "searched"], "one atomic answer: {lines:?}");
    for line in lines {
        assert_eq!(field_usize(line, "gen"), Some(41), "the standing request owns every reply");
    }
    assert_eq!(field_usize(&lines[0], "n"), Some(n));
    assert_eq!(field_usize(&lines[2], "n"), Some(n));
    assert!(lines[2].contains(r#""walking":false"#), "{lines:?}");
    assert!(lines[2].contains(r#""partial":false"#), "{lines:?}");
}

// Sibling test forks cannot inherit this writer and delay IN_CLOSE_WRITE beyond the returned path.
fn external_file(d: &TestDir, name: &str, body: &str) -> std::path::PathBuf {
    let path = d.join(name);
    d.assert_contains(&path);
    let status = std::process::Command::new("/bin/sh")
        .args(["-c", "printf '%s' \"$2\" > \"$1\"", "sh"])
        .arg(&path)
        .arg(body)
        .status()
        .expect("external fixture writer starts");
    assert!(status.success(), "external fixture writer completes");
    path
}

#[test]
fn a_watch_delta_after_walk_landing_is_published_once() {
    for held in [false, true] {
        let d = TestDir::new("watch-landing");
        let mut wire = Wire::parked(&d);
        let request = r#"{"c":"searchquery","query":"needle","gen":41,"count":3,"sizes":0}"#;
        if held { assert!(wire.ask(request).is_empty(), "the running walk holds the query"); }
        wire.release.send(()).unwrap();
        let walked = wire.next(|event| matches!(event, Event::SearchWalked));
        let initial = if held {
            let timer = wire.next(|event| matches!(event, Event::SearchHeld));
            wire.event(timer)
        } else {
            wire.ask(request)
        };
        check_answer(&initial, 0);
        assert_eq!(wire.st.listing.len(), 0);
        assert!(wire.st.search_session.as_ref().unwrap().landing().is_none());

        let path = external_file(&d, "docs/needle-a", "a");
        let delta = wire.next(|event| matches!(event, Event::SearchDelta(_)));
        let token = match &delta { Event::SearchDelta(token) => *token, _ => unreachable!() };
        assert_eq!(token, wire.st.search_session.as_ref().unwrap().watch_token());
        let wrong_owner = token.checked_add(1).expect("a second session token is representable");
        assert!(wire.event(Event::SearchDelta(wrong_owner)).is_empty(), "another session cannot consume the pending delta");
        assert_eq!(wire.st.listing.len(), 0);

        let landed = wire.event(walked);
        check_answer(&landed, 1);
        assert!(landed[1].contains("docs/needle-a"), "the reply carries the externally created file");
        assert_eq!(wire.st.listing.len(), 1, "the actual consumer adopted the matching reply");
        assert_eq!(wire.st.listing.name(0), "docs/needle-a");
        let paths = wire.ask(r#"{"c":"paths","rows":[0]}"#);
        assert_eq!(paths.len(), 1);
        assert_eq!(field_str(&paths[0], "t").as_deref(), Some("paths"));
        let reply = crate::jsondoc::parse(&paths[0]).expect("valid paths reply");
        let actual_paths = reply.get("paths").and_then(crate::jsondoc::Json::as_array).unwrap();
        assert_eq!(actual_paths.len(), 1);
        assert_eq!(actual_paths[0].as_str(), path.to_str(), "the adopted row resolves its actual identity");

        assert!(wire.event(Event::SearchWalked).is_empty(), "a landing announces once");
        assert!(wire.event(delta).is_empty(), "the queued event cannot republish a consumed delta");
        assert!(wire.ask(r#"{"c":"searchquery","query":"bench","gen":40}"#).is_empty(), "a stale query cannot replace the standing owner");
        assert_eq!(wire.st.listing.name(0), "docs/needle-a");

        assert!(wire.delta_queue_is_empty(), "all kernel, channel and buffered deltas precede needle-b");
        let second = external_file(&d, "docs/needle-b", "b");
        let later_delta = wire.next(|event| matches!(event, Event::SearchDelta(_)));
        assert!(matches!(&later_delta, Event::SearchDelta(later_token) if *later_token == token));
        assert!(wire.event(Event::SearchDelta(wrong_owner)).is_empty());
        assert_eq!(wire.st.listing.len(), 1, "wrong-owner delivery leaves the accepted snapshot intact");
        let later = wire.event(later_delta);
        check_answer(&later, 2);
        assert!(later[1].contains("docs/needle-a") && later[1].contains("docs/needle-b"));
        assert_eq!(wire.st.listing.len(), 2);
        assert!((0..wire.st.listing.len()).any(|row| wire.st.base.join(wire.st.listing.name(row)) == second));
        assert!(wire.event(Event::SearchDelta(token)).is_empty(), "the later delta also publishes once");
        wire.st.search_session.take().unwrap().close();
    }
}

#[test]
fn a_watch_landing_publishes_once_beside_forking_siblings() {
    crate::backend::testdir::with_forking_siblings(|| {
        for _ in 0..4 {
            a_watch_delta_after_walk_landing_is_published_once();
        }
    });
}

#[test]
fn a_stale_delta_cannot_pass_the_external_change_barrier() {
    let d = TestDir::new("watch-delta-barrier");
    let mut wire = Wire::parked(&d);
    wire.release.send(()).unwrap();
    let walked = wire.next(|event| matches!(event, Event::SearchWalked));
    assert!(wire.event(walked).is_empty());
    assert!(wire.delta_queue_is_empty());
    let token = wire.st.search_session.as_ref().unwrap().watch_token();
    wire.pending.push_back(Event::SearchDelta(token));
    assert!(!wire.delta_queue_is_empty(), "a stale wakeup cannot certify the next mutation");
    wire.pending.clear();
    assert!(wire.delta_queue_is_empty());
    wire.st.search_session.take().unwrap().close();
}
