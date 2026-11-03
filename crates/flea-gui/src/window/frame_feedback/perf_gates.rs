//! Deterministic painted-metadata work gate.
use super::*;

#[test]
fn m9_perf_cached_frames_reuse_painted_snapshots() {
    let mut context = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    );
    let (shell, cx, _requests) = mount(&mut context, 2000);
    for action in ["key", "scroll", "grid", "columns", "list", "navigate"] {
        sample(&shell, cx, 120, 2000, action, 0, 0);
    }
    scrolling(&shell, cx, 120, 2000, 0, 0);
    loading(&shell, cx);
    let before = snapshot(&shell, cx);
    assert!(
        before[3] > 0,
        "gate requires actual painted feedback metadata"
    );
    for _ in 0..60 {
        callback(cx, Duration::from_nanos(1_000_000_000 / 120));
    }
    let after = snapshot(&shell, cx);
    assert_eq!(
        after[0] - before[0],
        60,
        "gate must exercise real animated Root frames"
    );
    assert_eq!(
        after[1] - before[1],
        0,
        "cached frames must not rebuild Shell"
    );
    assert_eq!(
        after[2] - before[2],
        0,
        "cached frames must not lay out List again"
    );
    assert_eq!(
        after[3] - before[3],
        0,
        "unchanged painted List snapshot must be reused"
    );
    edit(&shell, cx, |shell, window, cx| shell.close(window, cx));
}
