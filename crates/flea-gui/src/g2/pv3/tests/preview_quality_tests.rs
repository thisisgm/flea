use super::{
    fake::{picture, Script},
    player_tests::{player, send, subject},
};
use flea::ui_logic_pv3::{
    frame::{Command, Event},
    player::Playback,
};
use gpui::TestAppContext;
use std::{rc::Rc, sync::Arc};
#[gpui::test]
fn preview_quality_video_resize_preserves_decoder_frame_and_pause(cx: &mut TestAppContext) {
    let script = Rc::new(Script::default());
    let (player, _) = player(cx, &script, None);
    player.update(cx, |player, cx| player.open(subject("clip.mp4", true), cx));
    send(
        cx,
        &script,
        Event::Loaded {
            video: true,
            duration_ms: 10_000,
        },
    );
    send(cx, &script, Event::Paused);
    let frame = picture(7);
    script.sender().frame(frame.clone());
    cx.run_until_parked();
    script.take_commands();
    let generation = player.read_with(cx, |player, _| player.generation());
    player.update(cx, |player, _| player.resize(1128, 684));
    assert_eq!(
        script.take_commands(),
        [Command::parse("resize 1128 684").unwrap()]
    );
    player.read_with(cx, |player, _| {
        assert_eq!(player.generation(), generation);
        assert_eq!(player.state.playback, Playback::Paused);
        assert!(Arc::ptr_eq(player.frame.as_ref().unwrap(), &frame));
    });
    assert_eq!(script.requests.borrow().len(), 1);
    assert_eq!(script.dropped.get(), 0);
    player.update(cx, |player, _| player.resize(1128, 684));
    assert!(script.take_commands().is_empty());
}
