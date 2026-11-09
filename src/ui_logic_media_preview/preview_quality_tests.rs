use super::{frame::Command, picture::decode_size};
#[test]
fn preview_quality_video_rounds_up_to_cover_its_physical_fit() {
    for scale in [1., 1.25, 1.5, 2.] {
        let w = (901.2_f64 * scale).ceil() as u32;
        let h = (547.2_f64 * scale).ceil() as u32;
        let got = decode_size(3840, 2160, w, h);
        let ratio = (w as f64 / 3840.).min(h as f64 / 2160.);
        assert!(
            got.0 >= (3840. * ratio).ceil() as u32 && got.1 >= (2160. * ratio).ceil() as u32,
            "{got:?} must cover scale {scale}"
        );
    }
}
#[test]
fn preview_quality_video_resize_command_is_bounded_and_round_trips() {
    assert_eq!(
        Command::parse("resize 1128 684").map(|command| command.render()),
        Some("resize 1128 684".into())
    );
    for bad in [
        "resize 0 10",
        "resize 10 0",
        "resize 9999999 2",
        "resize 2 9999999",
        "resize -1 4",
        "resize 2 3 trailing",
    ] {
        assert!(Command::parse(bad).is_none(), "{bad}");
    }
}
