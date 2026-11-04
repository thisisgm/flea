use super::*;
use crate::backend::testdir::TestDir;

const WIRE_DEADLINE: Duration = Duration::from_secs(30);

#[test]
fn review_job_undo_patches_the_session() {
    use crate::backend::undo::{Entry, ItemIdentity, Step};
    let d = TestDir::new("review-job-sessionundo");
    d.file("alpha-new.txt", "a");
    d.file("alpha-old.txt", "squatter");
    d.file("beta-new.txt", "b");
    d.file("gamma-new.txt", "c");
    let root = d
        .path()
        .to_str()
        .expect("the sandbox path is utf-8")
        .to_string();
    let tb = Tables::load();
    let (events, events_rx) = crate::backend::events::channel::<Event>();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let (results, _done) = channel();
    let pool = Pool::new(
        1,
        results,
        d.join("cache"),
        Arc::clone(&tb.aliases),
        Arc::clone(&tb.thumbs),
    );
    let cache = Cache::new();
    let (tx, _rx) = channel();
    let mut ops = Ops::new(tx);
    let (mut watch, mut peeks) = (
        Watch::start(events.clone()),
        PeekWatch::start(events.clone()),
    );
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    let mut out = Vec::new();
    let mut ask = |line: &str, st: &mut State, ops: &mut Ops| -> Vec<String> {
        assert!(
            handle_line(
                line,
                &mut out,
                st,
                &tb,
                &pool,
                &cache,
                ops,
                &mut flashslot::Ctx::new(channel().0),
                &mut watch,
                &mut peeks,
                &mut fsinfo,
                &poller,
                &events,
                &mut Gate::new(events.clone())
            ) == Control::Continue
        );
        String::from_utf8(std::mem::take(&mut out))
            .expect("utf-8")
            .lines()
            .map(str::to_string)
            .collect()
    };
    let moved = |old: &str, new: &str| {
        let identity = ItemIdentity::record(
            &std::fs::metadata(d.path().join(new)).expect("the renamed file stats"),
        );
        Step::Moved {
            from: d.path().join(old),
            to: d.path().join(new),
            before: identity.clone(),
            after: identity,
            rename_fat: false,
        }
    };
    // Older entry: alpha's reversal finds its old name taken, so it fails after beta's reverts.
    let older = Entry {
        op: "rename".to_string(),
        steps: vec![
            moved("alpha-old.txt", "alpha-new.txt"),
            moved("beta-old.txt", "beta-new.txt"),
        ],
    };
    let key = ops.journal.push_for_job(Entry {
        op: "rename".to_string(),
        steps: vec![moved("gamma-old.txt", "gamma-new.txt")],
    });
    ops.jobs.adopt(
        &mut Vec::new(),
        1,
        "move",
        "",
        1,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    ops.jobs.rows[0].state = crate::backend::jobs::State::Done;
    ops.jobs.rows[0].undo = true;
    ops.jobs.rows[0].undo_key = key;
    ask(
        &format!(
            r#"{{"c":"searchopen","path":"{}","folder":"{}"}}"#,
            root, root
        ),
        &mut st,
        &mut ops,
    );
    let session = st.search_session.as_ref().expect("a session stands");
    crate::backend::searchwalkhold::wait_walk_landed(&events_rx, session, WIRE_DEADLINE, |_| false);
    let total =
        |lines: &[String]| crate::json::field_usize(lines.last().expect("a searched line"), "n");
    assert_eq!(
        total(&ask(
            r#"{"c":"searchquery","query":"gamma-new","gen":1,"count":100}"#,
            &mut st,
            &mut ops
        )),
        Some(1)
    );
    let opened = st
        .search_session
        .as_ref()
        .expect("a session stands")
        .probes_opened();
    let undone = ask(r#"{"c":"jobs","op":"undo","id":1}"#, &mut st, &mut ops);
    assert!(
        undone[0].contains(r#""t":"undone""#),
        "the whole undo answers undone: {undone:?}"
    );
    assert_eq!(
        total(&ask(
            r#"{"c":"searchquery","query":"gamma-old","gen":2,"count":100}"#,
            &mut st,
            &mut ops
        )),
        Some(1),
        "gamma-old shows after the undo"
    );
    assert_eq!(
        total(&ask(
            r#"{"c":"searchquery","query":"gamma-new","gen":3,"count":100}"#,
            &mut st,
            &mut ops
        )),
        Some(0),
        "gamma-new is gone after the undo"
    );
    let key = ops.journal.push_for_job(older);
    ops.jobs.adopt(
        &mut Vec::new(),
        2,
        "move",
        "",
        2,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    ops.jobs.rows[0].state = crate::backend::jobs::State::Done;
    ops.jobs.rows[0].undo = true;
    ops.jobs.rows[0].undo_key = key;
    let failed = ask(r#"{"c":"jobs","op":"undo","id":2}"#, &mut st, &mut ops);
    assert!(
        failed[0].contains(r#""t":"error""#),
        "the partial undo answers its error: {failed:?}"
    );
    assert!(
        d.path().join("beta-old.txt").exists(),
        "the newer step reverted on disk"
    );
    assert_eq!(
        total(&ask(
            r#"{"c":"searchquery","query":"beta-old","gen":4,"count":100}"#,
            &mut st,
            &mut ops
        )),
        Some(1),
        "beta-old shows after the partial undo"
    );
    assert_eq!(
        total(&ask(
            r#"{"c":"searchquery","query":"beta-new","gen":5,"count":100}"#,
            &mut st,
            &mut ops
        )),
        Some(0),
        "beta-new is gone after the partial undo"
    );
    let session = st.search_session.as_ref().expect("a session stands");
    assert!(
        !session.walking() && session.probes_opened() == opened,
        "both undos patch with no walk"
    );
}

#[test]
fn refused_requests_preserve_job_undo_offer() {
    let sandbox = TestDir::new("jobs-refused-request");
    let root = sandbox.path();
    let tb = Tables::load();
    let (events, _) = crate::backend::events::channel::<Event>();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let pool = Pool::new(
        1,
        channel().0,
        root.join("cache"),
        Arc::clone(&tb.aliases),
        Arc::clone(&tb.thumbs),
    );
    let cache = Cache::new();
    let mut ops = Ops::new(channel().0);
    let mut watch = Watch::start(events.clone());
    let mut peeks = PeekWatch::start(events.clone());
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    ops.jobs.adopt(
        &mut Vec::new(),
        1,
        "copy",
        "",
        1,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    ops.jobs.rows[0].state = crate::backend::jobs::State::Done;
    ops.jobs.rows[0].undo = true;
    let made = sandbox.file("made", "");
    ops.jobs.rows[0].undo_key = ops.journal.push_for_job(crate::backend::undo::Entry {
        op: "copy".into(),
        steps: vec![crate::backend::undo::Step::Created { path: made }],
    });
    assert!(ops.jobs.rows[0].undo_key.is_some());
    let mut out = Vec::new();
    let mut ask = |line: &str, st: &mut State, ops: &mut Ops| {
        handle_line(
            line,
            &mut out,
            st,
            &tb,
            &pool,
            &cache,
            ops,
            &mut flashslot::Ctx::new(channel().0),
            &mut watch,
            &mut peeks,
            &mut fsinfo,
            &poller,
            &events,
            &mut Gate::new(events.clone()),
        );
        String::from_utf8(std::mem::take(&mut out)).unwrap()
    };
    for line in [
        format!(
            r#"{{"c":"trash","rows":[0],"listing":{}}}"#,
            st.generation + 1
        ),
        format!(
            r#"{{"c":"mkdir","path":"{}","name":"bad/name"}}"#,
            root.display()
        ),
    ] {
        let text = ask(&line, &mut st, &mut ops);
        assert!(text.contains("error"), "request must be refused: {text}");
        assert!(
            ops.jobs.rows[0].undo,
            "refused request expired offer: {line} {text}"
        );
    }
    let accepted = format!(
        r#"{{"c":"mkdir","path":"{}","name":"accepted"}}"#,
        root.display()
    );
    let text = ask(&accepted, &mut st, &mut ops);
    assert!(
        !text.contains("error") && !ops.jobs.rows[0].undo,
        "an accepted write must expire the offer through its token: {text}"
    );
}
