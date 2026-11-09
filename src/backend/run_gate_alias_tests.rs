// A let-go scan's late descriptor and a newer scan of the same folder, driven through the loop's own dispatch.
use super::*;

// Sample: list A held, list B held, list A again; A's first scan hands its descriptor back while the second A is out, and inotify gave both the same one.
#[test]
fn a_let_go_scans_late_abandon_leaves_the_watch_a_newer_scan_of_the_same_folder_armed() {
    let mut l = Loop::new("rungate-alias");
    let (here, a, b) = (l.dir.dir("here"), l.dir.dir("a"), l.dir.dir("b"));
    l.dir.file("here/h.txt", "h");
    l.dir.file("a/a.txt", "a");
    l.dir.file("b/b.txt", "b");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    assert!(l.request(&list(&here)) == Control::Continue);
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    let (hold_a, hold_b) = (scanhold::hold(&text(&a), Stage::Read), scanhold::hold(&text(&b), Stage::Read));
    assert!(l.request(&list(&a)) == Control::Continue);
    hold_a.entered.recv_timeout(EVENT_DEADLINE).expect("the first scan of A armed its watch and is held at its read");
    assert!(l.request(&list(&b)) == Control::Continue);
    hold_b.entered.recv_timeout(EVENT_DEADLINE).expect("B's scan is out and held, A's was let go");
    // Dropping a hold lets its scan go on and frees the path, so the second scan of A reads without waiting.
    drop(hold_a);
    assert!(l.request(&list(&a)) == Control::Continue);
    drop(hold_b);
    // The late descriptors and the second A's answer are fed in the order that matters: both abandons while A's scan is still out, then the answer.
    let (mut abandons, mut answer) = (Vec::new(), None);
    while abandons.len() < 2 || answer.is_none() {
        match l.rx.recv_timeout(EVENT_DEADLINE).expect("the let-go scans and the second A answer on the loop's channel") {
            late @ Event::AbandonWatch(_) => abandons.push(late),
            done @ Event::List(_) => answer = Some(done),
            _ => {}
        }
    }
    for late in abandons {
        assert!(l.feed(late) == Control::Continue);
    }
    assert!(l.gate.holds(), "the second A is still out while its predecessors' descriptors arrive");
    assert!(l.feed(answer.expect("the second A answered")) == Control::Continue);
    assert!(l.st.base == a, "the second A is the listing");
    // A removed watch's own IN_IGNORED also arrives as a burst, so what is already queued is drained before the folder changes.
    std::thread::sleep(Duration::from_millis(300));
    while l.rx.try_recv().is_ok() {}
    std::fs::write(a.join("new.txt"), "n").expect("the sandbox takes a file");
    let mut live = false;
    while !live {
        let Ok(event) = l.rx.recv_timeout(Duration::from_secs(3)) else { break };
        if let Event::Changed(wd) = event {
            live = l.watch.is_current(wd);
        }
    }
    assert!(live, "A's watch is alive after the first A's late abandon");
}

// Sample: list B held, list C held (B let go), B's late descriptor parked behind C, then a local list D lets C go and answers inline with no Event::List.
#[test]
fn a_local_list_that_lets_a_scan_go_releases_the_late_descriptors_parked_behind_it() {
    let mut l = Loop::new("rungate-parked");
    let local = TestDir::new("rungate-parked-local");
    let (here, b, c) = (l.dir.dir("here"), l.dir.dir("b"), l.dir.dir("c"));
    let d = local.dir("d");
    l.dir.file("here/h.txt", "h");
    l.dir.file("b/b.txt", "b");
    l.dir.file("c/c.txt", "c");
    local.file("d/d.txt", "d");
    let _body = crate::backend::iomount::test_hold_body(remote_body(l.dir.path()));
    assert!(l.request(&list(&here)) == Control::Continue);
    let landed = l.wait(is_list);
    assert!(l.feed(landed) == Control::Continue);
    let (hold_b, hold_c) = (scanhold::hold(&text(&b), Stage::Read), scanhold::hold(&text(&c), Stage::Read));
    assert!(l.request(&list(&b)) == Control::Continue);
    hold_b.entered.recv_timeout(EVENT_DEADLINE).expect("B's scan armed its watch and is held at its read");
    assert!(l.request(&list(&c)) == Control::Continue);
    hold_c.entered.recv_timeout(EVENT_DEADLINE).expect("C's scan is out and held, B's was let go");
    drop(hold_b);
    let late = l.wait(|event| matches!(event, Event::AbandonWatch(_)));
    let Event::AbandonWatch(parked) = late else { unreachable!("the wait returned an abandon") };
    assert!(l.feed(late) == Control::Continue);
    assert!(l.gate.holds(), "C is still out while B's descriptor arrives, so it waits");
    assert!(l.request(&list(&d)) == Control::Continue);
    assert!(!l.gate.holds() && l.st.base == d, "the local list let C go and answered inline");
    assert!(l.gate.release_late().is_empty(), "no scan is out, so B's descriptor was released with the let-go and nothing is left parked");
    // Removing a watch makes the kernel report IN_IGNORED for its descriptor, which the pump forwards as Vanished.
    while !matches!(l.rx.recv_timeout(EVENT_DEADLINE).expect("B's removed watch reports IN_IGNORED on the loop's channel"), Event::Vanished(wd) if wd == parked) {}
    drop(hold_c);
}
