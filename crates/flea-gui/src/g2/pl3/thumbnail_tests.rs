use super::*;

#[test]
fn thumbnails_settle_on_the_executor_clock() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("a.png", Some("/fixture/a.png"))]);
    let asks = || {
        sent(&calls)
            .iter()
            .filter(|line| line.contains(r#""c":"trashthumb""#))
            .count()
    };
    assert_eq!(asks(), 0, "nothing asks before the settle");
    cx.executor().advance_clock(Duration::from_millis(69));
    cx.run_until_parked();
    assert_eq!(asks(), 0, "the first settle is 70 ms");
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(asks(), 1, "the settle asks once for the visible image row");
    let _ = window;
}
