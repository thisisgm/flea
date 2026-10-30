//! A short real Destination exposes vertical containment inside padding, clear of the later-painted border.
use crate::{
    g2::op1::{
        field, fix11_paint_tests, host::Overlay, model::CopyFocus, model::Dialog,
        palette::CardPalette,
    },
    list::Typeface,
    palette::Theme,
};
use gpui::{div, prelude::*, px, relative, size, AppContext, Bounds, Entity, Pixels};
use image::RgbaImage;
use std::sync::Arc;

struct VerticalProbe {
    overlay: Entity<Overlay>,
    height: f32,
}

impl gpui::Render for VerticalProbe {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        self.overlay.update(cx, |view, cx| {
            let now = view.now(cx);
            view.progress.observe(&view.model, now);
            view.progress.commit(now);
            view.sync_focus(window, cx);
        });
        let face = Typeface::for_window(window, cx);
        let palette = self.overlay.read(cx).palette;
        let focus = self.overlay.read(cx).focus_handle().unwrap();
        let overlay = self.overlay.clone();
        let slot = div()
            .absolute()
            .left(px(32.))
            .top(px(64. - self.height / 2.))
            .w(px(240.))
            .h(px(self.height))
            .px(px(12.))
            .py(px(3.))
            .flex()
            .items_center()
            .border_1()
            .border_color(palette.accent_frame)
            .bg(palette.background);
        div()
            .size_full()
            .relative()
            .bg(palette.background)
            .font_family(face.font.family.clone())
            .text_size(face.body)
            .line_height(relative(flea::ui_logic_op1::card::LINE))
            .text_color(palette.foreground)
            .track_focus(&focus)
            .on_key_down(move |event, window, cx| {
                overlay.update(cx, |view, cx| view.key(event, window, cx));
            })
            .child(field::mount(self.overlay.clone(), slot, &face))
    }
}

fn capture(
    height: f32,
    moving: bool,
    base: f32,
    width: f32,
    blurred: bool,
) -> (RgbaImage, Bounds<Pixels>) {
    let mut app = fix11_paint_tests::app();
    let handle = app
        .open_window(size(px(width), px(700.)), |window, cx| {
            window.set_scale_factor(1.);
            window.set_rem_size(px(base));
            let palette = CardPalette::from_theme(&window.text_style(), *cx.global::<Theme>(), 8.);
            let overlay = cx.new(|cx| {
                let mut view = Overlay::new(
                    Arc::new(crate::test_catalogs::english()),
                    palette,
                    "default",
                );
                view.copy_to(71, moving, &format!("/{}", "Hgjpq/".repeat(80)), cx);
                view
            });
            cx.new(|_| VerticalProbe { overlay, height })
        })
        .unwrap();
    app.run_until_parked();
    app.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        if blurred {
            assert!(window.dispatch_keystroke(gpui::Keystroke::parse("tab").unwrap(), cx));
            window.refresh();
            window.draw(cx).clear(cx);
        }
    })
    .unwrap();
    let bounds = handle
        .update(&mut app, |probe, window, cx| {
            let view = probe.overlay.read(cx);
            let Some(Dialog::CopyTo { focus, .. }) = &view.model.dialog else {
                panic!("the genuine boundary fixture must retain the real dialog");
            };
            assert_eq!(
                *focus,
                if blurred {
                    CopyFocus::Cancel
                } else {
                    CopyFocus::Field
                }
            );
            assert_eq!(view.editing_copy(window, cx), !blurred);
            let paint = view.editor.drawn.borrow();
            let paint = paint.paint.as_ref().unwrap();
            assert_eq!(paint.mask, paint.bounds);
            paint.bounds
        })
        .unwrap();
    let image = app
        .capture_screenshot(handle.into())
        .expect("actual WGPU boundary glyphs");
    assert_eq!(image.dimensions(), ((width * 2.) as u32, 1400));
    if let Some(directory) = std::env::var_os("FLEA_G76_CAPTURE_DIR") {
        image
            .save(std::path::Path::new(&directory).join(format!(
            "vertical-height-{height}-move-{moving}-base-{base}-width-{width}-blurred-{blurred}.png"
        )))
            .unwrap();
    }
    (image, bounds)
}

fn foreground(image: &RgbaImage, x: u32, y: u32) -> bool {
    let rgba = image.get_pixel(x, y).0;
    [198u8, 179, 149]
        .iter()
        .zip(&rgba[..3])
        .all(|(a, b)| a.abs_diff(*b) <= 5)
}

fn check(complete_guard: bool) {
    for moving in [false, true] {
        for base in [12., 14., 16.] {
            for width in [1100., 360.] {
                for blurred in [false, true] {
                    let (reference, reference_bounds) = capture(40., moving, base, width, blurred);
                    let (clipped, bounds) = capture(12., moving, base, width, blurred);
                    let (left, right, top, bottom) = (
                        f32::from(bounds.left()).floor() as u32,
                        f32::from(bounds.right()).ceil() as u32,
                        f32::from(bounds.top()).floor() as u32,
                        f32::from(bounds.bottom()).ceil() as u32,
                    );
                    assert_eq!(bounds.center(), reference_bounds.center());
                    assert!(
                        reference_bounds.top() < bounds.top()
                            && reference_bounds.bottom() > bounds.bottom()
                    );
                    assert!(top > 3 && left > 3);
                    assert!((left..right).any(|x| foreground(&reference, x, top - 1)),
                        "VERTICAL_FIXTURE_PREMISE: reference glyph ink must cross the upper mask edge");
                    assert!((left..right).any(|x| foreground(&reference, x, bottom)),
                        "VERTICAL_FIXTURE_PREMISE: reference glyph ink must cross the lower mask edge");
                    let visible = (top..bottom)
                        .flat_map(|y| (left..right).map(move |x| (x, y)))
                        .filter(|(x, y)| foreground(&clipped, *x, *y))
                        .count();
                    assert!(
                        visible > 8,
                        "VERTICAL_VISIBLE_GLYPHS: the clipped real glyphs must remain visible"
                    );
                    if complete_guard {
                        // Three pixels of real vertical padding keep the guard clear of the later field border.
                        for y in top - 2..bottom + 2 {
                            for x in left - 2..right + 2 {
                                if x < left || x >= right || y < top || y >= bottom {
                                    assert!(!foreground(&clipped, x, y),
                                        "glyph paint escaped the real content mask: VERTICAL_GLYPH_ESCAPE at ({x}, {y}): move={moving} base={base} width={width} blurred={blurred}");
                                }
                            }
                        }
                    } else {
                        // This is the original two-column assertion, applied to the identical genuine field fixture.
                        for x in [left.saturating_sub(2), right + 1] {
                            assert!(
                                (top..bottom).all(|y| !foreground(&clipped, x, y)),
                                "glyph paint escaped the real content mask"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn original_two_columns_on_genuine_vertical_fixture() {
    check(false);
}

#[test]
fn candidate_full_guard_on_genuine_vertical_fixture() {
    check(true);
}
