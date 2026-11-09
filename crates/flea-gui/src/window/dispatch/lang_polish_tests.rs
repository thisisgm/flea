use super::super::previewtypes_kinds_tests::Root;
use super::super::previewtypes_tests::{file, local, meta, open_at, pump};
use gpui::TestAppContext;

fn catalog(root: &Root, code: &str) -> std::sync::Arc<flea::i18n::I18n> {
    let locale = root.0.join("locale");
    std::fs::create_dir_all(locale.join("default/i18n")).unwrap();
    std::fs::write(
        locale.join("default/i18n/locales.json"),
        include_str!("../../../../../src/i18n/fixtures/locales.json"),
    )
    .unwrap();
    std::fs::write(locale.join("language"), code).unwrap();
    std::sync::Arc::new(flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: locale.clone(),
        language_file: locale.join("language"),
        catalogs: vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n")],
    }))
}

fn long_kind(cx: &mut TestAppContext, code: &str, kind: &str) {
    cx.executor().allow_parking();
    let root = Root::new(&format!("lang-polish-{code}"));
    std::fs::write(root.0.join("sample.md"), "# Notes\n\n".repeat(120)).unwrap();
    let path = root.0.to_string_lossy().into_owned();
    let (shell, cx) = open_at(cx, &path, Some(14.));
    let catalog = catalog(&root, code);
    shell.update(cx, |shell, cx| {
        shell.apply_locale(
            crate::window::locale::Snapshot {
                revision: 2,
                catalog,
            },
            cx,
        )
    });
    let mut row = file("sample.md", "text-markdown", kind, 0o100644);
    row.size = 9285;
    super::super::test_shell::land(&shell, cx, &path, vec![row], 1);
    let lines = local(&shell, cx, &path);
    meta(&shell, cx, &lines, r#","lines":481"#);
    pump(cx);
    let value = cx
        .debug_bounds("column-preview-value-0")
        .expect("overflow uses the approved full-facts table");
    let face = shell.read_with(cx, |shell, cx| {
        shell.selection_preview(0).unwrap().read(cx).face.clone()
    });
    let width = cx.update(|window, _| {
        crate::g2::status_feedback::caption::measure(
            kind.to_owned().into(),
            face.font,
            face.caption,
            gpui::px(0.),
            window,
        )
    });
    assert!(
        width <= f32::from(value.size.width),
        "the entire kind fits at caption size: {kind}"
    );
    super::assert_fits(&shell, cx, "localized full facts");
}

#[gpui::test]
fn lang_polish_german_markdown_kind_fits_in_full_facts(cx: &mut TestAppContext) {
    long_kind(cx, "de", "Markdown-Dokument");
}

#[gpui::test]
fn lang_polish_french_markdown_kind_fits_in_full_facts(cx: &mut TestAppContext) {
    long_kind(cx, "fr", "document Markdown");
}

#[gpui::test]
fn lang_polish_arabic_compact_tail_keeps_the_separator_beside_the_kind(cx: &mut TestAppContext) {
    let root = Root::new("lang-polish-ar");
    let catalog = catalog(&root, "ar");
    let (shell, cx) = open_at(cx, "/files", Some(14.));
    shell.update(cx, |shell, cx| {
        shell.apply_locale(
            crate::window::locale::Snapshot {
                revision: 2,
                catalog,
            },
            cx,
        )
    });
    let mut row = file("sample.rs", "text-x-rust", "مستند مارك داون", 0o100644);
    row.size = 9285;
    super::super::test_shell::land(&shell, cx, "/files", vec![row], 1);
    let lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, r#","lines":481"#);
    let tail = shell.read_with(cx, |shell, cx| {
        let preview = shell.selection_preview(0).unwrap().read(cx);
        let frame = preview.pending.as_ref().or(preview.drawn.as_ref()).unwrap();
        preview.compact(frame, cx).1
    });
    let bidi = unicode_bidi::BidiInfo::new(&tail, None);
    let paragraph = &bidi.paragraphs[0];
    assert!(
        paragraph.level.is_rtl(),
        "tail follows its placement to the left of the kind in the mirrored window: {tail:?}"
    );
    let drawn = bidi.reorder_line(paragraph, paragraph.range.clone());
    assert!(
        drawn
            .trim_end_matches(['\u{200f}', '\u{2069}'])
            .ends_with(" · "),
        "separator stays beside the kind: {drawn:?}"
    );
    assert!(
        drawn.contains("9.3 kB"),
        "the size stays in LTR order: {drawn:?}"
    );
}
