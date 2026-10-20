use super::english;
use crate::ui_logic_fb1::empty::{Rotation, BEAT_LIMIT, PHRASES, ROTATE_MS};
use crate::ui_logic_fb1::{CLOSE_MS, OPEN_MS};

fn rest(hero: &mut Rotation, now: u64, index: usize) {
    hero.advance(now);
    let frame = hero.frame(now);
    assert_eq!(
        (
            frame.phrase,
            frame.caption,
            hero.mark_settled(),
            hero.at_rest()
        ),
        (index, 1., true, true)
    );
}

fn at(hero: &mut Rotation, now: u64) -> usize {
    hero.advance(now);
    hero.index()
}

/// tests/emptybeats.qml's plan on a test clock: each step is ms after the last show or hide.
#[test]
fn three_beats_per_appearance_then_rest_on_the_third() {
    let mut hero = Rotation::default();
    hero.show(0, true);
    assert_eq!(at(&mut hero, 2000), 0);
    assert_eq!(at(&mut hero, 9300), 3);
    rest(&mut hero, 12500, 3);
    rest(&mut hero, 14600, 3);
    hero.hide();
    let t = 14600 + 3500;
    assert_eq!(at(&mut hero, t), 3);
    hero.show(t, true);
    assert_eq!(at(&mut hero, t + 2000), 3);
    assert_eq!(at(&mut hero, t + 9300), 6);
    rest(&mut hero, t + 12500, 6);
    hero.hide();
    let t = t + 12500 + 500;
    hero.show(t, true);
    assert_eq!(at(&mut hero, t + 3500), 7 % PHRASES.len());
    let hidden = hero.index();
    hero.hide();
    let t = t + 3500 + 6500;
    assert_eq!(at(&mut hero, t), hidden);
    hero.show(t, true);
    assert_eq!(at(&mut hero, t + 2000), hidden);
    assert_eq!(at(&mut hero, t + 9300), (hidden + 3) % PHRASES.len());
    rest(&mut hero, t + 12500, (hidden + 3) % PHRASES.len());
}

#[test]
fn a_beat_fades_out_swaps_then_fades_in_and_replays_the_mark() {
    let mut hero = Rotation::default();
    hero.show(0, true);
    hero.advance(ROTATE_MS);
    assert_eq!(
        (hero.beats(), hero.index(), hero.frame(ROTATE_MS).caption),
        (1, 0, 1.)
    );
    hero.advance(ROTATE_MS + 70);
    assert!(hero.frame(ROTATE_MS + 70).caption < 1.);
    assert_eq!(hero.index(), 0);
    hero.advance(ROTATE_MS + CLOSE_MS);
    assert_eq!(
        (hero.index(), hero.frame(ROTATE_MS + CLOSE_MS).caption),
        (1, 0.)
    );
    let done = ROTATE_MS + CLOSE_MS + OPEN_MS;
    hero.advance(done);
    assert_eq!(hero.frame(done).caption, 1.);
    let mark = hero.frame(ROTATE_MS + CLOSE_MS + 200);
    assert_eq!((mark.mark_opacity, mark.mark_drawn), (1., 0.));
    assert!(hero.animating());
}

#[test]
fn the_entrance_rises_ten_pixels_on_out_cubic_unless_held_or_reduced() {
    let mut hero = Rotation::default();
    hero.show(0, true);
    let first = hero.frame(0);
    assert_eq!((first.opacity, first.offset), (0., 10.));
    hero.advance(OPEN_MS);
    let landed = hero.frame(OPEN_MS);
    assert_eq!((landed.opacity, landed.offset), (1., 0.));
    let mut held = Rotation::default();
    held.show(0, false);
    assert_eq!(held.frame(0).opacity, 1.);
}

#[test]
fn reduced_motion_rests_at_once_and_wakes_no_timer() {
    let mut hero = Rotation::default();
    hero.set_reduced(true, 0);
    hero.show(0, true);
    assert!(hero.at_rest() && !hero.animating());
    assert_eq!(hero.next_deadline(), None);
    assert_eq!(at(&mut hero, 20 * ROTATE_MS), 0);
    let frame = hero.frame(0);
    assert_eq!(
        (frame.opacity, frame.caption, frame.mark_drawn),
        (1., 1., 1.)
    );
    let mut mid = Rotation::default();
    mid.show(0, true);
    mid.advance(ROTATE_MS + 10);
    mid.set_reduced(true, ROTATE_MS + 10);
    // EmptyState.qml's fade.stop() halts the sequence before its index swap.
    assert_eq!((mid.index(), mid.frame(ROTATE_MS + 10).caption), (0, 1.));
}

/// ColumnsArea.qml lands a data-held empty folder with animateEntrance off and markItem.settle(): mark and caption whole at once.
#[test]
fn a_held_landing_draws_the_mark_whole_and_still_counts_its_beats() {
    let mut held = Rotation::default();
    held.show(0, false);
    for at in [0, CLOSE_MS + 200, 1000] {
        held.advance(at);
        let frame = held.frame(at);
        assert_eq!(
            (frame.opacity, frame.mark_opacity, frame.mark_drawn),
            (1., 1., 1.),
            "at {at} ms"
        );
    }
    assert!(held.mark_settled() && !held.animating());
    assert_eq!(held.next_deadline(), Some(ROTATE_MS));
    held.advance(ROTATE_MS);
    assert!(
        !held.mark_settled(),
        "a beat replays the mark as the oracle's timer does"
    );
}

/// The oracle's Timer restarts one 2800 ms interval when reduced motion turns off.
#[test]
fn motion_back_on_waits_one_interval_for_the_next_beat() {
    let mut hero = Rotation::default();
    hero.show(0, true);
    hero.advance(ROTATE_MS + 1000);
    hero.set_reduced(true, 4000);
    hero.set_reduced(false, 5000);
    assert_eq!(
        (hero.beats(), hero.next_deadline()),
        (1, Some(5000 + ROTATE_MS))
    );
    hero.advance(5000 + ROTATE_MS);
    assert_eq!(hero.beats(), 2);
}

#[test]
fn between_beats_a_timer_wakes_the_hero_and_at_rest_nothing_does() {
    let mut hero = Rotation::default();
    hero.show(0, true);
    hero.advance(5000);
    assert!(!hero.animating());
    assert_eq!(hero.next_deadline(), Some(2 * ROTATE_MS));
    hero.advance(30000);
    assert_eq!(
        (hero.beats(), hero.next_deadline(), hero.animating()),
        (BEAT_LIMIT, None, false)
    );
    hero.hide();
    assert_eq!(hero.next_deadline(), None);
}

#[test]
fn the_phrases_are_the_oem_seven_in_sentence_case() {
    let catalog = english();
    let words: Vec<_> = PHRASES.iter().map(|key| catalog.tr(key)).collect();
    assert_eq!(
        words,
        [
            "Nothing here yet",
            "A very tidy folder",
            "Not a file in sight",
            "Quiet in here",
            "No clutter to report",
            "Waiting for something to land",
            "Empty, and that is fine",
        ]
    );
}

/// The swap lands with the caption out and the mark undrawn; a new appearance or a fade cut by reduced motion has not landed.
#[test]
fn a_beat_lands_only_at_its_swap_while_nothing_is_drawn() {
    let mut hero = Rotation::default();
    hero.show(0, true);
    hero.advance(ROTATE_MS + CLOSE_MS - 1);
    assert_eq!((hero.beats(), hero.landed()), (1, false));
    let swap = ROTATE_MS + CLOSE_MS;
    hero.advance(swap);
    let frame = hero.frame(swap);
    assert_eq!(
        (hero.landed(), frame.phrase, frame.caption, frame.mark_drawn),
        (true, 1, 0., 0.)
    );
    hero.hide();
    hero.show(10_000, true);
    assert!(!hero.landed());
    hero.advance(10_000 + ROTATE_MS + 1);
    hero.set_reduced(true, 10_000 + ROTATE_MS + 1);
    hero.advance(10_000 + ROTATE_MS + CLOSE_MS + OPEN_MS);
    assert!(!hero.landed());
}
