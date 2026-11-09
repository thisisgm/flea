//! An office preview converts only the document's first 30 pages, and the bar and the facts say "30+" when the pages reach that cut.
use super::fixture::{pdf, LETTER};
use super::*;
use gpui::AppContext as _;

// Opens `name` as the subject and answers it with a real PDF of `pages` pages, cut at `first` when the backend says so.
fn answered(
    cx: &mut TestAppContext,
    view: WindowHandle<OfficeView>,
    root: &Root,
    name: &str,
    pages: usize,
    first: Option<usize>,
    id: u64,
) {
    let file = root.0.join(format!("{name}.pdf"));
    std::fs::write(&file, pdf(&vec![LETTER; pages])).unwrap();
    open(cx, view, &format!("/d/{name}.docx"));
    let cut = first.map_or(String::new(), |n| format!(r#","first":{n}"#));
    let line = format!(
        r#"{{"t":"officepdf","id":{id},"state":"ready","path":"{}"{cut}}}"#,
        escape(&file.to_string_lossy())
    );
    assert!(reply(cx, view, &line));
    // An isolated open waits for the next presented frame, and an open inside the last one's settle waits for the settle.
    cx.update_window(view.into(), |_, window, cx| window.simulate_next_frame(cx))
        .unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while view
        .update(cx, |v, _, cx| v.pdf.read(cx).surface.page_count())
        .unwrap()
        != pages
    {
        assert!(
            std::time::Instant::now() < until,
            "{name}: the PDF's pages never arrived"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(
                flea::ui_logic_pv2::pdf::DOCUMENT_SETTLE_MS,
            ));
        draw(cx, view);
    }
}

fn counter_and_pages(
    cx: &mut TestAppContext,
    view: WindowHandle<OfficeView>,
    i18n: &I18n,
) -> (Option<String>, Option<String>) {
    let counter = view
        .update(cx, |v, _, cx| v.bar(i18n, cx).strip.and_then(|s| s.counter))
        .unwrap();
    let facts = view.update(cx, |v, _, cx| v.facts(i18n, cx)).unwrap();
    (
        counter,
        facts
            .into_iter()
            .find(|(k, _)| k == "Pages")
            .map(|(_, v)| v),
    )
}

#[gpui::test]
fn pv2_a_cut_office_preview_counts_its_pages_with_a_plus(cx: &mut TestAppContext) {
    let (view, _, _) = office(cx, true);
    let (root, i18n) = (root(), crate::test_catalogs::english());
    answered(cx, view, &root, "long", 30, Some(30), 1);
    assert_eq!(
        counter_and_pages(cx, view, &i18n),
        (Some("1 / 30+".into()), Some("30+".into())),
        "a PDF that reaches the cut may be a longer document"
    );
    answered(cx, view, &root, "short", 12, Some(30), 2);
    assert_eq!(
        counter_and_pages(cx, view, &i18n),
        (Some("1 / 12".into()), Some("12".into())),
        "a preview short of the cut is the whole document"
    );
    answered(cx, view, &root, "whole", 30, None, 3);
    assert_eq!(
        counter_and_pages(cx, view, &i18n),
        (Some("1 / 30".into()), Some("30".into())),
        "a whole PDF counts exactly"
    );
}
