use super::*;
use crate::window::navigate::Step;
use std::sync::atomic::Ordering;

fn choose(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.step(0, Step::SortNext, window, cx), Ok(true));
        })
    });
}

fn finish_writes(shell: &Entity<Shell>, cx: &VisualTestContext) {
    shell.read_with(cx, |shell, _| {
        let writer = shell.panes[0].as_ref().unwrap().nav.writer.borrow();
        writer.done().store(writer.sent, Ordering::Release);
    });
}

fn order(shell: &Entity<Shell>, cx: &VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().nav.sort_mark()
    })
}

#[gpui::test]
fn ey476_explicit_sort_survives_other_window_after_completed_write(cx: &mut TestAppContext) {
    let (backend_a, mut requests_a) = Backend::test_lines();
    let (a, a_cx) = mount(backend_a, cx, true);
    commands(&mut requests_a);
    choose(&a, a_cx);
    land(&a, a_cx, "/work", 4, &["Empty", "A"]);
    finish_writes(&a, a_cx);
    let own = saved_sort(&a, a_cx);
    loaded(&a, a_cx, own);
    assert_eq!(order(&a, a_cx), "size:asc");
    let (backend_b, mut requests_b) = Backend::test_lines();
    let (b, b_cx) = mount(backend_b, cx, true);
    commands(&mut requests_b);
    for (listing, expected) in [(4, "size:asc"), (5, "mtime:asc")] {
        choose(&b, b_cx);
        assert_eq!(order(&b, b_cx), expected, "B applies its own key at once");
        land(&b, b_cx, "/work", listing, &["A", "Empty"]);
    }
    finish_writes(&b, b_cx);
    let latest = saved_sort(&b, b_cx);
    for delay in [500, 2500, 2500] {
        b_cx.executor().advance_clock(Duration::from_millis(delay));
        loaded(&a, b_cx, latest.clone());
        assert_eq!(order(&a, b_cx), "size:asc", "B must not replace A's choice");
        let sent = commands(&mut requests_a);
        for line in sent
            .iter()
            .filter(|line| line.get("c").and_then(Json::as_str) == Some("list"))
        {
            assert_eq!(line.get("by").and_then(Json::as_str), Some("size"));
        }
        a.read_with(b_cx, |shell, _| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().nav.drawn_order().key,
                "size"
            );
        });
        land(&a, b_cx, "/work", 5, &["Empty", "A"]);
    }
    b_cx.update(|window, cx| {
        a.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(0, Step::Open("/other".into()), window, cx),
                Ok(true)
            );
        })
    });
    land(&a, b_cx, "/other", 6, &["A"]);
    commands(&mut requests_a);
    b_cx.update(|window, cx| {
        a.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(0, Step::Open("/work".into()), window, cx),
                Ok(true)
            );
        })
    });
    let sent = commands(&mut requests_a);
    let list = sent
        .iter()
        .find(|line| line.get("c").and_then(Json::as_str) == Some("list"))
        .unwrap();
    assert_eq!(
        list.get("by").and_then(Json::as_str),
        Some("mtime"),
        "a subsequent open uses B's last explicit sort"
    );
}

#[gpui::test]
fn ey476_implicit_sort_still_follows_shared_folder_preference(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let (shell, cx) = mount(backend, cx, true);
    commands(&mut requests);
    loaded(
        &shell,
        cx,
        flea::uistate::from_file(r#"{"folderSorts":{"/work":{"key":"date","reverse":true}}}"#),
    );
    let sent = commands(&mut requests);
    let list = sent
        .iter()
        .find(|line| line.get("c").and_then(Json::as_str) == Some("list"))
        .unwrap();
    assert_eq!(list.get("by").and_then(Json::as_str), Some("mtime"));
    assert_eq!(list.get("desc").and_then(Json::as_bool), Some(true));
}
