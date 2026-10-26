use super::*;
use crate::backend::gitfixture::Fixture;
use crate::backend::gitreq::quote;
use crate::json::{field_bool, field_str, field_usize};

#[test]
fn git_wire_preserves_listing_rows_and_refreshes_after_the_existing_watch() {
    if !Fixture::available() || super::super::sandboxprobe::skipped() {
        return;
    }
    let d = Fixture::new();
    d.repo();
    d.write("tracked", b"original\n");
    d.git(&["add", "--", "."]);
    d.git(&["commit", "--quiet", "-m", "fixture"]);
    let tb = Tables::load();
    let (events, rx) = crate::backend::events::channel();
    let mut st = State::new(super::super::dirsizeworker::Worker::new(events.clone()));
    let pool = Pool::new(
        1,
        channel().0,
        d.path("cache"),
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
    let mut out = Vec::new();
    let path = quote(&d.0.to_string_lossy());
    let mut dispatch_event = |event: Event| {
        assert!(
            dispatch(
                event,
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
        String::from_utf8(std::mem::take(&mut out)).unwrap()
    };
    let listed = dispatch_event(Event::Request(format!(
        "{{\"c\":\"list\",\"path\":{path},\"first\":10}}"
    )));
    assert!(listed.contains("\"t\":\"listed\"") && listed.contains("\"t\":\"rows\""));
    assert!(dispatch_event(Event::Request(format!(
        "{{\"c\":\"gitstatus\",\"path\":{path},\"enabled\":true,\"id\":1}}"
    )))
    .is_empty());
    let deadline = Instant::now() + Duration::from_secs(10);
    let first = loop {
        assert!(Instant::now() < deadline);
        let event = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        let git = matches!(event, Event::Git(_));
        let reply = dispatch_event(event);
        if git {
            break reply;
        }
    };
    assert!(field_bool(&first, "ok"), "{first}");
    assert_eq!(
        field_str(&first, "label").as_deref(),
        Some("main · 0 changed")
    );
    d.write("tracked", b"changed outside Flea\n");
    let deadline = Instant::now() + Duration::from_secs(5);
    let notification = loop {
        let event = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("the directory watch fires");
        let reply = dispatch_event(event);
        if field_str(&reply, "t").as_deref() == Some("changed") {
            break reply;
        }
    };
    assert_eq!(field_str(&notification, "path").as_deref(), d.0.to_str());
    assert!(dispatch_event(Event::Request(format!(
        "{{\"c\":\"gitstatus\",\"path\":{path},\"enabled\":true,\"refresh\":true,\"id\":2}}"
    )))
    .is_empty());
    let deadline = Instant::now() + Duration::from_secs(10);
    let refreshed = loop {
        let event = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        let git = matches!(event, Event::Git(_));
        let reply = dispatch_event(event);
        if git {
            break reply;
        }
    };
    assert_eq!(field_usize(&refreshed, "changed"), Some(1), "{refreshed}");
    assert_eq!(field_usize(&refreshed, "id"), Some(2));
    let window = dispatch_event(Event::Request(
        "{\"c\":\"window\",\"start\":0,\"count\":10}".into(),
    ));
    assert!(
        window.contains("\"n\":\"tracked\"") || window.contains("\"name\":\"tracked\""),
        "{window}"
    );
    assert_eq!(
        st.generation, 1,
        "Git snapshots never renumber the file-operation listing"
    );
}
