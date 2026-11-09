//! A primary click on a video's picture toggles play; a drag, a second press, a right click and the strip never do.
use super::fake::{picture, Fake, Script};
use crate::g2::media_preview::ink::{size, MediaInk};
use crate::g2::media_preview::player::{MediaPlayer, Subject};
use crate::g2::media_preview::surface::{Caches, Facts, QuickLook};
use crate::list::bench::bench_typeface;
use flea::i18n::I18n;
use flea::ui_logic_media_preview::frame::{Command, Event};
use gpui::{
    div, point, prelude::*, px, AppContext, Context, Entity, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext, VisualTestContext, Window,
};
use std::path::PathBuf;
use std::rc::Rc;

const CARD: (f32, f32) = (902., 574.);

struct Host {
    player: Entity<MediaPlayer>,
    caches: Caches,
    catalog: I18n,
    frame_click: bool,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let known = Facts::default();
        let quick = QuickLook {
            player: &self.player,
            known: &known,
            catalog: &self.catalog,
            ink: MediaInk::inherited(&window.text_style(), theme),
            typeface: bench_typeface(),
            caches: &self.caches,
            strip_shown: true,
            touched: None,
            frame_click: self.frame_click,
        };
        div().w(px(CARD.0)).h(px(CARD.1)).child(quick.build(cx))
    }
}

/// A playing subject drawn in a Quick Look card, its decoder's opening commands already taken.
fn card(
    cx: &mut TestAppContext,
    video: bool,
    frame_click: bool,
) -> (Rc<Script>, Caches, &mut VisualTestContext) {
    let script = Rc::new(Script::default());
    let backend = Rc::new(Fake(script.clone()));
    let player = cx.new(|cx| {
        let mut player = MediaPlayer::new(backend, None, cx);
        player.mpris_program = "/nonexistent/flea-mpris".into();
        player
    });
    let name = if video { "clip.mp4" } else { "song.ogg" };
    let subject = Subject {
        path: PathBuf::from("/m").join(name),
        video,
        box_w: 902,
        box_h: 528,
        autoplay: true,
    };
    player.update(cx, |player, cx| player.open(subject, cx));
    script.sender().event(Event::Loaded {
        video,
        duration_ms: 60_000,
    });
    script.sender().event(Event::Playing);
    script.sender().frame(picture(1));
    cx.run_until_parked();
    let caches = Caches::default();
    let host = Host {
        player,
        caches: caches.clone(),
        catalog: crate::test_catalogs::english(),
        frame_click,
    };
    let (_, vcx) = cx.add_window_view(|_, _| host);
    vcx.run_until_parked();
    script.take_commands();
    (script, caches, vcx)
}

fn middle() -> Point<Pixels> {
    point(px(CARD.0 / 2.), px((CARD.1 - size::STRIP) / 2.))
}

fn press(vcx: &mut VisualTestContext, at: Point<Pixels>, clicks: usize) {
    vcx.simulate_event(MouseDownEvent {
        position: at,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: clicks,
        first_mouse: false,
    });
    vcx.simulate_event(MouseUpEvent {
        position: at,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: clicks,
    });
}

#[gpui::test]
fn a_click_on_the_playing_picture_pauses_and_the_next_one_plays(cx: &mut TestAppContext) {
    let (script, _caches, vcx) = card(cx, true, true);
    vcx.simulate_click(middle(), Modifiers::none());
    assert_eq!(
        script.take_commands(),
        [Command::Pause],
        "the first click pauses"
    );
    script.sender().event(Event::Paused);
    vcx.run_until_parked();
    vcx.simulate_click(middle(), Modifiers::none());
    assert_eq!(
        script.take_commands(),
        [Command::Play],
        "the second click plays"
    );
}

#[gpui::test]
fn a_double_click_on_the_picture_toggles_once(cx: &mut TestAppContext) {
    let (script, _caches, vcx) = card(cx, true, true);
    press(vcx, middle(), 1);
    press(vcx, middle(), 2);
    assert_eq!(
        script.take_commands(),
        [Command::Pause],
        "the double-click's second press toggles nothing"
    );
}

#[gpui::test]
fn a_drag_or_a_secondary_click_on_the_picture_toggles_nothing(cx: &mut TestAppContext) {
    let (script, _caches, vcx) = card(cx, true, true);
    let (from, to) = (middle(), middle() + point(px(40.), px(12.)));
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    vcx.simulate_mouse_down(middle(), MouseButton::Right, Modifiers::none());
    vcx.simulate_mouse_up(middle(), MouseButton::Right, Modifiers::none());
    assert!(
        script.take_commands().is_empty(),
        "a drag and a right click are not clicks"
    );
}

#[gpui::test]
fn a_press_released_within_two_pixels_toggles_and_one_past_them_does_not(cx: &mut TestAppContext) {
    let (script, _caches, vcx) = card(cx, true, true);
    let moved = |vcx: &mut VisualTestContext, by: f64| {
        let to = middle() + point(px(by as f32), px(0.));
        vcx.simulate_mouse_down(middle(), MouseButton::Left, Modifiers::none());
        vcx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    };
    moved(vcx, 2.);
    assert_eq!(
        script.take_commands(),
        [Command::Pause],
        "a press released exactly at gpui's 2 px drag threshold is a click"
    );
    moved(vcx, 3.);
    assert!(
        script.take_commands().is_empty(),
        "a press released 3 px away is a drag"
    );
}

#[gpui::test]
fn the_strips_play_toggles_once_and_its_band_is_not_the_picture(cx: &mut TestAppContext) {
    let (script, caches, vcx) = card(cx, true, true);
    let play = caches
        .strip
        .borrow()
        .play
        .expect("the strip drew its play mark");
    vcx.simulate_click(play.center(), Modifiers::none());
    assert_eq!(
        script.take_commands(),
        [Command::Pause],
        "one toggle, the strip's own"
    );
    let band = point(px(CARD.0 / 2.), px(CARD.1 - size::STRIP / 2.));
    vcx.simulate_click(band, Modifiers::none());
    let sent = script.take_commands();
    assert!(
        !sent.contains(&Command::Play) && !sent.contains(&Command::Pause),
        "the strip's band keeps its own meaning: {sent:?}"
    );
}

#[gpui::test]
fn an_audio_pane_or_a_revealing_card_takes_no_frame_click(cx: &mut TestAppContext) {
    let (script, _caches, vcx) = card(cx, false, true);
    vcx.simulate_click(middle(), Modifiers::none());
    assert!(
        script.take_commands().is_empty(),
        "audio has no picture to click"
    );
    let (script, _caches, vcx) = card(cx, true, false);
    vcx.simulate_click(middle(), Modifiers::none());
    assert!(
        script.take_commands().is_empty(),
        "a card still revealing takes no click"
    );
}
