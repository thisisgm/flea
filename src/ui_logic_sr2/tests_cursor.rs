//! tests/js/filter-cursor.js, issue 92: what the cursor is worth while a filter runs, the fallback target's rule.
use super::tests::{shown, LISTING};
use super::*;

fn loaded_from(held: usize) -> impl Fn(usize) -> bool {
    move |row| (held..held + LISTING.len()).contains(&row)
}

#[test]
fn a_cursor_the_filter_hides_is_no_target_and_says_so() {
    let nomatch = shown(&LISTING, 0, "zzz").unwrap();
    assert!(!cursor_shown(Some(&nomatch), Some(3)), "a cursor the filter hides is not a row the pane can act on");
    assert_eq!(targets(&[], Some(&nomatch), Some(3), loaded_from(0)), Err(Refusal::HiddenByFilter), "its target list is empty and says why");
    assert_eq!(targets(&[], None, None, loaded_from(0)), Err(Refusal::NothingToActOn), "an empty listing is not the filter's doing");
    assert_eq!(targets(&[], Some(&nomatch), None, loaded_from(0)), Err(Refusal::NothingToActOn), "a filter with no cursor at all is nothing to act on");
    let away = shown(&LISTING, 400, "2026").unwrap();
    assert_eq!(targets(&[], Some(&away), Some(3), loaded_from(400)), Err(Refusal::NotLoaded), "a cursor outside the loaded window has not loaded yet");
}

#[test]
fn a_cursor_the_filter_draws_is_still_the_fallback_target() {
    let visible = shown(&LISTING, 0, "2026").unwrap();
    let cursor = at(Some(&visible), 0);
    assert!(cursor_shown(Some(&visible), cursor), "a cursor the filter draws is still a row to act on");
    assert_eq!(targets(&[], Some(&visible), cursor, loaded_from(0)), Ok(vec![5]), "and it is still the fallback target");
    assert_eq!(targets(&[6], Some(&visible), Some(3), loaded_from(0)), Ok(vec![6]), "a selection outranks the cursor");
}

#[test]
fn a_cursor_the_filter_hides_lands_on_the_first_row_left_standing() {
    let narrowed = shown(&LISTING, 0, "2026").unwrap();
    assert_eq!(cursor_after(Some(&narrowed), 3), Some(5), "a cursor the filter hides lands on the first row it left standing");
    assert_eq!(cursor_after(Some(&narrowed), 6), None, "a cursor the filter keeps does not move");
    assert_eq!(cursor_after(Some(&[]), 3), None, "nothing standing leaves the cursor where it was");
    assert_eq!(cursor_after(None, 3), None, "no filter moves nothing");
}
