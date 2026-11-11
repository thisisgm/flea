use super::*;

#[test]
fn trash_image_rows_never_ask_for_a_thumbnail_after_the_settle() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("a.png", Some("/fixture/a.png"))]);
    let asks = || {
        sent(&calls)
            .iter()
            .filter(|line| line.contains(r#""c":"trashthumb""#))
            .count()
    };
    cx.executor().advance_clock(Duration::from_millis(70));
    cx.run_until_parked();
    assert_eq!(
        asks(),
        0,
        "v0.3.12 draws the generic icon, so no Trash row asks"
    );
    let _ = window;
}
