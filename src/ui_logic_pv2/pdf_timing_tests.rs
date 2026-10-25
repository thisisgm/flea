use super::*;

// tests/pdf-idle.qml: the fast path, the walk settle, the render gate and the cancelled open.
#[test]
fn pdf_isolated_press_opens_on_the_next_frame() {
    let mut opener = Opener::default();
    opener.request("/d/a.pdf", true, 1000, false);
    assert!(opener.fast_armed, "an isolated press arms the frame path");
    assert_eq!(
        opener.opened, "",
        "the open waits for the first frame after the press"
    );
    assert!(opener.frame(), "the frame opens it");
    assert_eq!(
        (opener.opened.as_str(), opener.fast_opens),
        ("/d/a.pdf", 1),
        "an isolated press opens on the fast path"
    );
    assert!(!opener.tick(5000), "the settle has nothing left to open");
}

#[test]
fn pdf_walk_waits_for_the_settle() {
    let mut opener = Opener::default();
    opener.request("/d/a.pdf", true, 1000, false);
    opener.frame();
    opener.request("/d/c.pdf", true, 2000, false);
    assert!(opener.fast_armed, "the first walk step arms the idle path");
    opener.request("/d/b.pdf", true, 2010, false);
    assert!(
        !opener.fast_armed,
        "the next step inside the settle drops it"
    );
    assert!(!opener.frame(), "a frame opens nothing once disarmed");
    assert!(
        !opener.tick(2010 + DOCUMENT_SETTLE_MS - 1),
        "the settle has not passed"
    );
    assert!(
        opener.tick(2010 + DOCUMENT_SETTLE_MS),
        "a walk press waits for the settle"
    );
    assert_eq!(
        opener.opened, "/d/b.pdf",
        "the walk lands on the second document"
    );
    assert_eq!(
        opener.fast_opens, 1,
        "the walk opens at the settle, never the fast path"
    );
}

#[test]
fn pdf_switch_with_a_live_render_waits_for_the_settle() {
    let mut opener = Opener::default();
    opener.request("/d/heavy.pdf", true, 1000, false);
    opener.frame();
    opener.request("/d/b.pdf", true, 5000, true);
    assert!(
        !opener.fast_armed,
        "a switch while the first render is in flight takes the settle"
    );
    assert!(
        opener.tick(5000 + DOCUMENT_SETTLE_MS),
        "the held switch lands at the settle"
    );
    assert_eq!(opener.fast_opens, 1, "never the fast path");
}

#[test]
fn pdf_cancelled_open_never_lands() {
    let mut opener = Opener::default();
    opener.request("/d/b.pdf", true, 1000, false);
    opener.frame();
    opener.request("/d/c.pdf", true, 9000, false);
    assert!(opener.fast_armed, "the cancel press arms the frame path");
    opener.request("/d/a.pdf", true, 9000, false);
    assert!(
        !opener.fast_armed,
        "a second request in the same turn disarms it"
    );
    assert!(
        !opener.frame(),
        "a frame presented while disarmed opens nothing"
    );
    assert_eq!(
        opener.opened, "/d/b.pdf",
        "the cancelled first open never lands"
    );
    assert!(
        opener.tick(9000 + DOCUMENT_SETTLE_MS),
        "the second request opens at the settle"
    );
    assert_eq!(
        (opener.opened.as_str(), opener.fast_opens),
        ("/d/a.pdf", 1),
        "never the fast path"
    );
}

#[test]
fn pdf_hidden_window_still_opens_at_the_settle() {
    let mut opener = Opener::default();
    opener.request("/d/c.pdf", true, 100_000, false);
    assert!(opener.fast_armed, "the hidden press takes the idle arm");
    assert!(
        opener.tick(100_000 + DOCUMENT_SETTLE_MS),
        "a hidden window still opens at the settle"
    );
    assert_eq!(
        opener.fast_opens, 0,
        "that open waits for the settle, never a frame"
    );
}

#[test]
fn pdf_inactive_surface_opens_nothing() {
    let mut opener = Opener::default();
    opener.request("/d/a.pdf", false, 0, false);
    assert_eq!(
        opener.settle_at, None,
        "a change that opens nothing stamps nothing"
    );
    opener.request("/d/a.pdf", true, 0, false);
    opener.frame();
    opener.request("/d/a.pdf", false, 500, false);
    assert!(
        opener.tick(500 + DOCUMENT_SETTLE_MS),
        "deactivating closes the document at the settle"
    );
    assert_eq!(opener.opened, "", "an inactive surface holds no document");
}

#[test]
fn pdf_due_treats_a_clock_step_back_as_due() {
    assert!(due(200, Some(0), 120), "past the settle is due");
    assert!(due(100, Some(200), 120), "a clock that went back is due");
    assert!(!due(50, Some(0), 120), "inside the settle trails");
    assert!(due(0, None, 120), "the first request is due");
    assert!(
        !open_now(500, None, 120, true),
        "a render in flight is never an idle press"
    );
}

// tests/pdf-first.qml: the turn during a slow render, the cap and the minimum show.
#[test]
fn pdf_slow_turn_falls_back_by_the_cap_and_stands_its_minimum() {
    let mut cap = Cap::new(66, 100);
    cap.render_finished(Some(0), 0);
    cap.page_changed(1, true, 1000);
    assert_eq!(
        cap.drawn_page(1),
        0,
        "the old page stands in while the turn renders"
    );
    assert!(!cap.tick(1065), "inside the cap nothing changes");
    assert!(cap.tick(1066), "the loading state shows at the cap");
    assert_eq!(
        (cap.fell_back, cap.fell_back_at),
        (true, Some(1066)),
        "and is stamped"
    );
    assert_eq!(
        cap.drawn_page(1),
        1,
        "the counter names the page asked for once nothing stands in"
    );
    cap.render_finished(Some(1), 1100);
    assert!(
        cap.fell_back,
        "a page that lands early waits behind the loading state"
    );
    assert_eq!(
        cap.next_deadline(),
        Some(1166),
        "until the minimum has stood"
    );
    assert!(cap.tick(1166), "then the page shows");
    assert_eq!(cap.drawn_page(1), 1, "and it is the page on screen");
}

#[test]
fn pdf_turning_back_onto_the_shown_page_stops_the_cap() {
    let mut cap = Cap::new(66, 100);
    cap.render_finished(Some(2), 0);
    cap.page_changed(3, true, 10);
    cap.page_changed(2, true, 20);
    assert_eq!(cap.cap_at, None, "turning back stops the cap");
    assert!(!cap.tick(500), "and nothing falls back");
    cap.page_changed(4, false, 30);
    assert_eq!(
        cap.cap_at, None,
        "a turn with no render loading arms nothing"
    );
}

#[test]
fn pdf_cap_needs_a_page_on_screen() {
    let mut cap = Cap::new(66, 100);
    cap.page_changed(1, true, 0);
    cap.render_started(1, 0);
    assert_eq!(
        cap.next_deadline(),
        None,
        "with no page shown yet a turn does nothing"
    );
    cap.render_finished(None, 10);
    assert_eq!(cap.shown_page, None, "a failed render shows nothing");
    cap.render_finished(Some(0), 20);
    cap.render_started(0, 30);
    assert_eq!(cap.cap_at, None, "a render of the shown page arms nothing");
    cap.render_started(1, 30);
    assert_eq!(
        cap.cap_at,
        Some(96),
        "a render of another page restarts the cap"
    );
    cap.fell_back = true;
    cap.reset();
    assert_eq!(
        cap,
        Cap::new(66, 100),
        "a document switch drops every state"
    );
}

// tests/pdf-first.qml FETCHLATE and FAILSENTENCE: the fetch decision stands and only the stalled flag says not responding.
#[test]
fn pdf_fetch_decides_once_and_reads_its_reply() {
    assert!(
        !Fetch::decide("", true, true),
        "no document fetches nothing"
    );
    assert!(
        !Fetch::decide("/n/a.pdf", true, false),
        "no backend fetches nothing"
    );
    assert!(
        Fetch::decide("/n/a.pdf", true, true),
        "a hangable document is fetched"
    );
    let mut fetch = Fetch::Waiting(4);
    assert_eq!(
        fetch.source("/n/a.pdf"),
        None,
        "nothing loads while the fetch is out"
    );
    assert!(
        !fetch.copied(3, "/c/old.pdf", "", false),
        "another fetch's reply is dropped"
    );
    assert!(
        fetch.copied(4, "/c/a.pdf", "", false),
        "the fetch in flight takes its reply"
    );
    assert_eq!(
        fetch.source("/n/a.pdf"),
        Some("/c/a.pdf"),
        "the copy loads once it lands"
    );
    assert_eq!(
        Fetch::InPlace.source("/n/a.pdf"),
        Some("/n/a.pdf"),
        "in place while no fetch runs"
    );
    assert_eq!(Fetch::InPlace.source(""), None, "no document opens nothing");
}

#[test]
fn pdf_only_the_stalled_flag_says_not_responding() {
    let mut fetch = Fetch::Waiting(1);
    fetch.copied(1, "", "that file is not responding", false);
    assert_eq!(
        fetch.fail_key(),
        "document.pdf_unreadable",
        "the error text alone is never the sentence"
    );
    let mut fetch = Fetch::Waiting(1);
    fetch.copied(1, "", "no such file", true);
    assert_eq!(
        fetch.fail_key(),
        "document.pdf_not_responding",
        "the stalled flag is"
    );
    assert_eq!(
        Fetch::InPlace.fail_key(),
        "document.pdf_unreadable",
        "a document that fails in place could not be read"
    );
    assert!(
        !fetch.copied(1, "/c/a.pdf", "", false),
        "a second reply to a settled fetch is dropped"
    );
}
