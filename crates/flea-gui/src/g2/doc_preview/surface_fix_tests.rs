//! A failed page is asked for once, held turns and fast scrolls stay bounded, and a page back in view settles the first frame.
use super::tests::{land, opened, renders, LETTER};
use super::*;

#[test]
fn a_failed_single_page_is_not_asked_again() {
    let mut s = opened(false, "/d/a.pdf", 1, vec![LETTER; 3], (600., 600.));
    let width = s.render_width;
    s.take_commands();
    s.reply(
        Reply::RenderFailed {
            doc: 1,
            page: 0,
            width,
        },
        5,
    );
    s.set_frame(600., 600., 1., 9.);
    s.reply(
        Reply::RenderFailed {
            doc: 1,
            page: 9,
            width,
        },
        6,
    );
    let again = renders(&s.take_commands());
    assert!(
        !again.iter().any(|&(page, _)| page == 0),
        "a page that failed stays failed: asked again {again:?}"
    );
}

#[test]
fn a_failed_stack_page_is_not_asked_again() {
    let mut s = opened(true, "/d/a.pdf", 1, vec![LETTER; 3], (600., 2000.));
    s.take_commands();
    s.reply(
        Reply::RenderFailed {
            doc: 1,
            page: 1,
            width: 600,
        },
        5,
    );
    s.scroll_to(1.);
    s.scroll_to(0.);
    let again = renders(&s.take_commands());
    assert!(
        !again.iter().any(|&(page, _)| page == 1),
        "a page that failed stays failed: asked again {again:?}"
    );
}

#[test]
fn held_turns_keep_the_pages_in_flight_bounded() {
    let mut s = opened(false, "/d/a.pdf", 1, vec![LETTER; 400], (600., 600.));
    for at in 0..300 {
        s.turn(1, 10 + at);
    }
    let asked = renders(&s.take_commands());
    assert!(
        s.inflight.len() <= 3,
        "only the page shown and its neighbours stay in flight: {} after 300 turns",
        s.inflight.len()
    );
    assert!(
        s.inflight.iter().all(|&(page, _)| page.abs_diff(300) <= 1),
        "and they are the pages near page 301: {:?}",
        s.inflight
    );
    assert!(
        asked.len() >= 300,
        "each turn still asks for its page: {}",
        asked.len()
    );
}

#[test]
fn a_fast_scroll_keeps_the_slots_in_flight_and_the_cache_bounded() {
    let mut s = opened(true, "/d/big.pdf", 1, vec![LETTER; 500], (600., 800.));
    let mut asked = renders(&s.take_commands());
    let step = s.layout.height / 500.;
    for page in 1..500 {
        s.scroll_to(step * page as f64);
        asked.extend(renders(&s.take_commands()));
    }
    assert!(
        s.slots.len() <= 6 && s.inflight.len() <= 6,
        "one pass keeps a few slots and renders out: {} slots, {} in flight",
        s.slots.len(),
        s.inflight.len()
    );
    let mut most = 0;
    for (at, (page, width)) in asked.into_iter().enumerate() {
        land(&mut s, 1, page, width, 10 + at as u64);
        most = most.max(s.cache.len());
    }
    let (first, last) = s.layout.near(s.view_top, 800.).unwrap();
    assert!(
        most <= last - first + 3,
        "the late pages never grow the cache past the near window and two: held up to {most}"
    );
    assert!(
        (first..=last).all(|page| s.cache.exact(page, 600).is_some()),
        "and the near window's pages are the ones kept"
    );
}

#[test]
fn a_turn_onto_a_page_that_already_failed_ends_the_loading_state() {
    let mut s = opened(false, "/d/a.pdf", 1, vec![LETTER; 3], (600., 600.));
    let width = s.render_width;
    s.take_commands();
    land(&mut s, 1, 0, width, 5);
    s.reply(
        Reply::RenderFailed {
            doc: 1,
            page: 1,
            width,
        },
        6,
    );
    s.turn(1, 10);
    assert!(
        !renders(&s.take_commands())
            .iter()
            .any(|&(page, _)| page == 1),
        "the failed page is not asked again"
    );
    s.tick(10_000);
    assert_eq!(
        (s.cap.fell_back, s.next_deadline(), s.shown_page()),
        (false, None, None),
        "the turn ends as a failed render, so no loading state stands"
    );
}

#[test]
fn a_failed_stack_page_back_in_view_counts_as_settled_for_the_first_frame() {
    let mut s = opened(true, "/d/a.pdf", 1, vec![LETTER; 50], (600., 800.));
    s.take_commands();
    s.reply(
        Reply::RenderFailed {
            doc: 1,
            page: 1,
            width: 600,
        },
        5,
    );
    s.scroll_to(s.layout.tops[20]);
    s.scroll_to(0.);
    let slot = s.slots.iter().find(|slot| slot.page == 1).copied();
    assert_eq!(
        slot.map(|slot| slot.errored),
        Some(true),
        "the failed page's rebuilt slot is errored: {slot:?}"
    );
    for (page, width) in renders(&s.take_commands()) {
        land(&mut s, 1, page, width, 10);
    }
    assert_eq!(
        s.shown_page(),
        Some(0),
        "the first frame draws with the failed page settled"
    );
}

#[test]
fn a_landed_stack_page_back_in_view_counts_as_settled_for_the_first_frame() {
    let mut s = opened(true, "/d/a.pdf", 1, vec![LETTER; 50], (600., 800.));
    let mut asked = renders(&s.take_commands());
    asked.retain(|&(page, _)| page != 1);
    land(&mut s, 1, 1, 600, 3);
    s.scroll_to(s.layout.tops[20]);
    s.scroll_to(0.);
    asked.extend(renders(&s.take_commands()));
    for (page, width) in asked {
        land(&mut s, 1, page, width, 10);
    }
    assert_eq!(
        s.shown_page(),
        Some(0),
        "a page that landed before it left the near window settles its slot again"
    );
}
