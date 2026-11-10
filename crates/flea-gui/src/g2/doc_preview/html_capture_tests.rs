//! The HTML capture reports an image only for the document whose page was painted.
use super::{code, html, Palette};
use gpui::{font, px, size, AppContext, RenderImage, TestAppContext, TestDispatcher, WindowHandle};
use std::sync::Arc;

fn frame(cx: &mut TestAppContext, handle: WindowHandle<html::HtmlView>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

#[test]
fn html_capture_drawn_resets_when_a_new_document_loads() {
    let mut cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    );
    let handle = cx.open_window(size(px(600.), px(400.)), |_, cx| {
        let ink = crate::palette::Theme::default().foreground;
        let parts = html::SourceParts {
            typeface: code::Typeface {
                font: font("monospace"),
                body: px(14.),
                caption: px(12.),
            },
            palette: code::Palette {
                foreground: ink,
                muted: ink,
                muted_frame: ink,
                gutter: ink,
                inks: [ink; 7],
            },
            metrics: code::Metrics {
                inset: 14.,
                top: 41.,
                right: 20.,
                row: 20.,
                gap: 9.,
                state_mark: 32.,
            },
        };
        let mut view = html::HtmlView::new(
            "look",
            Palette {
                background: crate::palette::Theme::default().background,
            },
            html::Metrics {
                row_height: 31.,
                radius: 0.,
                round_top: false,
            },
            parts,
            ("file".into(), b"<svg/>"),
            cx,
        );
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::new(12, 32),
        )]));
        view.image = Some((1, image, (1200, 3200)));
        view
    });
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |view, _, _| {
            assert_eq!(
                view.captured_image.get(),
                (true, true),
                "the page was painted"
            )
        })
        .unwrap();
    handle
        .update(&mut cx, |view, _, cx| {
            view.set_path("/other/page.html", 64, false, cx);
        })
        .unwrap();
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |view, _, _| {
            assert_eq!(
                view.captured_image.get(),
                (false, false),
                "a new document carries no earlier page's drawn flag"
            )
        })
        .unwrap();
}
