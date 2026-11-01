use super::*;
use crate::{
    backend::testdir::TestDir,
    i18n::{I18n, Paths},
    jsondoc::Json,
    tui::{cells, model::Row, pane, pane_state, theme::Theme},
};

fn language(root: &TestDir, code: &str) -> I18n {
    let catalog = root.join("catalog");
    for lang in ["en", "ja", "ar"] {
        std::fs::create_dir_all(catalog.join(lang)).unwrap();
    }
    std::fs::write(catalog.join("ja/flea.json"), r#"{"preview.loading":"準備中の文書プレビュー","preview.points.folder":"フォルダ","preview.unreadable":"読み取れません","preview.selected":{"other":"%1 選択"},"preview.fact.combined":"合計","preview.fact.modified":"更新日時","document.office_pages":"ページ","document.office_title":"題名","document.office_author":"作成者","media.play":"再生する","media.pause":"一時停止する","pdf.page":"ページ %1 / %2 · %3%"}"#).unwrap();
    std::fs::write(catalog.join("ar/flea.json"), r#"{"preview.fact.modified":"تعديل","preview.fact.combined":"المجموع","preview.selected":{"other":"%1 مختار"},"document.office_title":"العنوان","document.office_author":"المؤلف","pdf.page":"الصفحة %1 من %2 · %3%","media.play":"تشغيل طويل","media.pause":"إيقاف مؤقت طويل","model3d.plate":"اللوحة %1"}"#).unwrap();
    let roster = root.join("omarchy/default/i18n");
    std::fs::create_dir_all(&roster).unwrap();
    std::fs::write(
        roster.join("locales.json"),
        include_str!("../i18n/fixtures/locales.json"),
    )
    .unwrap();
    std::fs::write(catalog.join("en/flea.json"), PRIVATE_ENGLISH).unwrap();
    root.file("language", code);
    I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![catalog],
    })
}
fn model(root: &TestDir, name: &str, icon: &str) -> Model {
    let mut m = Model::new(root.path().into(), &Json::Null);
    m.rows.insert(
        0,
        Row {
            name: name.into(),
            directory: icon == "folder",
            size: 4,
            mode: 0o100644,
            link: String::new(),
            kind: "literal MIME kind".into(),
            thumbnail: false,
            modified: 1750000000,
            birth: None,
            icon: icon.into(),
        },
    );
    m.columns = 100;
    m.height = 25;
    m.preview_visible = true;
    m
}
fn settle(m: &mut Model) {
    let start = std::time::Instant::now();
    while m.preview_reader.outstanding(m.preview_generation) {
        poll(m);
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    poll(m);
}
#[test]
fn tui_preview_words_cover_every_kind_and_failure() {
    let root = TestDir::new("tui13-kinds");
    for (name, icon, kind) in [
        ("folder", "folder", pane_state::Kind::Folder),
        ("image.png", "image-png", pane_state::Kind::Image),
        ("file.pdf", "application-pdf", pane_state::Kind::Pdf),
        ("audio.ogg", "audio-ogg", pane_state::Kind::Audio),
        ("clip.mp4", "video-mp4", pane_state::Kind::Video),
        (
            "archive.zip",
            "package-x-generic",
            pane_state::Kind::Archive,
        ),
        (
            "model.stl",
            "application-x-generic",
            pane_state::Kind::Model3d,
        ),
        ("office.docx", "x-office-document", pane_state::Kind::Office),
        ("data.bin", "application-x-generic", pane_state::Kind::Data),
    ] {
        let mut m = model(&root, name, icon);
        assert_eq!(pane_state::kind(&m.rows[&0]), kind);
        m.i18n = language(&root, "ja");
        refresh_locale(&mut m);
        let generation = m.preview_generation;
        load(&mut m, true);
        assert_eq!(m.preview_path, m.current_path().unwrap(), "{kind:?}");
        assert_eq!(m.preview_generation, generation + 1, "{kind:?}");
        assert!(m.preview_loaded, "{kind:?}");
        let theme = Theme::from_text("");
        let title = crate::tui::render::frame_tests::plain(&pane::preview(&m, 0, 80, 25, &theme));
        assert!(title.contains(name), "{kind:?}: {title}");
        let facts = crate::tui::render::frame_tests::plain(&pane::preview(&m, 1, 80, 25, &theme));
        let expected = if kind == pane_state::Kind::Folder {
            m.i18n.tr("preview.points.folder")
        } else {
            "literal MIME kind".into()
        };
        assert!(facts.contains(&expected), "{kind:?}: {facts}");
        if kind == pane_state::Kind::Folder {
            pane::prepare(&mut m);
            let body =
                crate::tui::render::frame_tests::plain(&pane::preview(&m, 3, 80, 25, &theme));
            assert!(
                body.contains("フォルダ"),
                "folder body must contain its localized load projection: {body}"
            );
        }
        if kind != pane_state::Kind::Data {
            assert!(
                !m.preview_reader.outstanding(m.preview_generation),
                "{kind:?}"
            );
            assert!(m.preview_body.is_empty() && m.error.is_empty(), "{kind:?}");
        }
        if kind == pane_state::Kind::Data {
            settle(&mut m);
            assert!(
                m.preview.iter().any(|s| s == "読み取れません"),
                "{:?}",
                m.preview
            );
        }
    }
    let mut m = model(&root, "x.txt", "text-plain");
    m.preview_state.i18n = language(&root, "ja");
    load(&mut m, true);
    settle(&mut m);
    assert!(m.preview.iter().any(|s| s == "読み取れません"));
    let (_, disconnected) = mpsc::channel();
    m.preview_reader.running = Some((
        m.preview_generation,
        Arc::new(AtomicBool::new(false)),
        disconnected,
    ));
    poll(&mut m);
    assert_eq!(m.preview.last().unwrap(), "Text preview reader stopped.");
    assert_eq!(
        read_text(
            &root.file("oversize.txt", &"x".repeat(TEXT_BYTES as usize + 1)),
            &AtomicBool::new(false)
        )
        .unwrap_err(),
        "preview.tooLarge"
    );
    assert_eq!(
        read_text(&root.join("oversize.txt"), &AtomicBool::new(true)).unwrap_err(),
        "tui.preview.cancelled"
    );
    let mut archive = model(&root, "bundle.zip", "package-x-generic");
    archive.preview_path = archive.current_path().unwrap();
    archive.i18n = language(&root, "ja");
    pane_state::meta(
        &mut archive,
        &crate::jsondoc::parse(r#"{"afailed":true}"#).unwrap(),
    );
    refresh_locale(&mut archive);
    assert_eq!(
        archive.preview_state.unshown.as_ref().unwrap().sentence,
        archive.i18n.tr("preview.archiveUnreadable")
    );
    use flea::ui_logic_pv2::office::{Reason, State as OfficeState};
    for (state, expected) in [
        (OfficeState::Converting, None),
        (OfficeState::Cancelled, None),
        (OfficeState::Locked, Some("document.office_locked")),
        (
            OfficeState::Unsupported(Reason::NotOffice),
            Some("document.office_unsupported"),
        ),
        (
            OfficeState::Failed(Reason::NoPdf),
            Some("document.office_failed"),
        ),
    ] {
        archive.preview_state.office.doc.state = state;
        crate::tui::office::project(&mut archive);
        assert_eq!(archive.preview_state.unshown_key, expected);
        match expected {
            Some(key) => {
                let unshown = archive
                    .preview_state
                    .unshown
                    .as_ref()
                    .expect("Office failure must draw its diagnostic");
                assert_eq!(unshown.sentence, archive.i18n.tr(key));
                assert!(unshown.open_with);
            }
            None => assert!(archive.preview_state.unshown.is_none()),
        }
    }
    let mut media = crate::tui::media::Player::resting(
        std::path::Path::new("/absent/clip"),
        crate::tui::graphics::Protocol::None,
        false,
        (30, 10, 1, 1),
        (100, 30),
        (0, 0),
    );
    media.refresh_locale(&archive.i18n);
    assert!(media
        .line(40, true, &archive.i18n)
        .contains(&archive.i18n.tr("media.loading")));
    media.probe(10_000);
    for paused in [true, false] {
        media.paused = paused;
        assert!(media
            .line(60, true, &archive.i18n)
            .contains(
                &archive
                    .i18n
                    .tr(if paused { "media.play" } else { "media.pause" })
            ));
    }
    let refusal = crate::jsondoc::parse(r#"{"failed":true,"mode":16384}"#).unwrap();
    peeked(&mut archive, &refusal);
    assert_eq!(archive.preview_refusal, archive.i18n.tr("pane.locked"));
    let fault = words::issue("tui.preview.worker_limit", "legacy diagnostic transport");
    words::fail(
        &mut archive,
        Text::Args("preview.imageError", vec![words::from_io(&fault)]),
    );
    assert!(archive
        .error
        .contains("Preview cancelled or exceeded its limit"));
    assert!(!archive.error.contains("legacy diagnostic transport"));
}
#[test]
fn tui_preview_words_keep_file_content_literal() {
    let root = TestDir::new("tui13-content");
    let body = "Title · Spring report 2025\npreview.loading\nPlay Pages Author\nتقرير 2025.pdf\n";
    root.file("note.txt", body);
    let mut m = model(&root, "note.txt", "text-plain");
    load(&mut m, true);
    settle(&mut m);
    let generation = m.preview_generation;
    let path = m.preview_path.clone();
    m.i18n = language(&root, "ja");
    refresh_locale(&mut m);
    assert_eq!(m.preview_body, body);
    assert_eq!(m.preview_generation, generation);
    assert_eq!(m.preview_path, path);
    assert_eq!(m.preview_state.i18n.language(), "ja");
    assert!(!m.preview_reader.outstanding(generation));
    let mut model3d = model(&root, "kit.3mf", "application-x-generic");
    model3d.preview_path = model3d.current_path().unwrap();
    let raw = flea::ui_logic_pv4::wire::Data {
        format: "3mf".into(),
        facts: flea::ui_logic_pv4::wire::Facts {
            dimensions_mm: Some([110., 96., 40.]),
            triangles: Some(1204),
            objects: Some(2),
            plates: Some(2),
            ..Default::default()
        },
        slicer: flea::ui_logic_pv4::wire::Slicer {
            printer: Some("Printer · Pages %1".into()),
            filament: Some("تقرير PLA 2025".into()),
            ..Default::default()
        },
        plates: vec![Default::default(), Default::default()],
        ..Default::default()
    };
    model3d.model3d.data = Some(Box::new(raw.clone()));
    let address = model3d
        .model3d
        .data
        .as_ref()
        .map(|data| data.as_ref() as *const _);
    for code in ["en", "ja", "ar", "en"] {
        model3d.i18n = language(&root, code);
        refresh_locale(&mut model3d);
        assert_eq!(
            model3d
                .model3d
                .data
                .as_ref()
                .map(|data| data.as_ref() as *const _),
            address
        );
        assert_eq!(model3d.model3d.data.as_ref().unwrap().as_ref(), &raw);
        assert_eq!(model3d.model3d.renders, 0);
        assert!(model3d
            .preview_metadata
            .iter()
            .any(|line| line.ends_with("Printer · Pages %1")));
        assert!(model3d
            .preview_metadata
            .iter()
            .any(|line| line.ends_with("تقرير PLA 2025")));
    }
    let (mut wire, reader) = crate::tui::wire::echo_wire();
    let mut movie = model(&root, "clip.mp4", "video-mp4");
    movie.preview_path = movie.current_path().unwrap();
    movie.preview_requested = movie.preview_generation;
    movie.i18n = language(&root, "ja");
    refresh_locale(&mut movie);
    let reply = crate::jsondoc::parse(&format!(
        r#"{{"t":"meta","row":0,"token":{},"w":1920,"h":1080,"ms":72000,"rate":48000}}"#,
        movie.preview_requested
    ))
    .unwrap();
    movie.receive(reply, &mut wire).unwrap();
    assert!(movie
        .preview_metadata
        .iter()
        .any(|line| line == "Dimensions · 1,920 × 1,080"));
    assert_eq!(movie.media_probe, Some((1920, 1080, 72000)));
    drop(wire);
    let _ = reader.join();
}
#[test]
fn tui_preview_locale_revision_invalidates_labels_and_extents() {
    let root = TestDir::new("tui13-revision");
    let mut m = model(&root, "note.txt", "text-plain");
    root.file("note.txt", "body retained while reading");
    load(&mut m, true);
    pane::prepare(&mut m);
    let old = m.preview_display.clone();
    m.i18n = language(&root, "ja");
    refresh_locale(&mut m);
    assert_ne!(m.preview_display, old);
    assert_eq!(m.preview_state.i18n.language(), "ja");
    assert_eq!(
        m.preview_width,
        crate::tui::layout::preview_area(m.columns, false).1
    );
    let stable = m.preview_display.clone();
    refresh_locale(&mut m);
    assert_eq!(m.preview_display, stable);
    let before = m.locale_revision;
    root.file("language", "ar");
    assert!(m.refresh_locale(&[refresh_locale]));
    assert_eq!(m.locale_revision, before + 1);
    assert_eq!(m.preview_state.i18n.language(), "ar");
    assert_eq!(m.prompts.i18n.language(), "ar");
    let frozen = m.preview_display.clone();
    root.file("language", "ja");
    assert_eq!(m.preview_state.i18n.language(), "ar");
    assert_eq!(m.preview_display, frozen);
    assert!(m.refresh_locale(&[refresh_locale]));
    assert!(!m.refresh_locale(&[refresh_locale]));
    let generation = m.preview_generation;
    let mut media = crate::tui::media::Player::resting(
        std::path::Path::new("/absent/clip"),
        crate::tui::graphics::Protocol::None,
        false,
        (30, 10, 1, 1),
        (100, 30),
        (0, 0),
    );
    media.probe(10_000);
    m.player = Some(media);
    for code in ["en", "ja", "ar", "en"] {
        m.i18n = language(&root, code);
        refresh_locale(&mut m);
        let p = m.player.as_ref().unwrap();
        let width = p.play_width();
        let expected = ["media.play", "media.pause"]
            .into_iter()
            .map(|key| cells::text_width(&m.i18n.tr(key)))
            .max()
            .unwrap()
            .max(5)
            + 2;
        assert_eq!(width, expected);
        for columns in [0, 1, 5, 12, 23, 40, 60] {
            assert!(cells::text_width(&p.line(columns, true, &m.i18n)) <= columns);
        }
        assert!(p.at_rest());
        assert_eq!(m.preview_generation, generation);
    }
    let source = root.file("held.pdf", "%PDF-1.4\n");
    let mut pdf = crate::tui::pdf::Pdf::new(
        source.clone(),
        crate::tui::graphics::Protocol::None,
        20,
        10,
        (0, 0),
        "#101315",
        false,
    );
    pdf.title = "Spring report 2025".into();
    pdf.author = "Sam Ellis".into();
    pdf.pages = 3;
    pdf.bytes = vec![1, 2, 3];
    pdf.png = vec![4, 5, 6];
    pdf.revision = 9;
    m.player = None;
    m.pdf = Some(pdf);
    m.rows.get_mut(&0).unwrap().name = "office.docx".into();
    m.preview_path = m.current_path().unwrap();
    for code in ["en", "ja", "ar", "en"] {
        m.i18n = language(&root, code);
        refresh_locale(&mut m);
        let pdf = m.pdf.as_ref().unwrap();
        assert_eq!(
            (
                pdf.source.clone(),
                pdf.revision,
                pdf.bytes.clone(),
                pdf.png.clone()
            ),
            (source.clone(), 9, vec![1, 2, 3], vec![4, 5, 6])
        );
        assert_eq!(
            m.preview_metadata
                .iter()
                .filter(|line| line.contains("Spring report 2025"))
                .count(),
            1
        );
        assert!(m
            .preview_metadata
            .iter()
            .any(|line| line == &format!("{} · Sam Ellis", m.i18n.tr("document.office_author"))));
    }
}
#[test]
fn tui_preview_mixed_rtl_facts_stay_in_columns() {
    let root = TestDir::new("tui13-rtl");
    let mut m = model(&root, "تقرير 2025.pdf", "application-pdf");
    m.preview_path = m.current_path().unwrap();
    m.i18n = language(&root, "ar");
    refresh_locale(&mut m);
    let theme = Theme::from_text("");
    let fact = words::Fact {
        key: "document.office_title",
        value: "تقرير 2025.pdf\u{202e}\x1b[31m".into(),
        number: false,
    };
    let line = fact.paint(&m.preview_state.i18n, 40);
    assert_eq!(cells::text_width(&line), 40);
    assert!(line.contains('\u{2068}'));
    assert!(!line.contains('\u{202e}'));
    assert!(!line.contains('\x1b'));
    let number = words::Fact {
        key: "document.office_pages",
        value: "123".into(),
        number: true,
    }
    .paint(&m.preview_state.i18n, 30);
    assert!(number.contains("\u{2066}123"));
    let original_name = m.rows[&0].name.clone();
    m.rows.get_mut(&0).unwrap().name = "تقرير 2025.3mf".into();
    m.preview_path = m.current_path().unwrap();
    m.model3d.data = Some(Box::new(flea::ui_logic_pv4::wire::Data {
        plates: vec![Default::default(); 10],
        ..Default::default()
    }));
    m.model3d.controls.plate = 9;
    for columns in [32, 70, 130] {
        m.columns = columns;
        m.height = 21;
        m.quicklook = true;
        let card = crate::tui::look::card(&m, &theme, columns);
        let area = crate::tui::look::area(columns, m.height);
        assert_eq!(
            cells::text_width(&crate::tui::render::frame_tests::plain(&card)),
            (area.columns + 2) * m.height
        );
        if columns >= 70 {
            assert!(card.contains(&m.i18n.tr_args("model3d.plate", &["10"])));
        }
        m.quicklook = false;
    }
    m.rows.get_mut(&0).unwrap().name = original_name;
    m.preview_path = m.current_path().unwrap();
    for width in [12, 23, 42, 80] {
        let title = pane::preview(&m, 0, width, 20, &theme);
        assert_eq!(
            cells::text_width(&crate::tui::render::frame_tests::plain(&title)),
            width
        );
        assert!(title.contains("\x1b[1m\u{2068}"), "{title:?}");
        let facts = pane::preview(&m, 1, width, 20, &theme);
        if width >= 42 {
            assert!(facts.contains('\u{2066}'), "{facts:?}");
        }
    }
}

// This explicit test overlay is the private catalog delta; production catalogs stay owned by I50.
const PRIVATE_ENGLISH: &str = r#"{"tui.preview.reader_stopped": "Preview reader stopped", "tui.preview.changed": "Selected item changed", "tui.preview.cancelled": "Preview cancelled", "tui.preview.worker_limit": "Preview cancelled or exceeded its limit", "tui.preview.rectangle": "Preview rectangle exceeds its limit", "tui.preview.socket_create": "Could not create preview socket directory", "tui.preview.socket_sandbox": "Preview socket directory is outside its sandbox", "tui.preview.media_waiting": "Media preview is still loading", "tui.preview.media_start": "mpv could not start the inline preview", "tui.preview.media_stopped": "Media preview stopped", "tui.preview.media_error": "Media preview: %1", "tui.preview.no_pixels": "Inline preview unavailable: terminal did not report pixel dimensions", "tui.preview.text_reader_stopped": "Text preview reader stopped."}"#;

#[test]
fn tui_preview_header_uses_current_locale() {
    let root = TestDir::new("tui13-size-header");
    root.file("note.txt", &"a".repeat(1536));
    let mut m = model(&root, "note.txt", "text-plain");
    m.rows.get_mut(&0).unwrap().size = 1536;
    m.i18n = language(&root, "en");
    refresh_locale(&mut m);
    load(&mut m, true);
    settle(&mut m);
    let theme = Theme::from_text("");
    let before = crate::tui::render::frame_tests::plain(&pane::preview(&m, 1, 80, 25, &theme));
    assert!(before.contains("1.5 kB"), "{before}");
    let generation = m.preview_generation;
    let body = m.preview_body.clone();
    m.i18n = language(&root, "fr");
    refresh_locale(&mut m);
    let after = crate::tui::render::frame_tests::plain(&pane::preview(&m, 1, 80, 25, &theme));
    assert!(
        after.contains("1,5 kB") && !after.contains("1.5 kB"),
        "{after}"
    );
    assert_eq!(m.preview_body, body);
    assert_eq!(m.preview_generation, generation);
}
