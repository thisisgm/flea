use super::english;
use crate::ui_logic_status_feedback::empty::{Rotation, PHRASES, ROTATE_MS};
use crate::ui_logic_status_feedback::{CLOSE_MS, OPEN_MS};

fn rest(hero: &mut Rotation, now: u64, index: usize) {
    hero.advance(now);
    let frame = hero.frame(now);
    assert_eq!(
        (
            frame.phrase,
            frame.caption,
            hero.mark_settled(),
            !hero.animating()
        ),
        (index, 1., true, true)
    );
}

fn at(hero: &mut Rotation, now: u64) -> usize {
    hero.advance(now);
    hero.index()
}

/// Captions loop past the last phrase back to the first for as long as the hero is shown; no beat lands early.
#[test]
fn captions_wrap_from_the_last_phrase_to_the_first_while_shown() {
    let mut hero = Rotation::default();
    hero.show(0, true);
    for beat in 1..=3 * PHRASES.len() {
        let now = beat as u64 * ROTATE_MS;
        assert_eq!(at(&mut hero, now - 1), (beat - 1) % PHRASES.len());
        assert_eq!(hero.beats(), beat - 1, "no beat before {now} ms");
        assert_eq!(at(&mut hero, now), (beat - 1) % PHRASES.len());
        assert_eq!(hero.beats(), beat);
        assert!(hero.animating());
        assert_eq!(at(&mut hero, now + CLOSE_MS), beat % PHRASES.len());
        rest(&mut hero, now + 2400, beat % PHRASES.len());
        assert_eq!(hero.next_deadline(), Some(now + ROTATE_MS));
    }
}

#[test]
fn each_appearance_starts_at_the_first_phrase_even_after_a_cut_fade() {
    let mut hero = Rotation::default();
    hero.show(0, true);
    hero.advance(5 * ROTATE_MS + 10);
    hero.hide();
    assert_eq!(hero.next_deadline(), None);
    assert!(!hero.animating());
    hero.advance(30 * ROTATE_MS);
    for entrance in [false, true] {
        let now = 30 * ROTATE_MS;
        hero.show(now, entrance);
        assert_eq!((hero.index(), hero.beats(), hero.landed()), (0, 0, false));
        assert_eq!(hero.next_deadline(), Some(now + ROTATE_MS));
        assert_eq!(at(&mut hero, now + ROTATE_MS - 1), 0);
        assert_eq!(at(&mut hero, now + ROTATE_MS + CLOSE_MS), 1);
        hero.hide();
    }
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
    assert!(
        !hero.caption_animating(),
        "the caption is whole while the legacy mark finishes drawing"
    );
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
fn between_beats_a_timer_waits_and_hidden_heroes_have_no_deadline() {
    let mut hero = Rotation::default();
    hero.show(0, true);
    hero.advance(5000);
    assert!(!hero.animating());
    assert_eq!(hero.next_deadline(), Some(2 * ROTATE_MS));
    let beat = 10;
    hero.advance(beat * ROTATE_MS + 2400);
    assert_eq!(
        (hero.beats(), hero.next_deadline(), hero.animating()),
        (beat as usize, Some((beat + 1) * ROTATE_MS), false)
    );
    hero.hide();
    assert_eq!(hero.next_deadline(), None);
}

/// EY-249: the beats are the catalog's four pane.empty keys, and the loop runs on past the last one while shown.
#[test]
fn fixchrome_ey249_the_beats_are_the_catalog_keys_and_loop_while_shown() {
    let catalog = english();
    let words: Vec<_> = PHRASES.iter().map(|key| catalog.tr(key)).collect();
    assert_eq!(
        words,
        [
            "NOTHING HERE YET",
            "A VERY TIDY FOLDER",
            "NOT A FILE IN SIGHT",
            "QUIET IN HERE",
        ]
    );
    let mut hero = Rotation::default();
    hero.show(0, true);
    let shown: Vec<_> = (1..=2 * PHRASES.len() as u64)
        .map(|beat| {
            let now = beat * ROTATE_MS + CLOSE_MS;
            hero.advance(now);
            hero.index()
        })
        .collect();
    assert_eq!(shown, [1, 2, 3, 0, 1, 2, 3, 0], "no stop on the last beat");
    assert_eq!(
        hero.next_deadline(),
        Some((2 * PHRASES.len() as u64 + 1) * ROTATE_MS)
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

/// Held: the first phrase, no beat at any motion setting, a fade cut short; released: a full interval later.
#[test]
fn a_held_rotation_never_beats_and_resumes_from_the_first_phrase() {
    let mut hero = Rotation::default();
    hero.show(0, true);
    hero.advance(2 * ROTATE_MS + 10);
    assert!(hero.caption_animating());
    hero.set_held(true, 2 * ROTATE_MS + 10);
    assert_eq!((hero.index(), hero.next_deadline()), (0, None));
    for reduced in [false, true, false] {
        hero.set_reduced(reduced, 3 * ROTATE_MS);
        rest(&mut hero, 40 * ROTATE_MS, 0);
        assert_eq!((hero.beats(), hero.next_deadline()), (0, None));
    }
    hero.hide();
    hero.show(50 * ROTATE_MS, true);
    assert_eq!(hero.next_deadline(), None, "a new appearance stays held");
    hero.set_held(false, 60 * ROTATE_MS);
    assert_eq!(hero.next_deadline(), Some(61 * ROTATE_MS));
    assert_eq!(at(&mut hero, 61 * ROTATE_MS + CLOSE_MS), 1);
}
