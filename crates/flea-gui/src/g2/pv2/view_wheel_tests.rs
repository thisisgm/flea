//! EY-54: a wheel notch moves the PDF stack as far as the list, 3 lines of 24px times 4, the same 288px.
use super::*;
use crate::g2::pv2::render::{self, without_threads};
use gpui::{point, AppContext as _, ScrollDelta, TestAppContext};

// A continuous stack of four US Letter pages, opened and laid out in a 612 x 400 frame.
fn four_pages() -> Surface {
    let mut surface = Surface::new(true, 66, 100);
    surface.set_active(true, 0);
    surface.set_path("/a.pdf", 0);
    surface.frame();
    surface.take_commands();
    surface.opening(1);
    surface.set_frame(612., 400., 1., 9.);
    let sizes = vec![(612., 792.); 4];
    let read = sizes.len();
    surface.reply(
        render::Reply::Opened {
            doc: 1,
            read,
            sizes,
        },
        0,
    );
    surface
}

#[gpui::test]
fn a_wheel_notch_moves_the_pdf_stack_as_far_as_the_list(cx: &mut TestAppContext) {
    let view = without_threads(|| {
        cx.new(|cx| {
            let metrics = Metrics {
                gap: 8.,
                margin: 16.,
                row_height: 31.,
            };
            let palette = Palette {
                background: crate::palette::Theme::default().background,
            };
            let mut view = PdfView::new(true, palette, metrics, cx);
            view.surface = four_pages();
            view
        })
    });
    view.update(cx, |view, cx| {
        assert_eq!(view.surface.view_top, 0.);
        let notch = ScrollWheelEvent {
            delta: ScrollDelta::Lines(point(0., -3.)),
            ..Default::default()
        };
        view.wheel(&notch, cx);
        assert_eq!(view.surface.view_top, 288., "one notch is the list's 288px");
    });
}
