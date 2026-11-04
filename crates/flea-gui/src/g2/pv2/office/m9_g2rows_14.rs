//! Acceptance pins for the recovered Office packet and its shared preview contracts.
use super::*;
use crate::g2::pv2::render::Reply;
use flea::ui_logic_pv2::viewer::Control;
use gpui::{font, TestAppContext, WindowHandle};
use std::{cell::Cell, rc::Rc, time::Duration};

struct Backend(Rc<Cell<u64>>);

impl OfficeHost for Backend {
    fn office(&self, _: &str, _: bool) -> u64 {
        let id = self.0.get() + 1;
        self.0.set(id);
        id
    }
}

struct Engine;

impl CodeHost for Engine {
    fn code(&self, _: &dyn Fn(u64) -> String) -> u64 {
        panic!("these wire pins do not lay out a table")
    }
}

// The pins answer the PDF surface by hand, so its renderer starts no thread that could answer from elsewhere.
fn view(cx: &mut TestAppContext) -> WindowHandle<OfficeView> {
    crate::g2::pv2::render::without_threads(|| view_on(cx))
}

fn view_on(cx: &mut TestAppContext) -> WindowHandle<OfficeView> {
    cx.open_window(size(px(902.), px(625.)), |_, cx| {
        let mut view = OfficeView::new(
            true,
            Palette {
                background: gpui::Hsla::default(),
            },
            Metrics {
                gap: 9.,
                margin: 28.,
                row_height: 28.,
            },
            TableFace {
                font: font("JetBrains Mono"),
                caption: px(12.),
                line: 16.,
                row_padding_x: 14.,
                hairline: 1.,
            },
            TablePalette {
                foreground: gpui::Hsla::default(),
                bright: gpui::Hsla::default(),
                muted: gpui::Hsla::default(),
            },
            cx,
        );
        view.set_host(Box::new(Backend(Rc::default())), Box::new(Engine), cx);
        view
    })
}

fn image(width: u32, height: u32) -> Arc<RenderImage> {
    let buffer = image::RgbaImage::from_pixel(width, height, image::Rgba([255; 4]));
    Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
}

fn disabled_zoom_controls(bar: &Bar) -> bool {
    [Control::ZoomIn, Control::ZoomOut]
        .into_iter()
        .all(|control| {
            bar.items
                .iter()
                .any(|(item, _, _)| item.control == control && item.visible && !item.enabled)
        })
}

#[gpui::test]
fn m9_g2rows_14_office_thumbnail_and_ready_page_share_geometry(cx: &mut TestAppContext) {
    for (path, dimensions, frame) in [
        ("/d/garden-report.docx", (210, 297), (442., 625.)),
        ("/d/talk.odp", (160, 90), (902., 625.)),
    ] {
        let view = view(cx);
        // The container check runs off the UI thread, so the ask lands once the test parks.
        view.update(cx, |view, window, cx| view.set_path(path, window, cx))
            .unwrap();
        cx.run_until_parked();
        view.update(cx, |view, window, cx| {
            view.set_thumbnail(path, image(dimensions.0, dimensions.1), cx);
            let thumb = view.thumb_rect(frame).unwrap();
            let bar = view
                .bar(&crate::test_catalogs::english(), cx)
                .strip
                .unwrap();
            assert!(bar.counter.is_none());
            assert!(bar
                .items
                .iter()
                .filter(|(i, _, _)| matches!(i.control, Control::Previous | Control::Next))
                .all(|(i, _, _)| !i.visible));
            assert!(disabled_zoom_controls(&bar));
            let mut missing_zoom = bar.clone();
            missing_zoom
                .items
                .retain(|(item, _, _)| !matches!(item.control, Control::ZoomIn | Control::ZoomOut));
            assert!(!disabled_zoom_controls(&missing_zoom));
            assert!(view.failure(&crate::test_catalogs::english()).is_none());
            assert!(view.reply(
                r#"{"t":"officepdf","id":1,"state":"ready","path":"/c/page.pdf"}"#,
                window,
                cx
            ));
            assert!(!view.shown(cx), "readiness waits for the rendered page");
            view.pdf.update(cx, |pdf, _| {
                let surface = &mut pdf.surface;
                surface.opening(7);
                surface.set_frame(frame.0 - 56., frame.1 - 56., 1., 9.);
                surface.reply(
                    Reply::Opened {
                        doc: 7,
                        sizes: vec![(dimensions.0.into(), dimensions.1.into())],
                        read: 1,
                    },
                    0,
                );
                surface.reply(
                    Reply::Rendered {
                        doc: 7,
                        page: 0,
                        width: surface.render_width,
                        image: image(1, 1),
                        took: Duration::ZERO,
                    },
                    0,
                );
                assert_eq!(
                    pdf.single_rect(),
                    Some(thumb),
                    "thumbnail and ready page occupy the same rectangle"
                );
            });
            assert!(view.shown(cx));
            assert_eq!(view.office.state, State::Ready);
        })
        .unwrap();
    }
}

#[gpui::test]
fn m9_g2rows_14_sheet_switch_and_stale_reply_preserve_identity(cx: &mut TestAppContext) {
    let view = view(cx);
    view.update(cx, |view, window, cx| {
        view.set_path("/d/budget.xlsx", window, cx)
    })
    .unwrap();
    cx.run_until_parked();
    view.update(cx, |view, window, cx| {
        assert!(!view.action("nextSheet", cx));
        assert!(view.reply(r#"{"t":"officesheets","id":1,"state":"ready","names":["Monthly","Savings"],"paths":["/c/monthly.csv","/c/savings.csv"]}"#, window, cx));
        assert_eq!(view.bar(&crate::test_catalogs::english(), cx).sheets, vec![("Monthly".into(), true), ("Savings".into(), false)]);
        view.table.update(cx, |table, _| table.feed.sent(101));
        assert!(view.action("nextSheet", cx));
        assert_eq!(view.office.sheet, 1);
        assert_eq!(view.bar(&crate::test_catalogs::english(), cx).sheets, vec![("Monthly".into(), false), ("Savings".into(), true)]);
        view.table.update(cx, |table, cx| {
            assert!(!table.answered(r#"{"t":"code","token":101,"state":"plain","first":1,"total":1,"lines":[[0,"wrong sheet"]]}"#, cx));
            assert!(table.feed.held.is_empty(), "the previous sheet cannot populate the new table");
        });
        assert!(!view.reply(r#"{"t":"officesheets","id":1,"state":"ready","names":["Wrong"],"paths":["/c/wrong.csv"]}"#, window, cx));
        assert_eq!(view.office.sheet_names, ["Monthly", "Savings"]);
        assert_eq!(view.office.sheet, 1);
        assert!(view.action("nextSheet", cx));
        assert_eq!(view.office.sheet, 0, "the final sheet wraps to the first");
        assert!(view.action("previousSheet", cx));
        assert_eq!(view.office.sheet, 1, "the first sheet wraps back to the last");
        view.set_path("/d/new.xlsx", window, cx);
    }).unwrap();
    cx.run_until_parked();
    view.update(cx, |view, window, cx| {
        assert!(!view.reply(
            r#"{"t":"officesheets","id":1,"state":"ready","names":["Old"],"paths":["/c/old.csv"]}"#,
            window,
            cx
        ));
        assert!(view.office.sheet_names.is_empty());
        assert_eq!(view.office.id, 2);
    })
    .unwrap();
}

#[gpui::test]
fn m9_g2rows_14_locked_unavailable_and_cancel_keep_officepdf_contract(cx: &mut TestAppContext) {
    let view = view(cx);
    let i18n = crate::test_catalogs::english();
    view.update(cx, |view, window, cx| {
        view.set_path("/d/lease.docx", window, cx)
    })
    .unwrap();
    cx.run_until_parked();
    view.update(cx, |view, window, cx| {
        assert!(view.reply(
            r#"{"t":"officepdf","id":1,"state":"failed","err":"encrypted","locked":true}"#,
            window,
            cx
        ));
        let locked = view.failure(&i18n).unwrap();
        assert_eq!(locked.sentence, "This document is password protected.");
        assert_eq!(locked.open_with.as_deref(), Some("Open With…"));
        assert_eq!(locked.glyph, "lock");
        view.set_path("/d/note.odt", window, cx);
    })
    .unwrap();
    cx.run_until_parked();
    view.update(cx, |view, window, cx| {
        assert!(view.reply(r#"{"t":"officepdf","id":2,"state":"failed","err":"LibreOffice is not installed on this box.","unavailable":true}"#, window, cx));
        assert!(matches!(view.office.state, State::Unsupported(_)));
        assert_eq!(view.failure(&i18n).unwrap().sentence, i18n.tr("document.office_unsupported"));
        view.set_path("/d/talk.odp", window, cx);
    }).unwrap();
    cx.run_until_parked();
    view.update(cx, |view, window, cx| {
        view.leave(window, cx);
        assert_eq!(view.office.state, State::Cancelled);
        assert!(view.failure(&i18n).is_none());
        assert!(!view.reply(
            r#"{"t":"officepdf","id":3,"state":"ready","path":"/c/old.pdf"}"#,
            window,
            cx
        ));
        view.set_path("/d/new.docx", window, cx);
    })
    .unwrap();
    cx.run_until_parked();
    view.update(cx, |view, window, cx| {
        assert!(view.reply(
            r#"{"t":"officepdf","id":4,"state":"failed","err":"cancelled"}"#,
            window,
            cx
        ));
        assert_eq!(view.office.state, State::Cancelled);
        assert!(view.failure(&i18n).is_none());
        assert_eq!(
            office::request_line(5, "/d/new.docx", false),
            r#"{"c":"officepdf","id":5,"path":"/d/new.docx"}"#
        );
    })
    .unwrap();
}
