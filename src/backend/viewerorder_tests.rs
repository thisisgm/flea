use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn image_paths_follow_the_held_listing_and_refuse_stale_numbering() {
    let dir = TestDir::new("viewerorder-wire");
    let mut tb = Tables::load();
    tb.mime = Arc::new(super::super::mime::Db::from_str(
        "50:image/png:*.png\n50:text/plain:*.txt\n",
    ))
    .into();
    let (events, _received) = crate::backend::events::channel();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    st.base = dir.join("unscanned");
    st.generation = 7;
    for (name, directory) in [
        ("z.png", false),
        ("notes.txt", false),
        ("folder.png", true),
        ("a.png", false),
    ] {
        st.listing.push(name, directory);
    }
    let (results, _done) = channel();
    let pool = Pool::new(
        1,
        results,
        dir.join("cache"),
        Arc::clone(&tb.aliases),
        Arc::clone(&tb.thumbs),
    );
    let cache = Cache::new();
    let mut ops = Ops::new(channel().0);
    let mut flash = flashslot::Ctx::new(channel().0);
    let mut watch = Watch::start(events.clone());
    let mut peeks = PeekWatch::start(events.clone());
    let mut fsinfo = FsInfo::new(events.clone());
    let poller = super::super::watchpoll::Poller::new(events.clone());
    let mut gate = Gate::new(events.clone());
    let mut ask = |line: &str| {
        let mut out = Vec::new();
        assert!(
            handle_line(
                line,
                &mut out,
                &mut st,
                &tb,
                &pool,
                &cache,
                &mut ops,
                &mut flash,
                &mut watch,
                &mut peeks,
                &mut fsinfo,
                &poller,
                &events,
                &mut gate
            ) == Control::Continue
        );
        String::from_utf8(out).unwrap()
    };
    let base = dir.join("unscanned");
    let paths = |names: &[&str]| {
        names
            .iter()
            .map(|name| base.join(name).to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    };
    for (request, expected) in [
        (
            r#"{"c":"paths","images":3,"listing":7}"#,
            paths(&["a.png", "z.png"]),
        ),
        (
            r#"{"c":"paths","images":3,"listing":7,"scoped":true,"rows":[3,0,3],"selected":true}"#,
            paths(&["z.png", "a.png"]),
        ),
        (
            r#"{"c":"paths","images":3,"listing":7,"scoped":true,"rows":[0,3]}"#,
            paths(&["a.png", "z.png"]),
        ),
        (
            r#"{"c":"paths","images":3,"listing":7,"scoped":true,"rows":[]}"#,
            Vec::new(),
        ),
        (
            r#"{"c":"paths","rows":[3,0,1],"listing":7}"#,
            paths(&["a.png", "z.png", "notes.txt"]),
        ),
    ] {
        let reply = ask(request);
        assert_eq!(
            crate::json::field_str(&reply, "t").as_deref(),
            Some("paths")
        );
        assert_eq!(
            crate::json::field_str_array(&reply, "paths"),
            expected,
            "{request}"
        );
    }
    let stale = ask(r#"{"c":"paths","images":3,"listing":6}"#);
    assert_eq!(
        crate::json::field_str(&stale, "where").as_deref(),
        Some("stale")
    );
    assert_eq!(
        crate::json::field_str(&stale, "path").as_deref(),
        Some("paths")
    );
    // The real-backend syscall audit in tests/viewerorder-backend.py verifies that these requests perform no per-file I/O.
}
