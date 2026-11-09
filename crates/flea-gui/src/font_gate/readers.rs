use super::*;
use crate::g2::media_preview::engine::{Backend, Feed, FeedSender, Playback, Request};
use crate::g2::quick_look_overlay::{doc::View as Reader, Body};

struct Decoder;
struct Playing {
    _feed: FeedSender,
}
impl Playback for Playing {
    fn send(&mut self, _: flea::ui_logic_media_preview::frame::Command) {}
    fn pids(&self) -> Vec<u32> {
        Vec::new()
    }
}
impl Backend for Decoder {
    fn start(&self, request: &Request) -> Result<(Box<dyn Playback>, Feed), String> {
        let (sender, feed) = crate::g2::media_preview::engine::feed();
        sender.event(flea::ui_logic_media_preview::frame::Event::Loaded {
            video: !request.audio,
            duration_ms: 20_000,
        });
        Ok((Box::new(Playing { _feed: sender }), feed))
    }
}

pub(super) fn check() -> Vec<String> {
    let mut failed = Vec::new();
    for (name, icon) in [
        ("notes.txt", "text-plain"),
        ("main.rs", "text-x-rust"),
        ("readme.md", "text-markdown"),
        ("page.html", "text-html"),
        ("table.csv", "text-csv"),
        ("report.docx", "x-office-document"),
        ("book.pdf", "application-pdf"),
        ("sample.ttf", "font-x-generic"),
        ("pack.zip", "package-x-generic"),
        ("benchy.stl", "model-stl"),
        ("song.ogg", "audio-ogg"),
        ("clip.mp4", "video-mp4"),
        ("picture.png", "image-png"),
        ("disk.img", "application-octet-stream"),
    ] {
        let (mut cx, recorder) = context();
        cx.executor().allow_parking();
        recorder.begin();
        let handle = open(&mut cx);
        edit(&mut cx, handle, |shell, window, cx| {
            if icon.starts_with("audio") || icon.starts_with("video") {
                shell.panes[0].as_mut().unwrap().pane.held =
                    crate::list::bench::held(vec![flea::protocol::RowFacts {
                        name: "bootstrap.img".into(),
                        icon: "application-octet-stream".into(),
                        ..Default::default()
                    }]);
                assert!(shell.open_quick_look(0, window, cx));
                shell.quick_look.as_mut().unwrap().backend = Some(std::rc::Rc::new(Decoder));
            }
            let backend = channel(cx);
            let host = shell.panes[0].as_mut().unwrap();
            host.backend = Some(backend);
            host.pane.held = crate::list::bench::held(vec![flea::protocol::RowFacts {
                name: name.into(),
                icon: icon.into(),
                size: 64,
                mode: 0o100644,
                ..Default::default()
            }]);
            host.pane.cursor = 0;
            assert!(
                shell.open_quick_look(0, window, cx),
                "{name} must open Quick Look"
            );
            let view = shell.quick_look.as_ref().unwrap().view.clone();
            view.update(cx, |view, cx| {
                let body = view.subject().unwrap().body.clone();
                if let Body::Doc(doc) = body {
                    match doc.view {
                        Reader::Code(code) => code.update(cx, |code, cx| {
                            code.set_host(None, cx);
                            code.set_subject(
                                "/font-gate-sample",
                                23,
                                true,
                                Some("Font gate reader sample".into()),
                                cx,
                            );
                            code.set_active(true, cx);
                        }),
                        Reader::Markdown(markdown) => markdown.update(cx, |markdown, cx| {
                            let root = crate::presentation::fixture::Fixture::open(std::env::var_os("FONT_GATE_ROOT").unwrap().into()).unwrap();
                            let path = root.path().join("reader.md");
                            root.write(&path, "# Font gate heading\n\nFont gate paragraph with **bold text** and `code`.\n");
                            markdown.set_path(Some(path), cx);
                        }),
                        Reader::Table(table) => table.update(cx, |table, cx| {
                            table.set_text(
                                "table.csv",
                                "Font gate column,Second column\nFont gate cell,value\n",
                                cx,
                            )
                        }),
                        Reader::Font(font) => font.update(cx, |font, cx| {
                            let root = crate::presentation::fixture::Fixture::open(std::env::var_os("FONT_GATE_ROOT").unwrap().into()).unwrap();
                            let path = root.path().join("font-sample.png");
                            let mut bytes = Vec::new();
                            {
                                let mut encoder = png::Encoder::new(&mut bytes, 8, 8);
                                encoder.set_color(png::ColorType::Rgba);
                                encoder.set_depth(png::BitDepth::Eight);
                                encoder.write_header().unwrap().write_image_data(&[0; 256]).unwrap();
                            }
                            root.write(&path, bytes);
                            font.answered(flea::ui_logic_doc_preview::font::Reply {
                                id: font.page.pending, family: "Font Gate Specimen".into(), style: "Regular".into(), glyphs: 42,
                                files: vec![path.to_string_lossy().into_owned(); 3], ..Default::default()
                            }, cx);
                        }),
                        Reader::Archive(archive) => archive.update(cx, |archive, cx| {
                            let path = archive.path.clone();
                            assert!(archive.answered(
                                &path,
                                flea::ui_logic_doc_preview::archive::Meta {
                                    entries: 2,
                                    unpacked: 64,
                                    failed: false,
                                    names: vec![
                                        ("Font gate archive entry.txt".into(), false),
                                        ("Folder".into(), true)
                                    ]
                                },
                                cx
                            ));
                        }),
                        _ => {}
                    }
                }
                view.revealed = true;
                cx.notify();
            });
        });
        // Text only the card draws; the PDF, office and model cards draw only their bar until the reader answers, and a picture draws no text.
        let required = match name {
            "notes.txt" | "main.rs" => Some("Font gate reader sample"),
            "readme.md" => Some("Font gate heading"),
            "page.html" => Some("Rendered"),
            "table.csv" => Some("Font gate column"),
            "pack.zip" => Some("Font gate archive entry.txt"),
            "sample.ttf" => Some("Font Gate Specimen"),
            "song.ogg" | "clip.mp4" => Some("0:00 / 0:20"),
            "disk.img" => Some("This file cannot be previewed."),
            _ => None,
        };
        let part = match name {
            "book.pdf" | "report.docx" | "benchy.stl" => "pv1-bar",
            _ => "pv1-card",
        };
        let until = std::time::Instant::now() + std::time::Duration::from_secs(4);
        let drawn = loop {
            draw(&mut cx, handle);
            let drawn = gpui::VisualTestContext::from_window(handle.into(), &cx)
                .debug_bounds(part)
                .is_some();
            if drawn && required.is_none_or(|text| recorder.contains(text)) {
                break true;
            }
            if std::time::Instant::now() >= until {
                break drawn;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let surface = format!("quick-look-{name}");
        let mut mounted = vec![name];
        mounted.extend(required);
        let passed = if drawn {
            assert_runs(&surface, &recorder, &mounted)
        } else {
            println!("FONT_GATE surface={surface} missing={part:?} FAIL");
            false
        };
        if !passed {
            failed.push(surface);
        }
    }
    column(&mut failed);
    failed
}

fn column(failed: &mut Vec<String>) {
    use crate::window::selection_preview::{docs::Doc, Mounted};
    let (mut cx, recorder) = context();
    recorder.begin();
    let handle = open(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        shell.view = crate::window::startup::View::Columns;
        shell.sync_selection_previews(window, cx);
        let preview = shell.selection_preview(0).expect("the real column mounted");
        let directory = shell.panes[0].as_ref().unwrap().pane.path().to_owned();
        preview.update(cx, |preview, _| {
            preview.class = Some((directory, "local".into()))
        });
        shell.sync_selection_previews(window, cx);
        preview.update(cx, |preview, cx| {
            let frame = preview
                .pending
                .as_mut()
                .expect("the selection mounted a reader");
            let Mounted::Doc(Doc::Code(code)) = &frame.mounted else {
                panic!("text must mount CodeView");
            };
            code.update(cx, |code, cx| {
                code.set_host(None, cx);
                code.set_subject(
                    "/column-sample",
                    24,
                    true,
                    Some("Font gate preview column".into()),
                    cx,
                );
                code.set_active(true, cx);
            });
            frame.meta = Some(flea::jsondoc::Json::Obj(Default::default()));
            preview.poll(window, cx);
            cx.notify();
        });
    });
    for _ in 0..10 {
        draw(&mut cx, handle);
        if recorder.contains("Font gate preview column") {
            break;
        }
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
    }
    if !assert_runs("preview-column", &recorder, &["Font gate preview column"]) {
        failed.push("preview-column".into());
    }
}
