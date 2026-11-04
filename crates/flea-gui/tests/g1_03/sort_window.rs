use super::*;

// A sort whose window failed, then a scroll: the pair the sort armed must take the scroll's answer at the scroll's count.
fn scrolled_after_a_failed_sort_window(height: f32) -> (Pane, usize, (usize, usize)) {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let asked = window_count(&mut nav);
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    assert!(!pane.fail("window", "/f/base", "timed out", 0, now));
    nav.after_failure(&mut pane, "window");
    pane.set_height(height);
    pane.scroll_to(400. * 31.);
    pane.cursor = 420;
    let window = pane.wanted_window().expect("the scroll asks a window");
    (pane, asked, window)
}

#[test]
fn a_scroll_answer_after_a_failed_sort_window_lands_at_its_own_count() {
    let (mut pane, asked, (start, count)) = scrolled_after_a_failed_sort_window(900.);
    assert_ne!(asked, count, "the height change moved the count");
    let now = pane.now();
    let landed = pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(start, count, named),
            listing: 2,
        },
        now,
    );
    assert_eq!(
        landed,
        Ok(true),
        "sort asked {asked}, scroll asked {count} at {start}"
    );
}

#[test]
fn a_scroll_answer_after_a_failed_sort_window_leaves_the_pane_able_to_ask_again() {
    let (mut pane, _, (start, count)) = scrolled_after_a_failed_sort_window(900.);
    let now = pane.now();
    let landed = pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(start, count, named),
            listing: 2,
        },
        now,
    );
    pane.scroll_to(700. * 31.);
    assert!(
        pane.wanted_window().is_some(),
        "answer {landed:?}, still out {:?}",
        pane.requests.outstanding
    );
}

#[test]
fn a_scroll_answer_after_a_failed_sort_window_keeps_the_cursor_and_the_view_where_the_user_left_them(
) {
    let (mut pane, asked, (start, count)) = scrolled_after_a_failed_sort_window(620.);
    assert_eq!(asked, count, "an unchanged height asks the sort's count");
    let now = pane.now();
    let landed = pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(start, count, named),
            listing: 2,
        },
        now,
    );
    assert_eq!(landed, Ok(true));
    assert_eq!((pane.held.listing, pane.held.start), (2, start));
    assert_eq!(
        (pane.cursor, pane.scroll_y),
        (420, 400. * 31.),
        "the view stays where the user scrolled"
    );
}

#[test]
fn a_malformed_scroll_answer_after_a_failed_sort_window_settles_its_request() {
    let (mut pane, _, (start, count)) = scrolled_after_a_failed_sort_window(620.);
    let now = pane.now();
    let landed = pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(start, count - 1, named),
            listing: 2,
        },
        now,
    );
    assert!(landed.is_err());
    assert_eq!(
        pane.requests.outstanding, None,
        "the refused answer settles its request"
    );
    pane.scroll_to(700. * 31.);
    let (start, count) = pane.wanted_window().expect("a moved view asks again");
    let landed = pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(start, count, named),
            listing: 2,
        },
        now,
    );
    assert_eq!(
        landed,
        Ok(true),
        "the sorted listing still lands at the next window asked"
    );
}

// Home during a scrolled sort asks another top window; the sort's own fails after a resize, and the first late answer is the view's.
fn resized_before_a_failed_sort_window() -> (Pane, usize, Option<(usize, usize)>) {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    pane.scroll_to(400. * 31.);
    let (start, count) = pane.wanted_window().expect("the scroll asks a window");
    let now = pane.now();
    let scrolled = pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(start, count, named),
            listing: 1,
        },
        now,
    );
    assert_eq!(scrolled, Ok(true));
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let asked = window_count(&mut nav);
    pane.move_cursor(flea_gui::pane::Move::Home);
    assert_eq!(
        pane.wanted_window(),
        Some((0, asked)),
        "Home asks the missing top while the sort's own window is still out"
    );
    pane.set_height(900.);
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    assert!(!pane.fail("window", "/f/base", "timed out", 0, now));
    nav.after_failure(&mut pane, "window");
    let late = pane.wanted_window();
    (pane, asked, late)
}

#[test]
fn an_answer_asked_before_a_resize_is_checked_against_its_own_count() {
    let (mut pane, asked, late) = resized_before_a_failed_sort_window();
    let now = pane.now();
    let first = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, asked, named),
            listing: 2,
        },
        now,
    );
    assert_eq!(
        first,
        Ok(true),
        "the view's window asked {asked} at 0, the window asked after the failure is {late:?}"
    );
}

#[test]
fn both_late_answers_leave_the_sorted_top_held() {
    let (mut pane, asked, late) = resized_before_a_failed_sort_window();
    let now = pane.now();
    let first = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, asked, named),
            listing: 2,
        },
        now,
    );
    let second = late.map(|(start, count)| {
        pane.receive_at(
            Reply::Rows {
                start,
                rows: rows(start, count, named),
                listing: 2,
            },
            now,
        )
    });
    assert_eq!(
        (pane.held.listing, pane.held.start),
        (2, 0),
        "answers {first:?} then {second:?}, asks next {:?}, refused {:?}",
        pane.wanted_window(),
        pane.requests.refused
    );
}

// A malformed answer to the sort's own window refuses those rows, and the sorted listing still lands at the next window asked.
#[test]
fn a_refused_sort_window_leaves_the_sorted_listing_to_the_next_window_asked() {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let asked = window_count(&mut nav);
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    let refused = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, asked - 1, named),
            listing: 2,
        },
        now,
    );
    assert_eq!(refused, Err("pane.reply_window".into()));
    assert_eq!(pane.held.listing, 1, "refused rows never land");
    pane.scroll_to(400. * 31.);
    let (start, count) = pane.wanted_window().expect("the scroll asks a window");
    let landed = pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(start, count, named),
            listing: 2,
        },
        now,
    );
    assert_eq!(
        landed,
        Ok(true),
        "still out {:?}",
        pane.requests.outstanding
    );
    assert_eq!(
        (
            pane.held.listing,
            pane.held.start,
            pane.requests.outstanding
        ),
        (2, start, None)
    );
    assert_eq!(
        (pane.cursor, pane.scroll_y),
        (0, 400. * 31.),
        "the view stays where the user scrolled"
    );
}

// A listed line no sort asked arms a pair whose own rows come with that listing, so a window error answers the view's request.
#[test]
fn a_window_error_under_a_listed_line_no_sort_asked_settles_the_view_request() {
    let (mut pane, _, _) = settled("/f/base", 1000);
    pane.scroll_to(400. * 31.);
    let (start, count) = pane.wanted_window().expect("the scroll asks a window");
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    assert!(!pane.fail("window", "/f/base", "timed out", 0, now));
    assert_eq!(
        pane.requests.outstanding, None,
        "the error answered the view's request"
    );
    assert_eq!(
        pane.wanted_window(),
        Some((start, count)),
        "the view asks again"
    );
    let first = pane.first_count();
    let landed = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, first, named),
            listing: 2,
        },
        now,
    );
    assert_eq!(
        landed,
        Ok(true),
        "the listing's own rows land at its own count"
    );
    assert_eq!((pane.held.listing, pane.cursor), (2, 0));
}

// A second sort while Home's top window is still out after the first sort's window failed.
fn resorted_after_a_failed_sort_window() -> (Pane, Navigator, usize, usize, Option<(usize, usize)>)
{
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    pane.scroll_to(400. * 31.);
    let (start, count) = pane.wanted_window().expect("the scroll asks a window");
    let now = pane.now();
    let scrolled = pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(start, count, named),
            listing: 1,
        },
        now,
    );
    assert_eq!(scrolled, Ok(true));
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let asked = window_count(&mut nav);
    pane.move_cursor(flea_gui::pane::Move::Home);
    assert_eq!(
        pane.wanted_window(),
        Some((0, asked)),
        "Home asks the missing top while the sort's own window is still out"
    );
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    assert!(!pane.fail("window", "/f/base", "timed out", 0, now));
    nav.after_failure(&mut pane, "window");
    pane.set_height(900.);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    let second = window_count(&mut nav);
    let resorted = pane.wanted_window();
    (pane, nav, asked, second, resorted)
}

#[test]
fn a_late_view_answer_after_a_resort_is_checked_against_its_own_count() {
    let (mut pane, _, asked, second, resorted) = resorted_after_a_failed_sort_window();
    let now = pane.now();
    let late = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, asked, named),
            listing: 2,
        },
        now,
    );
    assert!(late.is_ok(), "the view's window asked {asked} at 0 before the second sort, the window asked after it is {resorted:?}: {late:?}");
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    let landed = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, second, named),
            listing: 3,
        },
        now,
    );
    assert_eq!(landed, Ok(true));
}

#[test]
fn a_resort_whose_window_also_fails_leaves_the_top_held() {
    let (mut pane, mut nav, asked, _, resorted) = resorted_after_a_failed_sort_window();
    let now = pane.now();
    let late = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, asked, named),
            listing: 2,
        },
        now,
    );
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    assert!(!pane.fail("window", "/f/base", "timed out", 0, now));
    nav.after_failure(&mut pane, "window");
    let answer = resorted.map(|(start, count)| {
        pane.receive_at(
            Reply::Rows {
                start,
                rows: rows(start, count, named),
                listing: 3,
            },
            now,
        )
    });
    assert!(
        pane.held.listing > 1 && pane.held.row(0).is_some(),
        "late {late:?}, answer {answer:?}, held {:?}, asks next {:?}, refused {:?}",
        (pane.held.listing, pane.held.start),
        pane.wanted_window(),
        pane.requests.refused
    );
}

// A second sort while a scroll's window is out under a pair whose own sort window failed.
fn resorted_while_a_scroll_window_is_out() -> (Pane, Navigator, (usize, usize)) {
    let (mut pane, mut nav, mut sel) = settled("/f/base", 1000);
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    window_count(&mut nav);
    assert_eq!(
        pane.wanted_window(),
        None,
        "the old top is held, so the view asks nothing"
    );
    let now = pane.now();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    assert!(!pane.fail("window", "/f/base", "timed out", 0, now));
    nav.after_failure(&mut pane, "window");
    pane.scroll_to(400. * 31.);
    let scroll = pane.wanted_window().expect("the scroll asks a window");
    assert_eq!(nav.sort_next(&mut pane, &mut sel, String::new()), Ok(true));
    window_count(&mut nav);
    (pane, nav, scroll)
}

#[test]
fn a_late_scroll_answer_after_a_resort_keeps_rows_on_the_resorted_top() {
    let (mut pane, _, (start, count)) = resorted_while_a_scroll_window_is_out();
    let now = pane.now();
    let late = pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(start, count, named),
            listing: 2,
        },
        now,
    );
    let shown = (0..pane.visible_rows())
        .filter(|&index| pane.held.row(index).is_some())
        .count();
    eprintln!("scroll asked ({start}, {count}); late answer {late:?}; held {:?}; view row {}; rows on screen {shown} of {}",
        (pane.held.listing, pane.held.start), (pane.scroll_y / 31.) as usize, pane.visible_rows());
    assert_eq!(
        shown,
        pane.visible_rows(),
        "the resorted top keeps rows on screen until the second sort lands"
    );
}

#[test]
fn the_second_sort_still_lands_after_a_late_scroll_answer() {
    let (mut pane, mut nav, (start, count)) = resorted_while_a_scroll_window_is_out();
    let now = pane.now();
    let late = pane.receive_at(
        Reply::Rows {
            start,
            rows: rows(start, count, named),
            listing: 2,
        },
        now,
    );
    let render = pane.wanted_window();
    pane.receive_at(
        Reply::Listed {
            path: "/f/base".into(),
            total: 1000,
        },
        now,
    )
    .unwrap();
    let first = pane.first_count();
    let landed = pane.receive_at(
        Reply::Rows {
            start: 0,
            rows: rows(0, first, named),
            listing: 3,
        },
        now,
    );
    nav.after_reply(&mut pane, "/f/base", &mut Vec::new());
    assert_eq!(landed, Ok(true), "late {late:?}, render asked {render:?}");
    assert_eq!((pane.held.listing, pane.held.start, pane.cursor), (3, 0, 0));
}
