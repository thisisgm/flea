//! A locale snapshot relabels the mounted preview without reopening or moving its document.
use super::{
    preview_tests::{body, close},
    quick_look_body_tests::{mounted, open_kind},
};
use crate::{
    g2::{
        doc_preview::markdown::{Status, View},
        quick_look_overlay::{doc::Chrome, Body},
    },
    state::Shell,
    window::locale::Snapshot,
};
use flea::i18n::{I18n, Paths};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{fs, path::PathBuf, process::Command, sync::Arc};

struct Catalogs(PathBuf);

impl Catalogs {
    fn new() -> Self {
        let made = Command::new("mktemp")
            .args(["-d", "/tmp/flea-live-md-chrome.XXXXXX"])
            .output()
            .unwrap();
        assert!(made.status.success());
        let root = PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
        assert_eq!(root.parent(), Some(std::path::Path::new("/tmp")));
        assert!(root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("flea-live-md-chrome."));
        fs::write(root.join(".flea-test-sandbox"), b"live-md-chrome\n").unwrap();
        fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
        fs::write(
            root.join("omarchy/default/i18n/locales.json"),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../src/i18n/fixtures/locales.json"
            )),
        )
        .unwrap();
        for (code, rendered, source, lines) in [
            ("ja", "整形表示", "ソース", "%1 行"),
            ("de", "Gerendert", "Quelltext", "%1 Zeilen"),
            ("fr", "Rendu", "Source", "%1 lignes"),
            ("ar", "منسّق", "المصدر", "%1 سطر"),
        ] {
            let dir = root.join("catalogs").join(code);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("flea.json"),
                format!(
                    r#"{{"document.markdown_rendered":"{rendered}","document.markdown_source":"{source}","document.markdown_lines":{{"other":"{lines}"}},"document.markdown_unreadable":"{code} unreadable","document.view_rendered":"{rendered}","document.view_source":"{source}","document.line_count":{{"other":"{lines}"}}}}"#
                ),
            )
            .unwrap();
        }
        Self(root)
    }

    fn load(&self) -> I18n {
        I18n::load(Paths {
            omarchy: self.0.join("omarchy"),
            language_file: self.0.join("language"),
            catalogs: vec![self.0.join("catalogs")],
        })
    }

    fn switch(&self, catalog: &mut I18n, code: &str) {
        fs::write(self.0.join("language"), code).unwrap();
        assert!(catalog.poll_changed());
        assert_eq!(catalog.locale().code, code);
    }
}

impl Drop for Catalogs {
    fn drop(&mut self) {
        assert_eq!(self.0.parent(), Some(std::path::Path::new("/tmp")));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("flea-live-md-chrome."));
        assert_eq!(
            fs::read(self.0.join(".flea-test-sandbox")).unwrap(),
            b"live-md-chrome\n"
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn chrome(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Chrome {
    shell.read_with(cx, |shell, cx| {
        let look = shell.quick_look.as_ref().unwrap().view.read(cx);
        look.subject()
            .unwrap()
            .body
            .doc()
            .unwrap()
            .chrome(&look.i18n, cx)
    })
}

fn apply(shell: &Entity<Shell>, cx: &mut VisualTestContext, catalog: &I18n) {
    shell.update(cx, |shell, cx| {
        shell.apply_locale(
            Snapshot {
                revision: shell.locale.revision + 1,
                catalog: Arc::new(catalog.clone()),
            },
            cx,
        );
    });
    cx.run_until_parked();
}

#[gpui::test]
fn sol_live_md_chrome_markdown_relabels_without_reloading_or_scrolling(cx: &mut TestAppContext) {
    let catalogs = Catalogs::new();
    let mut catalog = catalogs.load();
    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "readme.md");
    let Some(Body::Doc(doc)) = body(&shell, cx) else {
        panic!("mounted document")
    };
    let crate::g2::quick_look_overlay::doc::View::Markdown(markdown) = &doc.view else {
        panic!("Markdown view")
    };
    let text: Arc<str> = "A document line\n".repeat(481).into();
    markdown.update(cx, |view, _| {
        view.doc.status = Status::Ready;
        view.doc.complete = true;
        view.doc.shown = true;
        view.doc.text = Some(text.clone());
        view.doc.lines = 481;
        view.doc.height = 4000.;
        view.source.set_text(
            text.clone(),
            Arc::new(crate::g2::doc_preview::markdown::line_starts(&text)),
        );
        view.wrap = true;
        view.close_focused = true;
    });
    let seq = markdown.read_with(cx, |view, _| view.doc.seq);
    for shown in [View::Rendered, View::Source] {
        markdown.update(cx, |view, _| view.view = shown);
        super::preview_tests::draw(cx);
        markdown.update(cx, |view, _| {
            view.scroll = 270.;
            view.source.top = 13;
            view.source.offset = 7.5;
        });
        for code in ["ja", "de", "fr", "ar", "en"] {
            catalogs.switch(&mut catalog, code);
            apply(&shell, cx, &catalog);
            let bar = chrome(&shell, cx);
            assert_eq!(
                bar.words,
                vec![
                    (
                        catalog.tr("document.markdown_rendered"),
                        shown == View::Rendered
                    ),
                    (
                        catalog.tr("document.markdown_source"),
                        shown == View::Source
                    ),
                ],
                "{code}: the mounted bar uses the new catalog"
            );
            assert_eq!(bar.tail, Some(catalog.trn("document.markdown_lines", 481)));
            assert!(bar.close_focused);
            markdown.read_with(cx, |view, _| {
                assert_eq!(view.scroll, 270.);
                assert_eq!((view.source.top, view.source.offset), (13, 7.5));
                assert_eq!(view.view, shown);
                assert!(view.wrap);
                assert_eq!(view.doc.seq, seq);
                assert!(Arc::ptr_eq(view.doc.text.as_ref().unwrap(), &text));
                assert_eq!(
                    view.words.unreadable,
                    catalog.tr("document.markdown_unreadable")
                );
            });
            let Some(Body::Doc(current)) = body(&shell, cx) else {
                panic!("document retained")
            };
            assert_eq!(current.id(), doc.id());
        }
    }
    close(&shell, cx);
}

#[gpui::test]
fn sol_live_md_chrome_code_and_html_bars_follow_the_host_catalog(cx: &mut TestAppContext) {
    use crate::g2::quick_look_overlay::doc::View as Mounted;
    use flea::ui_logic_doc_preview::{code_view::Answer, html::View as Html};

    let catalogs = Catalogs::new();
    let (shell, cx, _requests, _script) = mounted(cx);
    for name in ["notes.txt", "main.rs", "page.html"] {
        open_kind(&shell, cx, name);
        let Some(Body::Doc(doc)) = body(&shell, cx) else {
            panic!("mounted document")
        };
        match &doc.view {
            Mounted::Code(view) => view.update(cx, |view, _| {
                view.lines.answer = Answer::Plain;
                view.lines.total = 481;
            }),
            Mounted::Html(view) => view.update(cx, |view, _| {
                view.doc.view = Html::Source;
                let image = gpui::RenderImage::from_bgra(2, 8, vec![255; 64]).unwrap();
                view.image = Some((1, Arc::new(image), (2, 8)));
                view.scroll_y = 270.;
            }),
            _ => panic!("text or HTML"),
        }
        let mut catalog = catalogs.load();
        for code in ["de", "ar", "en"] {
            catalogs.switch(&mut catalog, code);
            apply(&shell, cx, &catalog);
            let bar = chrome(&shell, cx);
            match &doc.view {
                Mounted::Code(_) => {
                    assert_eq!(bar.tail, Some(catalog.trn("document.line_count", 481)))
                }
                Mounted::Html(view) => {
                    assert_eq!(
                        bar.words,
                        vec![
                            (catalog.tr("document.view_rendered"), false),
                            (catalog.tr("document.view_source"), true),
                        ]
                    );
                    assert_eq!(view.read_with(cx, |view, _| view.scroll_y), 270.);
                }
                _ => unreachable!(),
            }
            let Some(Body::Doc(current)) = body(&shell, cx) else {
                panic!("document retained")
            };
            assert_eq!(current.id(), doc.id());
        }
        cx.simulate_keystrokes("escape");
        super::preview_tests::draw(cx);
    }
    close(&shell, cx);
}

#[gpui::test]
fn sol_live_md_chrome_shown_and_pending_columns_refresh_cached_words(cx: &mut TestAppContext) {
    use crate::window::selection_preview::{
        docs::Doc, Frame, Kind, Mounted, Port, SelectionPreview, Subject,
    };
    use gpui::AppContext;

    let catalogs = Catalogs::new();
    let mut catalog = catalogs.load();
    let (shell, cx, _requests, _script) = mounted(cx);
    open_kind(&shell, cx, "readme.md");
    let Some(Body::Doc(doc)) = body(&shell, cx) else {
        panic!("mounted document")
    };
    let crate::g2::quick_look_overlay::doc::View::Markdown(markdown) = doc.view else {
        panic!("Markdown view")
    };
    markdown.update(cx, |view, _| view.scroll = 270.);
    let preview = cx.new(|cx| {
        SelectionPreview::new(
            Port::default(),
            Arc::new(catalog.clone()),
            crate::list::Typeface::omarchy_default(),
            cx,
        )
    });
    preview.update(cx, |preview, _| {
        preview.pending = Some(Frame {
            subject: Subject {
                directory: "/files".into(),
                listing: 1,
                index: 0,
                row: super::preview_tests::file("readme.md", "text-markdown"),
                selected: Vec::new(),
                selected_count: 1,
            },
            generation: 1,
            kind: Kind::Markdown,
            mounted: Mounted::Doc(Doc::Markdown(markdown.clone())),
            meta: None,
            facts: None,
            thumb_answered: false,
            thumb_observation: None,
            source: None,
            image: None,
            animation: None,
            animation_source: None,
            quality: Default::default(),
            trace: None,
            failed: false,
            manual: false,
            remote: false,
            _watch: None,
        });
    });
    shell.update(cx, |shell, _| {
        shell.quick_look = None;
        shell.panes[0].as_mut().unwrap().preview = Some(preview.clone());
    });
    for code in ["ja", "de"] {
        catalogs.switch(&mut catalog, code);
        apply(&shell, cx, &catalog);
        markdown.read_with(cx, |view, _| {
            assert_eq!(
                view.words.unreadable,
                catalog.tr("document.markdown_unreadable")
            );
            assert_eq!(view.scroll, 270.);
        });
        preview.update(cx, |preview, cx| {
            assert!(Arc::ptr_eq(
                &preview.catalog,
                &shell.read(cx).locale.catalog
            ));
            if preview.pending.is_some() {
                preview.drawn = preview.pending.take();
            }
        });
    }
    close(&shell, cx);
}
