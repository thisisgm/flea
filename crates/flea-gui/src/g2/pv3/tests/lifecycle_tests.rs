//! A player's lifetime: what it holds while no frame is drawn, and what it lets go when it fails, closes or drops.
use super::fake::{picture, Fake, Script};
use crate::g2::pv3::ink::MediaInk;
use crate::g2::pv3::player::{Inhibit, MediaPlayer, Subject};
use crate::g2::pv3::surface::{Caches, Facts, QuickLook};
use crate::list::bench::bench_typeface;
use flea::i18n::I18n;
use flea::ui_logic_pv3::frame::{Event, Failure};
use flea::ui_logic_pv3::player::{Media, Status};
use gpui::{
    div, prelude::*, px, AppContext, Context, Entity, RenderImage, TestAppContext,
    VisualTestContext, Window,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

type Awake = Rc<RefCell<Vec<bool>>>;
/// The host's hold on its player, emptied when it lets go.
type Held = Rc<RefCell<Option<Entity<MediaPlayer>>>>;

fn video() -> Subject {
    Subject {
        path: PathBuf::from("/m/clip.mp4"),
        video: true,
        box_w: 902,
        box_h: 547,
        autoplay: true,
    }
}

fn player(cx: &mut TestAppContext, script: &Rc<Script>) -> (Entity<MediaPlayer>, Awake) {
    let awake: Awake = Rc::default();
    let seen = awake.clone();
    let inhibit: Inhibit = Rc::new(move |on, _| seen.borrow_mut().push(on));
    let backend = Rc::new(Fake(script.clone()));
    let player = cx.new(|cx| {
        let mut player = MediaPlayer::new(backend, Some(inhibit), cx);
        player.mpris_program = "/nonexistent/flea-mpris".into();
        player
    });
    (player, awake)
}

fn playing(cx: &mut TestAppContext) -> (Rc<Script>, Entity<MediaPlayer>, Awake) {
    let script = Rc::new(Script::default());
    let (player, awake) = player(cx, &script);
    player.update(cx, |player, cx| player.open(video(), cx));
    script.sender().event(Event::Loaded {
        video: true,
        duration_ms: 60_000,
    });
    script.sender().event(Event::Playing);
    cx.run_until_parked();
    assert_eq!(*awake.borrow(), [true], "a playing video holds the screen");
    (script, player, awake)
}

// A hidden or occluded window gets no frame callbacks, so nothing may wait on a render to let a picture go.
#[gpui::test]
fn pv3_replaced_frames_stay_bounded_without_a_render(cx: &mut TestAppContext) {
    let (script, _player, _) = playing(cx);
    let pictures: Vec<Arc<RenderImage>> = (0..120_u8).map(picture).collect();
    for image in &pictures {
        script.sender().frame(image.clone());
        cx.run_until_parked();
    }
    let alive = pictures
        .iter()
        .filter(|image| Arc::strong_count(image) > 1)
        .count();
    assert!(alive <= 2, "120 decoded frames with no render left {alive} pictures alive, the shown one and at most one more");
}

// A helper that dies mid-play, a decoder crash or an OOM kill, closes the feed; the player stops holding the screen.
#[gpui::test]
fn pv3_a_helper_that_dies_while_playing_releases_the_inhibitor(cx: &mut TestAppContext) {
    let (script, player, awake) = playing(cx);
    script.sender().closed();
    cx.run_until_parked();
    let status = player.read_with(cx, |player, _| player.status());
    assert_eq!(
        awake.borrow().last(),
        Some(&false),
        "inhibitor calls {:?}, status {status:?}",
        awake.borrow()
    );
    assert_eq!(status, Status::Failed, "a decoder that died cannot play on");
}

// A decoder error after playback started, mpv's END_FILE with an error mid-stream, also lets the screen sleep.
#[gpui::test]
fn pv3_a_failure_while_playing_releases_the_inhibitor(cx: &mut TestAppContext) {
    let (script, player, awake) = playing(cx);
    script.sender().event(Event::Failed(Failure::Unsupported));
    cx.run_until_parked();
    let status = player.read_with(cx, |player, _| player.status());
    assert_eq!(status, Status::Failed);
    assert_eq!(
        awake.borrow().last(),
        Some(&false),
        "inhibitor calls {:?} after a failure",
        awake.borrow()
    );
}

// The oracle's IdleInhibitor is a child of PreviewMedia and dies with it; a released player never leaves it on.
#[gpui::test]
fn pv3_a_released_player_releases_the_inhibitor(cx: &mut TestAppContext) {
    let (script, player, awake) = playing(cx);
    drop(player);
    cx.update(|_| {});
    cx.run_until_parked();
    assert_eq!(script.dropped.get(), 1, "the decoder ended with the player");
    assert_eq!(
        awake.borrow().last(),
        Some(&false),
        "inhibitor calls {:?} after the player was released",
        awake.borrow()
    );
}

// QMediaPlayer::play at EndOfMedia starts over: the helper rewinds, and its Playing takes the player out of Ended.
#[gpui::test]
fn pv3_play_after_the_end_leaves_ended(cx: &mut TestAppContext) {
    let (script, player, _) = playing(cx);
    script.sender().event(Event::Ended);
    cx.run_until_parked();
    let ended = player.read_with(cx, |player, _| {
        (
            player.state.media,
            player.status(),
            player.state.poster(true),
        )
    });
    assert_eq!(ended, (Media::Ended, Status::Stopped, true));
    script.sender().event(Event::Seeked(0));
    script.sender().event(Event::Playing);
    cx.run_until_parked();
    let replayed = player.read_with(cx, |player, _| {
        (player.state.media, player.status(), player.position)
    });
    assert_ne!(replayed.0, Media::Ended, "a replay is no longer at the end");
    assert_eq!((replayed.1, replayed.2), (Status::Playing, 0));
}

/// A host that builds the Quick Look while it holds a player, and nothing once it lets go.
struct Host {
    player: Held,
    caches: Caches,
    catalog: I18n,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = MediaInk::inherited(&window.text_style(), theme);
        let card = div().w(px(902.)).h(px(574.));
        let Some(player) = self.player.borrow().clone() else {
            return card;
        };
        let known = Facts::default();
        let quick = QuickLook {
            player: &player,
            known: &known,
            catalog: &self.catalog,
            ink,
            typeface: bench_typeface(),
            caches: &self.caches,
            strip_shown: false,
            touched: None,
        };
        card.child(quick.build(cx))
    }
}

fn shown(cx: &mut TestAppContext) -> (Rc<Script>, Held, Arc<RenderImage>, &mut VisualTestContext) {
    let (script, player, _) = playing(cx);
    let first = picture(1);
    script.sender().frame(first.clone());
    cx.run_until_parked();
    let held = Rc::new(RefCell::new(Some(player)));
    let host = Host {
        player: held.clone(),
        caches: Caches::default(),
        catalog: crate::test_catalogs::english(),
    };
    let (_, vcx) = cx.add_window_view(|_, _| host);
    vcx.run_until_parked();
    assert!(
        vcx.update(|window, _| window.has_image_atlas_entry(&first)),
        "the painted picture is in the atlas"
    );
    (script, held, first, vcx)
}

fn in_atlas(vcx: &mut VisualTestContext, image: &RenderImage) -> bool {
    vcx.update(|window, _| window.has_image_atlas_entry(image))
}

// The replaced picture leaves the atlas at the next paint, and only the newest one is uploaded.
#[gpui::test]
fn pv3_a_replaced_picture_leaves_the_atlas_at_the_next_paint(cx: &mut TestAppContext) {
    let (script, _held, first, vcx) = shown(cx);
    let second = picture(2);
    script.sender().frame(second.clone());
    vcx.run_until_parked();
    assert!(!in_atlas(vcx, &first), "the replaced picture left");
    assert!(in_atlas(vcx, &second), "the new one drew");
}

// A host that closes the player and stops building the Quick Look still gets the last picture out of the atlas.
#[gpui::test]
fn pv3_a_closed_player_takes_its_last_picture_out_of_the_atlas(cx: &mut TestAppContext) {
    let (_script, held, first, vcx) = shown(cx);
    let player = held.borrow_mut().take().expect("held");
    player.update(vcx, |player, cx| player.close(cx));
    vcx.run_until_parked();
    assert!(
        !in_atlas(vcx, &first),
        "the last picture left the atlas on close"
    );
}

// A host that drops the player without closing it still frees the decoder, the inhibitor and the atlas.
#[gpui::test]
fn pv3_a_dropped_player_takes_its_last_picture_out_of_the_atlas(cx: &mut TestAppContext) {
    let (script, held, first, vcx) = shown(cx);
    drop(held.borrow_mut().take());
    vcx.update(|_, _| {});
    vcx.run_until_parked();
    assert_eq!(script.dropped.get(), 1, "the decoder ended with the player");
    assert!(
        !in_atlas(vcx, &first),
        "the last picture left the atlas on drop"
    );
}

// A host that hides the Quick Look keeps the player and its picture, but gives the atlas back until the next paint.
#[gpui::test]
fn pv3_a_hidden_player_gives_the_atlas_back(cx: &mut TestAppContext) {
    let (_script, held, first, vcx) = shown(cx);
    let player = held.borrow_mut().take().expect("held");
    player.update(vcx, |player, cx| player.hide(cx));
    vcx.run_until_parked();
    assert!(!in_atlas(vcx, &first), "a hidden picture left the atlas");
    let kept = player.read_with(vcx, |player, _| player.frame.as_ref().map(|image| image.id));
    assert_eq!(
        kept,
        Some(first.id),
        "the player still holds it to show again"
    );
}
