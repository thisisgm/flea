//! Mounted document scrollbars write the drawn content on either axis without consuming the shared corner.
use super::{code, csv, html, render, surface::Surface, Metrics, Palette, PdfView};
use gpui::{
    font, point, px, size, AppContext, Bounds, Modifiers, MouseButton, Pixels, Render, RenderImage,
    TestAppContext, TestDispatcher, VisualTestContext, WindowHandle,
};
use std::sync::Arc;

fn context() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        gpui_platform::current_platform(true).text_system(),
    )
}

fn frame<T: Render>(cx: &mut TestAppContext, handle: WindowHandle<T>) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
}

fn click<T: Render>(
    cx: &mut TestAppContext,
    handle: WindowHandle<T>,
    horizontal: bool,
    bounds: Bounds<Pixels>,
) {
    let at = if horizontal {
        point(bounds.right() - px(20.), bounds.bottom() - px(3.))
    } else {
        point(bounds.right() - px(3.), bounds.bottom() - px(20.))
    };
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    frame(cx, handle);
}

fn rect() -> Bounds<Pixels> {
    Bounds::new(point(px(0.), px(0.)), size(px(600.), px(400.)))
}

fn pdf(cx: &mut TestAppContext, continuous: bool) -> WindowHandle<PdfView> {
    render::without_threads(|| {
        cx.open_window(size(px(600.), px(400.)), |_, cx| {
            let mut view = PdfView::new(
                continuous,
                Palette {
                    background: crate::palette::Theme::default().background,
                },
                Metrics {
                    gap: 9.,
                    margin: 28.,
                    row_height: 31.,
                },
                cx,
            );
            let mut surface = Surface::new(continuous, 66, 100);
            surface.set_active(true, 0);
            surface.set_path("/document.pdf", 0);
            surface.frame();
            surface.take_commands();
            surface.opening(1);
            surface.set_frame(600., 400., 1., 9.);
            surface.reply(
                render::Reply::Opened {
                    doc: 1,
                    read: 4,
                    sizes: vec![(600., 800.); 4],
                },
                0,
            );
            view.surface = surface;
            if let Some(viewer) = &mut view.viewer {
                viewer.zoom = 2.;
            }
            view
        })
    })
}

#[test]
fn pb_scroll_document_pdf_stack_and_zoomed_page_lanes_move_content() {
    let mut cx = context();
    for continuous in [true, false] {
        let handle = pdf(&mut cx, continuous);
        frame(&mut cx, handle);
        click(&mut cx, handle, false, rect());
        handle
            .update(&mut cx, |view, _, _| {
                assert!(
                    if continuous {
                        view.surface.view_top
                    } else {
                        view.pan.1
                    } > 50.,
                    "the mounted vertical PDF lane scrolls its content"
                );
            })
            .unwrap();
        if !continuous {
            click(&mut cx, handle, true, rect());
            handle
                .update(&mut cx, |view, _, _| {
                    assert!(
                        view.pan.0 > 50.,
                        "the zoomed page pans from its horizontal lane"
                    )
                })
                .unwrap();
        }
    }
}

#[test]
fn pb_scroll_document_csv_grid_has_both_lanes_and_fitted_column_has_none() {
    let mut cx = context();
    for look in [true, false] {
        let handle = cx.open_window(size(px(600.), px(400.)), |_, cx| {
            let face = csv::TableFace { font: font("JetBrainsMono Nerd Font"), caption: px(12.), line: 16., row_padding_x: 14., hairline: 1. };
            let ink = crate::palette::Theme::default().foreground;
            let palette = csv::TablePalette { foreground: ink, bright: ink, muted: ink };
            let mut view = csv::CsvView::new(look, face, palette);
            let line = "a_long_heading,b_long_heading,c_long_heading,d_long_heading,e_long_heading,f_long_heading,g_long_heading,h_long_heading\n";
            let text = line.repeat(100);
            view.set_text("table.csv", &text, cx);
            view
        });
        cx.run_until_parked();
        frame(&mut cx, handle);
        click(&mut cx, handle, false, rect());
        click(&mut cx, handle, true, rect());
        handle
            .update(&mut cx, |view, _, _| {
                if look {
                    assert!(
                        view.scroll.0 > 20. && view.scroll.1 > 20.,
                        "the table's two lanes move the grid: {:?}",
                        view.scroll
                    );
                } else {
                    assert_eq!(
                        view.scroll,
                        (0., 0.),
                        "the fitted column keeps its no-lane recipe"
                    );
                }
            })
            .unwrap();
    }
}

#[test]
fn pb_scroll_document_html_lanes_pan_zoomed_page_and_leave_shared_corner_clear() {
    let mut cx = context();
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
        view.doc.zoom = 2.;
        view
    });
    frame(&mut cx, handle);
    click(&mut cx, handle, false, rect());
    click(&mut cx, handle, true, rect());
    let before = handle
        .update(&mut cx, |view, _, _| {
            assert!(
                view.scroll_x > 50. && view.scroll_y > 50.,
                "the page pans on both axes"
            );
            (view.scroll_x, view.scroll_y)
        })
        .unwrap();
    let at = point(px(597.), px(397.));
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |view, _, _| {
            assert_eq!((view.scroll_x, view.scroll_y), before)
        })
        .unwrap();
    let lane = point(px(597.), px(380.));
    let outside = point(px(800.), px(-200.));
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    visual.simulate_mouse_down(lane, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(outside, Some(MouseButton::Left), Modifiers::none());
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |view, _, _| assert_eq!(view.scroll_y, 0.))
        .unwrap();
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_up(
        outside,
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    // A held move after the release: only a drag the release failed to end could pan here.
    VisualTestContext::from_window(handle.into(), &cx).simulate_mouse_move(
        lane,
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    frame(&mut cx, handle);
    handle
        .update(&mut cx, |view, _, _| {
            assert_eq!(
                view.scroll_y, 0.,
                "the release outside the view ends the drag"
            )
        })
        .unwrap();
}
