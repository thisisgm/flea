//! 0401-empty-stock, -omi-off, -fixed and -both-off: Settings > View's two rows always win and each turns off alone.
use super::hero::EmptyLook;
use super::hero_tests::{frame, open, read, run, FRAME};
use flea::ui_logic_status_feedback::empty::ROTATE_MS;
use gpui::{px, size, TestAppContext, VisualTestContext};

fn look(cx: &mut TestAppContext, omi: bool, rotate: bool) {
    let data = flea::jsondoc::parse(&format!(
        r#"{{"emptyFolderOmi":{omi},"emptyFolderRotate":{rotate}}}"#
    ))
    .unwrap();
    cx.update(|cx| EmptyLook::apply(&data, cx));
    cx.run_until_parked();
}

fn drawn(
    cx: &mut TestAppContext,
    handle: gpui::WindowHandle<super::hero::EmptyHero>,
    part: &'static str,
) -> bool {
    VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds(part)
        .is_some()
}

fn caption(cx: &mut TestAppContext, handle: gpui::WindowHandle<super::hero::EmptyHero>) -> String {
    read(cx, handle, |hero| {
        hero.painted
            .get("caption")
            .map(|text| text.text.to_string())
    })
    .unwrap_or_default()
}

#[gpui::test]
fn an_absent_key_reads_ticked_so_a_fresh_install_draws_041s_empty_folder(cx: &mut TestAppContext) {
    let data = flea::jsondoc::parse("{}").unwrap();
    assert_eq!(
        EmptyLook::read(&data),
        EmptyLook {
            omi: true,
            rotate: true
        }
    );
    let handle = open(cx);
    run(cx, handle, ROTATE_MS + 2 * FRAME);
    assert!(drawn(cx, handle, "fb1-hero-mark"));
    assert!(read(cx, handle, |hero| hero.omi.shown()));
    assert_eq!(
        read(cx, handle, |hero| (
            hero.phrases().len(),
            hero.rotation().beats()
        )),
        (4, 1)
    );
}

#[gpui::test]
fn omi_unticked_draws_no_mark_and_the_headline_alone_stays_centred(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    look(cx, false, true);
    frame(cx, handle);
    frame(cx, handle);
    assert!(
        !drawn(cx, handle, "fb1-hero-mark"),
        "no mark, not even a still one"
    );
    assert!(!read(cx, handle, |hero| hero.omi.shown()));
    let bounds = super::hero_tests::bounds(cx, handle, &["fb1-hero-caption"])[0];
    let scale = handle
        .update(cx, |_, window, _| window.scale_factor())
        .unwrap();
    super::state_tests::centred(bounds.center().y, 619. / 2., scale);
    super::state_tests::centred(bounds.center().x, 908. / 2., scale);
    assert_eq!(
        caption(cx, handle),
        "NOTHING HERE YET",
        "reduced motion holds the phrase showing"
    );
}

#[gpui::test]
fn rotate_unticked_holds_empty_folder_with_or_without_reduced_motion(cx: &mut TestAppContext) {
    let handle = open(cx);
    look(cx, true, false);
    for reduced in [false, true, false] {
        cx.update(|cx| cx.set_reduce_motion(reduced));
        run(cx, handle, 2 * ROTATE_MS);
        assert_eq!(caption(cx, handle), "EMPTY FOLDER");
        assert_eq!(read(cx, handle, |hero| hero.rotation().beats()), 0);
        assert_eq!(
            read(cx, handle, |hero| hero.rotation().next_deadline()),
            None
        );
    }
    assert!(drawn(cx, handle, "fb1-hero-mark"), "Omi stays on");
    let timers = read(cx, handle, |hero| hero.counters.timers.get());
    run(cx, handle, 3 * ROTATE_MS);
    assert_eq!(
        read(cx, handle, |hero| hero.counters.timers.get()),
        timers,
        "no beat is ever armed"
    );
}

#[gpui::test]
fn omi_alone_unticked_keeps_the_running_rotation_on_its_beat(cx: &mut TestAppContext) {
    let handle = open(cx);
    run(cx, handle, 2400);
    let due = read(cx, handle, |hero| hero.rotation().next_deadline());
    assert!(due.is_some());
    look(cx, false, true);
    frame(cx, handle);
    assert!(!drawn(cx, handle, "fb1-hero-mark"));
    assert_eq!(
        read(cx, handle, |hero| hero.rotation().next_deadline()),
        due
    );
    run(cx, handle, ROTATE_MS - 2400 + 2 * FRAME);
    assert_eq!(read(cx, handle, |hero| hero.rotation().beats()), 1);
}

#[gpui::test]
fn both_rows_turn_off_and_on_again_without_tying_each_other(cx: &mut TestAppContext) {
    let handle = open(cx);
    run(cx, handle, 2400);
    look(cx, false, false);
    run(cx, handle, ROTATE_MS);
    assert_eq!(caption(cx, handle), "EMPTY FOLDER");
    assert!(!drawn(cx, handle, "fb1-hero-mark"));
    assert_eq!(
        run(cx, handle, 2 * ROTATE_MS),
        0,
        "nothing on the pane asks for a frame"
    );
    look(cx, true, false);
    frame(cx, handle);
    assert!(drawn(cx, handle, "fb1-hero-mark"));
    assert_eq!(caption(cx, handle), "EMPTY FOLDER");
    look(cx, true, true);
    frame(cx, handle);
    assert_eq!(caption(cx, handle), "NOTHING HERE YET");
    let now = read(cx, handle, |hero| hero.rotation().next_deadline());
    assert!(now.is_some(), "the rotation resumes");
    run(cx, handle, ROTATE_MS + 2 * FRAME);
    assert_eq!(read(cx, handle, |hero| hero.rotation().beats()), 1);
}
