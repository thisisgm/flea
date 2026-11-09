//! The archive page (ql-archive, Preview.qml archivePane): one meta ask per path, the rows the frame has room for, the
//! "+ N more" line counting every entry not named, and a failed index drawing one sentence.
use super::parts::{catalog, draw, mark, with, Calls};
use flea::ui_logic_doc_preview::archive::Meta;
use flea::ui_logic_doc_preview::refusal::Refusal;
use flea_gui::g2::doc_preview::archive::{ArchiveHost, ArchiveView, Metrics, Palette};
use flea_gui::g2::doc_preview::code::shape;
use gpui::AppContext as _;
use gpui::{px, TestAppContext, WindowHandle};

struct Host(Calls<String>);

impl ArchiveHost for Host {
    fn ask_meta(&self, path: &str) {
        self.0.borrow_mut().push(path.into());
    }
}

fn open(cx: &mut TestAppContext) -> (WindowHandle<ArchiveView>, Calls<String>) {
    let calls = Calls::default();
    let host = Host(calls.clone());
    let view = cx.add_window(|_, cx| {
        let ink = flea_gui::palette::Theme::default().foreground;
        let palette = Palette {
            foreground: ink,
            muted: ink.opacity(0.6),
            muted_frame: ink.opacity(0.3),
        };
        let metrics = Metrics {
            inset: 14.,
            gap: 9.,
            row: 22.,
            glyph: 14.,
            font: gpui::font("sans-serif"),
            name: px(14.),
            caption: px(12.),
            more_opacity: 0.6,
        };
        let mut view = ArchiveView::new(
            palette,
            metrics,
            mark("archive"),
            mark("file"),
            mark("folder"),
        );
        view.set_host(Some(Box::new(host)));
        view.set_catalog(catalog(), cx);
        view
    });
    view.update(cx, |v, _, cx| {
        v.set_active(true, cx);
        v.set_subject("/tmp/backup.tar.gz", cx);
    })
    .unwrap();
    draw(cx, view);
    (view, calls)
}

fn meta(entries: u64, names: usize) -> Meta {
    let names = (0..names)
        .map(|n| (format!("entry-{n}"), n % 5 == 0))
        .collect();
    Meta {
        entries,
        unpacked: 3_400_000_000,
        failed: false,
        names,
    }
}

#[gpui::test]
fn pv2_archive_lists_what_fits_and_counts_the_rest(cx: &mut TestAppContext) {
    let i18n = catalog();
    let (view, calls) = open(cx);
    draw(cx, view);
    assert_eq!(
        calls.borrow().as_slice(),
        ["/tmp/backup.tar.gz"],
        "one meta ask per path"
    );
    assert_eq!(with(cx, view, |v| v.status(&i18n)), "loading");
    view.update(cx, |v, _, cx| {
        assert!(
            !v.answered("/tmp/other.zip", meta(1, 1), cx),
            "another row's answer is dropped"
        )
    })
    .unwrap();
    view.update(cx, |v, _, cx| {
        v.answered("/tmp/backup.tar.gz", meta(214, 100), cx)
    })
    .unwrap();
    draw(cx, view);
    let shown = with(cx, view, |v| v.shown(400.));
    let (line, rows, more) = with(cx, view, |v| v.texts(&i18n, 400.)).expect("the index landed");
    assert_eq!(line, "214 entries · 3.4 GB unpacked", "the count line");
    assert_eq!(rows.len(), shown, "as many rows as the frame has room for");
    assert_eq!(
        more,
        Some(format!("+ {} more", 214 - shown)),
        "the rest, named or not, in the more line"
    );
    let shaped = with(cx, view, |v| v.shaped);
    draw(cx, view);
    assert_eq!(
        with(cx, view, |v| v.shaped),
        shaped,
        "an idle redraw shapes nothing"
    );
    let (_, few, none) = with(cx, view, |v| v.texts(&i18n, 4_000.)).unwrap();
    assert_eq!(
        (few.len(), none),
        (100, Some("+ 114 more".into())),
        "the wire's cap bounds the rows too"
    );
    assert_eq!(with(cx, view, |v| v.status(&i18n)), "archive");
}

#[gpui::test]
fn pv2_archive_failed_index_draws_its_sentence(cx: &mut TestAppContext) {
    let i18n = catalog();
    let (view, _) = open(cx);
    view.update(cx, |v, _, cx| {
        v.answered(
            "/tmp/backup.tar.gz",
            Meta {
                failed: true,
                ..Meta::default()
            },
            cx,
        )
    })
    .unwrap();
    draw(cx, view);
    let got = with(cx, view, |v| {
        (v.refusal(), v.fail_sentence(&i18n), v.texts(&i18n, 400.))
    });
    assert_eq!(
        got,
        (
            Some(Refusal::ArchiveUnreadable),
            Some("This archive could not be read.".into()),
            None
        )
    );
    let (view, _) = open(cx);
    view.update(cx, |v, _, cx| {
        v.answered("/tmp/backup.tar.gz", meta(0, 0), cx)
    })
    .unwrap();
    let (line, rows, more) = with(cx, view, |v| v.texts(&i18n, 400.)).unwrap();
    assert_eq!(
        (line.as_str(), rows.len(), more),
        ("0 entries", 0, None),
        "an empty archive states its count"
    );
}

#[gpui::test]
fn pv2_archive_long_names_are_elided_to_the_card(cx: &mut TestAppContext) {
    let (view, _) = open(cx);
    let long = format!(
        "project-1.2.3/{}index.js",
        "node_modules/@scope/package/dist/esm/".repeat(12)
    );
    view.update(cx, |v, _, cx| {
        v.set_subject(
            &format!("/tmp/{}.tar.gz", "very-long-archive-name-".repeat(30)),
            cx,
        );
        let path = v.path.clone();
        v.answered(
            &path,
            Meta {
                entries: 1,
                unpacked: 10,
                failed: false,
                names: vec![(long.clone(), false)],
            },
            cx,
        );
    })
    .unwrap();
    draw(cx, view);
    let width = scale_free_width(cx, view);
    let drawn = with(cx, view, |v| v.drawn());
    let (name, entry) = (&drawn[0], &drawn[2]);
    assert!(
        name.0.contains('\u{2026}') && !name.0.ends_with('\u{2026}'),
        "the name is elided in the middle: {}",
        name.0
    );
    assert!(
        name.1 <= width - 28.,
        "and fits the card between its 14 px insets: {} of {width}",
        name.1
    );
    assert!(
        entry.0.ends_with('\u{2026}') && entry.0.len() < long.len(),
        "an entry is elided at its end: {}",
        entry.0
    );
    assert!(
        entry.1 <= width - 28. - 14. - 9.,
        "and fits beside its mark: {} of {width}",
        entry.1
    );
}

fn scale_free_width(cx: &mut TestAppContext, view: WindowHandle<ArchiveView>) -> f32 {
    cx.update_window(view.into(), |_, window, _| {
        f32::from(window.viewport_size().width)
    })
    .unwrap()
}

#[gpui::test]
fn pv2_archive_elision_shapes_only_what_can_fit(cx: &mut TestAppContext) {
    let (view, _) = open(cx);
    let long = "a/".repeat(32_768);
    let before = shape::shaped_bytes();
    view.update(cx, |v, _, cx| {
        v.set_subject(&format!("/tmp/{}.zip", "n".repeat(65_536)), cx);
        let path = v.path.clone();
        v.answered(
            &path,
            Meta {
                entries: 1,
                unpacked: 10,
                failed: false,
                names: vec![(long.clone(), false)],
            },
            cx,
        );
    })
    .unwrap();
    draw(cx, view);
    let shaped = shape::shaped_bytes() - before;
    let drawn = with(cx, view, |v| v.drawn());
    assert!(
        drawn[0].0.contains('\u{2026}') && drawn[2].0.ends_with('\u{2026}'),
        "both are still elided: {} and {}",
        drawn[0].0.len(),
        drawn[2].0.len()
    );
    assert!(
        shaped < 16_384,
        "a 64 KiB name and a 64 KiB entry shape only what the card can hold: {shaped} bytes"
    );
}
