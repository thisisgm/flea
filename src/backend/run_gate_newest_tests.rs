// The newest list taking the place of a held scan, driven through the same Loop as run_gate_tests.rs.
use super::*;

// Sample: list A held on a stalled share, then list B; B answers while A is still held, and A's late answer says nothing.
#[test]
fn a_newer_list_takes_the_place_of_a_held_plain_scan_and_the_late_answer_says_nothing() {
    let mut l = Loop::new("rungate-newest");
    let (here, held, newer) = (l.dir.dir("here"), l.dir.dir("held"), l.dir.dir("newer"));
    let kept = l.dir.file("here/h.txt", "h");
    l.dir.file("held/a.txt", "a");
    l.dir.file("newer/b.txt", "b");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    assert!(l.request(&list(&here)) == Control::Continue);
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    assert!(l.said().contains(r#""t":"listed""#), "the first folder lands before the held one is asked for");
    let hold = scanhold::hold(&text(&held), Stage::Read);
    assert!(l.request(&list(&held)) == Control::Continue);
    hold.entered.recv_timeout(EVENT_DEADLINE).expect("the held folder's scan is out and held at its read");
    assert!(l.request(r#"{"c":"paths","rows":[0]}"#) == Control::Continue);
    assert!(l.said().is_empty(), "the request waits while the scan is out");
    assert!(l.request(&list(&newer)) == Control::Continue);
    assert!(l.gate.holds() && !hold.over(), "the newer list's own scan is out, and the held one still is not over");
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    assert!(!hold.over(), "the newer folder answered while the held scan was still held");
    let said = l.said();
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 3, "the waiting request, then listed and rows for the newer folder: {said}");
    assert_eq!(lines[0], paths_line(&[text(&kept)]), "the waiting request replays first, against the listing still current");
    assert_eq!(field_str(lines[1], "path"), Some(text(&newer)), "the newer folder became the listing: {said}");
    assert_eq!(field_str(lines[2], "t").as_deref(), Some("rows"), "{said}");
    hold.release();
    // The scan let go hands its watch back and answers nothing, so no Event::List follows it.
    let late = l.wait(|event| matches!(event, Event::AbandonWatch(_)));
    assert!(l.feed(late) == Control::Continue);
    assert!(l.rx.try_recv().map_or(true, |event| !is_list(&event)), "no answer follows the let-go scan");
    assert!(l.said().is_empty() && l.st.base == newer, "the held scan's late answer says nothing and the listing stays the newer folder");
}

// A list for the folder already out, and a scan that has answered, wait their turn as before.
#[test]
fn a_list_for_the_same_folder_or_behind_an_answered_scan_waits_for_the_scan() {
    let mut l = Loop::new("rungate-waits");
    let (a, b) = (l.dir.dir("a"), l.dir.dir("b"));
    l.dir.file("a/a.txt", "a");
    l.dir.file("b/b.txt", "b");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    let hold = scanhold::hold(&text(&a), Stage::Read);
    assert!(l.request(&list(&a)) == Control::Continue);
    hold.entered.recv_timeout(EVENT_DEADLINE).expect("the scan is out and held at its read");
    assert!(l.request(&list(&a)) == Control::Continue);
    assert!(l.gate.holds() && !hold.over() && l.said().is_empty(), "the same folder waits behind the held scan");
    hold.release();
    let landed = l.wait(is_list);
    // The hold ends with the first scan, so the replayed one for the same folder reads freely.
    drop(hold);
    assert!(l.feed(landed) == Control::Continue);
    // The replayed list scans again; it has answered before the next list arrives, so that one waits for it.
    let answered = l.wait(is_list);
    assert!(l.request(&list(&b)) == Control::Continue);
    assert!(l.feed(answered) == Control::Continue);
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    let said = l.said();
    let listed: Vec<Option<String>> = said.lines().filter(|line| field_str(line, "t").as_deref() == Some("listed")).map(|line| field_str(line, "path")).collect();
    assert_eq!(listed, vec![Some(text(&a)), Some(text(&a)), Some(text(&b))], "each list answered in order: {said}");
}

// A search session's folder scan keeps today's order: a list behind it waits for the session's answer.
#[test]
fn a_list_behind_a_search_sessions_scan_waits_for_its_answer() {
    let mut l = Loop::new("rungate-session");
    let (folder, other) = (l.dir.dir("share"), l.dir.dir("other"));
    l.dir.file("share/s.txt", "s");
    l.dir.file("other/o.txt", "o");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    let named = crate::json::escape(&text(&folder));
    assert!(l.request(&format!(r#"{{"c":"searchopen","path":"{named}","folder":"{named}"}}"#)) == Control::Continue);
    let walked = l.landing();
    assert!(l.feed(walked) == Control::Continue);
    l.said();
    let hold = scanhold::hold(&text(&folder), Stage::Read);
    assert!(l.request(r#"{"c":"searchquery","query":"","gen":1,"count":10}"#) == Control::Continue);
    hold.entered.recv_timeout(EVENT_DEADLINE).expect("the folder scan reached its read");
    assert!(l.request(&list(&other)) == Control::Continue);
    assert!(!hold.over() && l.said().is_empty(), "the list waits and the session's scan is not let go");
    hold.release();
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    let said = l.said();
    let listed: Vec<Option<String>> = said.lines().filter(|line| field_str(line, "t").as_deref() == Some("listed")).map(|line| field_str(line, "path")).collect();
    assert_eq!(listed, vec![Some(text(&folder)), Some(text(&other))], "the session's folder answers first, then the list: {said}");
}
