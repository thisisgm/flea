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
    let row =
        |name: &str, dir: bool| format!(r#"{{"n":"{name}","d":{dir},"s":0,"m":0,"p":420,"i":""}}"#);
    format!(
        r#"{{"t":"rows","start":0,"listing":{listing},"rows":[{},{}]}}"#,
        row("Docs", true),
        row("a.txt", false)
    )
}

#[gpui::test]
fn a_hundred_thousand_row_folder_draws_its_held_window_and_nothing_more(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update(cx, |view, cx| {
        view.open("/big");
        let gen = view.generation();
        assert!(view.receive(gen, &listed(100_000)));
        assert!(view.receive(gen, &rows(gen)));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("pk2-listing").is_some(),
        "the listing element is drawn"
    );
    view.update(cx, |view, _| {
        let facts = view.facts();
        assert_eq!(
            (facts.total, facts.held),
            (100_000, 2),
            "the total is a count; only the two held rows exist"
        );
        assert_eq!(
            facts.names,
            vec!["Docs".to_owned(), "a.txt".to_owned()],
            "the drawn slots name the held rows"
        );
        assert_eq!(view.shaped_rows(), 2, "both held rows shaped in the frame");
        assert!(
            view.painted_cells() > 0,
            "the frame painted the held rows' cells"
        );
    });
}

#[gpui::test]
fn ey479_backend_death_keeps_rows_without_a_second_failure_sentence(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
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
    assert!(
        cx.debug_bounds("pk2-failure").is_none(),
        "the chooser status reports backend death without contradicting the retained rows"
    );
    assert!(
        cx.debug_bounds("pk2-listing").is_some(),
        "the held rows stay on screen"
    );
    view.update(cx, |view, _| {
        assert_eq!(
            view.facts().held,
            2,
            "the held rows stay, as PickerWindow.qml keeps them"
        );
        assert_eq!(
            view.shaped_rows(),
            2,
            "the kept rows still shaped in the frame"
        );
    });
}

#[gpui::test]
fn ey479_backend_death_before_rows_still_draws_the_listing_failure(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
    view.update(cx, |view, cx| {
        view.open("/big");
        assert!(view.fail(view.generation(), Cause::Gone));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("pk2-failure").is_some());
}

#[gpui::test]
fn a_stale_worker_draws_nothing_of_its_own(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update(cx, |view, cx| {
        view.open("/first");
        let old = view.generation();
        view.open("/second");
        assert!(
            !view.receive(old, &listed(9)),
            "the first folder's answer is refused"
        );
        assert!(!view.fail(old, Cause::Gone), "and so is its death");
        cx.notify();
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("pk2-failure").is_none(),
        "no sentence for a worker that was replaced"
    );
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
    assert!(
        cx.debug_bounds("pk2-save-error").is_some(),
        "the field carries its own line"
    );
}

#[gpui::test]
fn ey377_opening_another_folder_resets_the_scrolled_viewport(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
    view.update(cx, |view, _| {
        view.open("/big");
        view.set_scroll_y(900.);
        view.open("/other");
        assert_eq!(
            view.scroll_y(),
            0.,
            "a replacement listing starts at its first row"
        );
    });
}

#[gpui::test]
fn ey377_fractional_scroll_fetches_the_trailing_partial_row(cx: &mut TestAppContext) {
    const HELD: usize = 350;
    let (view, cx) =
        cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update(cx, |view, cx| {
        view.open("/big");
        let generation = view.generation();
        assert!(view.receive(generation, &listed(1000)));
        let rows = (0..HELD)
            .map(|index| {
                format!(r#"{{"n":"file-{index}.txt","d":false,"s":0,"m":0,"p":420,"i":""}}"#)
            })
            .collect::<Vec<_>>()
            .join(",");
        assert!(view.receive(
            generation,
            &format!(r#"{{"t":"rows","start":0,"listing":{generation},"rows":[{rows}]}}"#)
        ));
        cx.notify();
    });
    cx.run_until_parked();
    view.update(cx, |view, _| {
        let (height, row) = {
            let state = view.drawn();
            (
                f32::from(state.drawn.bounds.size.height),
                state.file_metrics().row,
            )
        };
        let first = HELD - view.list_visible_rows();
        view.set_scroll_y(first as f32 * row);
        assert!(
            view.list_window_request().is_none(),
            "all visible rows are held before fractional scrolling"
        );
        let fraction = (height / row).fract();
        let offset = if fraction == 0. {
            0.5
        } else {
            1. - fraction / 2.
        };
        view.set_scroll_y((first as f32 + offset) * row);
        let line = view
            .list_window_request()
            .expect("the newly exposed trailing partial row needs fetching");
        let request = flea::jsondoc::parse(&line).unwrap();
        let start = request
            .get("start")
            .and_then(|value| value.as_f64())
            .unwrap() as usize;
        let count = request
            .get("count")
            .and_then(|value| value.as_f64())
            .unwrap() as usize;
        assert!(
            start <= HELD && start + count > HELD,
            "the fetched window includes the partial row at {HELD}"
        );
    });
}

// EY-511: a tagged row in the chooser's List reserves the folder-wide tag slot, so its squares draw as the window's do.
#[gpui::test]
fn fix_picker_2b_tagged_rows_draw_the_tag_slot_in_the_chooser_list(cx: &mut TestAppContext) {
    let (view, cx) =
        cx.add_window_view(|_, _| ListingView::new(english(), Typeface::omarchy_default()));
    cx.simulate_resize(size(px(800.), px(600.)));
    let tagged = |listing: u64| {
        format!(
            r#"{{"t":"rows","start":0,"listing":{listing},"rows":[{{"n":"notes.txt","d":false,"s":0,"m":0,"p":420,"i":"","tg":["Red","Blue"]}},{{"n":"todo.md","d":false,"s":0,"m":0,"p":420,"i":""}}]}}"#
        )
    };
    let shown = |view: &gpui::Entity<ListingView>, cx: &mut gpui::VisualTestContext| {
        cx.run_until_parked();
        view.update(cx, |view, _| view.drawn().view.tags_shown)
    };
    view.update(cx, |view, cx| {
        view.open("/work");
        let gen = view.generation();
        assert!(view.receive(gen, &listed(4)));
        assert!(view.receive(gen, &tagged(gen)));
        cx.notify();
    });
    assert!(shown(&view, cx), "a tagged row reserves the tag slot");
    view.update(cx, |view, cx| {
        let gen = view.generation();
        let later = rows(gen).replace(r#""start":0"#, r#""start":2"#);
        assert!(view.receive(gen, &later));
        cx.notify();
    });
    assert!(
        shown(&view, cx),
        "the slot stays for the folder once a held window without tags replaces the tagged one"
    );
    view.update(cx, |view, cx| {
        view.open("/plain");
        let gen = view.generation();
        assert!(view.receive(gen, &listed(2)));
        assert!(view.receive(gen, &rows(gen)));
        cx.notify();
    });
    assert!(!shown(&view, cx), "an untagged folder draws no tag slot");
}
