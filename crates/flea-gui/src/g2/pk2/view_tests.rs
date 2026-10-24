//! GPUI tests: listing and Save entities in a real window.
use super::listing::Cause;
use super::save::SaveField;
use super::view::{ListingView, SaveView};
use crate::list::Typeface;
use flea::i18n::{I18n, Paths};
use gpui::{px, size, TestAppContext};

pub(super) fn english() -> I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-pk2-view");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

pub(super) fn listed(total: usize) -> String {
    format!(r#"{{"t":"listed","path":"/big","n":{total}}}"#)
}

pub(super) fn rows(listing: u64) -> String {
    let row = |name: &str, dir: bool| format!(r#"{{"n":"{name}","d":{dir},"s":0,"m":0,"p":420,"i":""}}"#);
    format!(
        r#"{{"t":"rows","start":0,"listing":{listing},"rows":[{},{}]}}"#,
        row("Docs", true),
        row("a.txt", false)
    )
}

#[gpui::test]
fn a_hundred_thousand_row_folder_draws_its_held_window_and_nothing_more(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update(cx, |view, cx| {
        view.open("/big");
        let gen = view.generation();
        assert!(view.receive(gen, &listed(100_000)));
        assert!(view.receive(gen, &rows(gen)));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("pk2-listing").is_some(), "the listing element is drawn");
    view.update(cx, |view, _| {
        let facts = view.facts();
        assert_eq!((facts.total, facts.held), (100_000, 2), "the total is a count; only the two held rows exist");
        assert_eq!(facts.names, vec!["Docs".to_owned(), "a.txt".to_owned()], "the drawn slots name the held rows");
        assert_eq!(view.shaped_rows(), 2, "both held rows shaped in the frame");
        assert!(view.painted_cells() > 0, "the frame painted the held rows' cells");
    });
}

#[gpui::test]
fn a_backend_death_draws_its_sentence_and_keeps_the_held_rows(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update(cx, |view, cx| {
        view.open("/big");
        let gen = view.generation();
        assert!(view.receive(gen, &listed(100_000)));
        assert!(view.receive(gen, &rows(gen)));
        assert!(view.fail(gen, Cause::Gone));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("pk2-failure").is_some(), "the footer sentence is drawn");
    assert!(cx.debug_bounds("pk2-listing").is_some(), "the held rows stay on screen");
    view.update(cx, |view, _| {
        assert_eq!(view.facts().held, 2, "the held rows stay, as PickerWindow.qml keeps them");
        assert_eq!(view.shaped_rows(), 2, "the kept rows still shaped in the frame");
    });
}

#[gpui::test]
fn a_stale_worker_draws_nothing_of_its_own(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update(cx, |view, cx| {
        view.open("/first");
        let old = view.generation();
        view.open("/second");
        assert!(!view.receive(old, &listed(9)), "the first folder's answer is refused");
        assert!(!view.fail(old, Cause::Gone), "and so is its death");
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("pk2-failure").is_none(), "no sentence for a worker that was replaced");
    view.update(cx, |view, _| {
        assert_eq!(view.facts().total, 0);
        assert_eq!(view.shaped_rows(), 0, "a refused listing shaped no row");
    });
}

#[gpui::test]
fn the_save_field_draws_its_error_line_from_the_field(cx: &mut TestAppContext) {
    let (save, cx) = cx.add_window_view(|_, _| SaveView::new(english(), SaveField::new("")));
    save.update(cx, |save, cx| {
        save.field().edit("a.txt");
        save.field().note_backend_gone();
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("pk2-save-field").is_some());
    assert!(cx.debug_bounds("pk2-save-error").is_some(), "the field carries its own line");
}
