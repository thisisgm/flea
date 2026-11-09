//! tests/js/filter.js, its pure checks, over tests/js/filterfixture.js's seven rows.
use super::*;

// Directories first, as the backend sends them; Screens is capitalised on purpose.
pub(super) const LISTING: [(&str, bool); 7] = [
    ("Screens", true),
    ("scripts", true),
    ("tools", true),
    ("notes.md", false),
    ("readme.txt", false),
    ("screenrecording-2026-08-21.mp4", false),
    ("screenshot-2026-08-30.png", false),
];

// The same seven reversed the way the backend returns them: directories still lead, each run reversed inside itself.
const REVERSED: [(&str, bool); 7] = [
    ("tools", true),
    ("scripts", true),
    ("Screens", true),
    ("screenshot-2026-08-30.png", false),
    ("screenrecording-2026-08-21.mp4", false),
    ("readme.txt", false),
    ("notes.md", false),
];

/// Filter.shown(): the listing rows a query leaves standing in a window held from `held`.
pub(super) fn shown(rows: &[(&str, bool)], held: usize, query: &str) -> Option<Vec<usize>> {
    narrow(query, rows.iter().enumerate().map(|(at, (name, _))| (held + at, *name))).map(|n| n.exact)
}

#[test]
fn a_query_keeps_the_rows_whose_name_contains_it_folded_anywhere() {
    assert_eq!(shown(&LISTING, 0, ""), None, "no query means no filter at all, which every view reads as the identity");
    assert_eq!(shown(&LISTING, 0, "scr"), Some(vec![0, 1, 5, 6]), "a query keeps the rows whose name contains it");
    assert_eq!(shown(&LISTING, 0, "screen"), Some(vec![0, 5, 6]), "matching is case-insensitive, so capital Screens survives");
    assert_eq!(shown(&LISTING, 0, "2026"), Some(vec![5, 6]), "and it matches anywhere in the name, not only at the start");
    assert_eq!(shown(&LISTING, 0, "zzz").map(|list| list.len()), Some(0), "a query nothing matches keeps an empty list");
    assert!(shown(&LISTING, 0, "zzz").is_some(), "and an empty match list is still a filter, so it is not null");
    assert_eq!(shown(&LISTING, 400, "2026"), Some(vec![405, 406]), "the answers are listing rows, so a held window offsets them");
}

#[test]
fn a_filtered_view_keeps_the_backends_order() {
    let mixed = shown(&LISTING, 0, "scr").unwrap();
    let dirs: Vec<bool> = mixed.iter().map(|&row| LISTING[row].1).collect();
    assert_eq!(dirs, vec![true, true, false, false], "a filtered view keeps the backend's order, so directories still come first");
    let mut sorted = mixed.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, mixed, "and the match list is strictly ascending, which is what keeps that true");
    assert!(mixed.len() < LISTING.len(), "and it really dropped rows, so the check above has a denominator");
}

#[test]
fn view_positions_and_listing_rows_convert_both_ways() {
    let mixed = shown(&LISTING, 0, "scr").unwrap();
    let list = Some(mixed.as_slice());
    assert_eq!(at(list, 2), Some(5), "a view position resolves to the listing row it draws");
    assert_eq!(view_of(list, 5), Some(2), "and back again");
    assert_eq!(view_of(list, 3), None, "a row the filter hides has no view position");
    assert_eq!(at(list, 9), None, "a view position past the end is a listing row nothing draws");
    assert_eq!((at(None, 12), view_of(None, 12)), (Some(12), Some(12)), "with no filter both directions are the identity");
}

#[test]
fn between_keep_and_span_never_reach_a_hidden_row() {
    let mixed = shown(&LISTING, 0, "scr").unwrap();
    let list = Some(mixed.as_slice());
    assert_eq!(between(&mixed, 1, 3), vec![1, 5, 6], "the rows drawn between two view positions skip the ones the filter hid");
    assert_eq!(between(&mixed, 3, 1), vec![1, 5, 6], "the ends are inclusive and their order does not matter");
    assert_eq!(keep(&[1, 2, 3, 5], list), vec![1, 5], "a request is cut back to the rows the filter still shows");
    assert_eq!(keep(&[1, 2, 3], None), vec![1, 2, 3], "with no filter a request passes through untouched");
    assert_eq!(span(list, 1, 3), Some((1, 6)), "a view range spans the listing rows at its ends");
    assert_eq!(span(None, 3, 9), Some((3, 9)), "with no filter the range is itself");
    assert_eq!(span(Some(&[]), 0, 5), None, "a filter matching nothing spans nothing, so no row is ever asked for");
}

// Thumbs.plan and DirSizes.plan belong to their own units; their plans enter here as the data they produce.
#[test]
fn thumbnail_and_folder_size_plans_are_cut_to_the_drawn_rows() {
    let mixed = shown(&LISTING, 0, "scr").unwrap();
    assert_eq!(cut(&[1, 3, 5], &[9], Some(&mixed), &[]), (vec![1, 5], vec![9]), "a thumbnail plan is cut on its asks and keeps its drops");
    // Rows 400 to 406: thumbnails at 400, 401, 403, 405 and 406, folders at 402 and 404; 403 is already cached.
    let sparse = [401, 405];
    assert_eq!(span(Some(&sparse), 0, 1), Some((401, 405)));
    let pending_wide = [400, 401, 405, 406];
    let plan_ask = [401, 403, 405];
    let narrowed = cut(&plan_ask, &[], Some(&sparse), &pending_wide);
    assert_eq!(narrowed.1, vec![400, 406], "a narrowed viewport cancels every hidden pending thumbnail, including gaps between matches");
    assert!(!narrowed.1.contains(&403), "completed thumbnails stay cached when the filter hides them");
    let hidden_inside = [401, 403, 405];
    assert_eq!(cut(&[], &[], Some(&sparse), &hidden_inside).1, vec![403], "a pending thumbnail between visible matches is cancelled");
    assert_eq!(cut(&[], &[403], Some(&sparse), &hidden_inside).1, vec![403], "an already scheduled cancellation is not duplicated");
    assert_eq!(cut(&[], &[], Some(&[]), &hidden_inside), (vec![], vec![401, 403, 405]), "zero matches cancels all pending thumbnails");
    assert_eq!(cut(&[400], &[406], None, &hidden_inside), (vec![400], vec![406]), "clearing leaves the ordinary viewport plan intact");
    assert_eq!(narrowed.0, vec![401, 405], "sparse filtered tile requests retain backend row identities");
    assert!(keep(&[402, 404], Some(&sparse)).is_empty(), "directory-size requests exclude directories hidden between filtered tiles");
}

#[test]
fn the_cursor_steps_through_drawn_rows_not_listing_rows() {
    let walk = shown(&LISTING, 0, "2026").unwrap();
    let list = Some(walk.as_slice());
    assert_eq!(step(list, 7, 5, 1), Some((6, 1)), "j steps to the next row the filter shows, scrolling to its view position");
    assert_eq!(step(list, 7, 6, 1), Some((6, 1)), "the last row shown is where down stops");
    assert_eq!(cursor_at_view(list, 7, 0), Some((5, 0)), "g goes to the first row shown, which is not the listing's first row");
    assert_eq!(step(list, 7, 5, -1), Some((5, 0)), "and up stops there too");
    let apart = shown(&LISTING, 0, "o").unwrap();
    assert_eq!(apart, vec![2, 3, 5, 6]);
    let list = Some(apart.as_slice());
    assert_eq!(step(list, 7, 2, 1), Some((3, 1)), "a step down lands on the next row drawn, not on cursor plus one");
    assert_eq!(step(list, 7, 6, -1), Some((5, 2)), "a step up lands on the previous row drawn, not on cursor minus one");
    assert_eq!(cursor_at_view(list, 7, 3), Some((6, 3)), "the last view position is the last row drawn");
    assert_eq!(step(None, 7, 3, 1), Some((4, 4)), "with no filter the cursor still steps one listing row");
    assert_eq!(step(Some(&[]), 7, 0, 1), None, "a filter matching nothing has no row to move to");
}

#[test]
fn the_wheel_pulls_the_cursor_onto_a_drawn_row_it_can_see() {
    let walk = shown(&LISTING, 0, "2026").unwrap();
    assert_eq!(clamp(Some(&walk), 5, 1, 1), Some(6), "a scrolled viewport pulls the cursor onto a row it can see");
    assert_eq!(clamp(Some(&walk), 6, 0, 1), None, "and leaves it alone once it is inside");
}

#[test]
fn a_hidden_mark_drops_out_and_a_kept_one_stays() {
    let narrowed = shown(&LISTING, 0, "scr").unwrap();
    assert_eq!(pruned(&[3, 5, 6], Some(&narrowed)), vec![3], "a row the filter hides drops out of the selection with it");
    assert!(pruned(&[5, 6], None).is_empty(), "clearing the filter keeps what survived it and restores nothing");
}

#[test]
fn a_sort_reorders_the_rows_and_the_query_follows_the_new_numbering() {
    let sort_first = shown(&REVERSED, 0, "scr").unwrap();
    assert_eq!(sort_first, vec![1, 2, 3, 4], "sorting first and filtering after keeps the sorted order");
    let names = |rows: &[(&str, bool)], list: &[usize]| list.iter().map(|&i| rows[i].0).collect::<Vec<_>>().join(" ");
    let mixed = shown(&LISTING, 0, "scr").unwrap();
    assert_eq!(
        format!("{} then {}", names(&LISTING, &mixed), names(&REVERSED, &sort_first)),
        "Screens scripts screenrecording-2026-08-21.mp4 screenshot-2026-08-30.png then \
         scripts Screens screenshot-2026-08-30.png screenrecording-2026-08-21.mp4",
        "the matches name the reordered rows, not the ones that used to be there"
    );
    let dirs: Vec<bool> = sort_first.iter().map(|&i| REVERSED[i].1).collect();
    assert_eq!(dirs, vec![true, true, false, false], "and directories still lead the filtered view after a reverse");
}

#[test]
fn the_strip_counts_shown_tested_and_close_apart() {
    let counts = Counts::of(&narrow("scr", LISTING.iter().enumerate().map(|(i, r)| (i, r.0))).unwrap(), 7, Scope::Folder);
    assert_eq!(counts, Counts { shown: 4, tested: 7, close: 0, scope: Scope::Folder });
    let typo = Counts::of(&narrow("raedme", LISTING.iter().enumerate().map(|(i, r)| (i, r.0))).unwrap(), 7, Scope::Subfolders);
    assert_eq!((typo.shown, typo.close, typo.nothing()), (0, 1, false), "a close match is not the no-match state");
    let none = Counts::of(&narrow("zzz", LISTING.iter().enumerate().map(|(i, r)| (i, r.0))).unwrap(), 7, Scope::Folder);
    assert!(none.nothing(), "nothing exact and nothing close is");
    assert_eq!(Scope::Folder.toggled().toggled(), Scope::Folder, "Tab again returns to this folder");
}
