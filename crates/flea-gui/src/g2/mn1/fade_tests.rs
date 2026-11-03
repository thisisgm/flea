//! Mounted menu pixels must lose glyph contrast under each edge fade, in both palette directions.
use super::MenuElement;
use crate::{
    g2::mn1::{
        lines::Env,
        menu::{Face, Flyout, Menu},
    },
    list::{bench::bench_typeface, Typeface},
    palette::Theme,
};
use flea::{
    i18n::{I18n, Paths},
    ui_logic_mn1::{
        fit::{self, Rect, PAD, ROW},
        Entry, Text,
    },
};
use gpui::{
    div, prelude::*, px, size, AppContext, Context, HeadlessAppContext, Render, TextRenderingMode,
    Window, WindowHandle,
};
use image::RgbaImage;
use std::{borrow::Cow, cell::RefCell, path::PathBuf, rc::Rc, sync::Arc};

struct Host(Rc<RefCell<Menu>>, Typeface, f32, bool);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .font_family(self.1.font.family.clone())
            .text_size(self.1.body)
            .child(if self.3 {
                MenuElement::fading(self.0.clone(), px(self.2))
            } else {
                MenuElement::new(self.0.clone(), px(self.2))
            })
    }
}

fn app() -> HeadlessAppContext {
    let mut app = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    app.update(|cx| {
        // The edge fades are judged on a card at rest, past its open fade.
        cx.set_reduce_motion(true);
        cx.set_text_rendering_mode(TextRenderingMode::Grayscale);
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(
                include_bytes!("../../../tests/fonts/JetBrainsMono-Regular.ttf").as_slice(),
            )])
            .unwrap();
    });
    app
}

fn rows() -> Vec<Entry> {
    (0..20)
        .map(|at| Entry {
            hint: Some(Text::Data("HHHH".into())),
            key_hint: Some("H"),
            ..Entry::row(format!("row{at}"), Text::Data("HHHH".into()), "checkmark")
        })
        .collect()
}

fn open(app: &mut HeadlessAppContext, radius: f32) -> (WindowHandle<Host>, Rc<RefCell<Menu>>) {
    let menu = Rc::new(RefCell::new(Menu::default()));
    let shared = menu.clone();
    let handle = app
        .open_window(size(px(600.), px(300.)), |window, cx| {
            window.set_scale_factor(1.);
            window.set_rem_size(px(14.));
            let typeface = bench_typeface();
            let catalog = I18n::load(Paths {
                omarchy: "/nonexistent".into(),
                language_file: "/nonexistent".into(),
                catalogs: vec![crate::test_catalogs::english_only()],
            });
            let env = Env {
                catalog: &catalog,
                preset: "default",
                key_hints: true,
                typeface: &typeface,
                inset: 4.,
            };
            shared.borrow_mut().open(
                Face::File,
                rows(),
                (20., 20.),
                Rect {
                    x: 0.,
                    y: 0.,
                    w: 600.,
                    h: 240.,
                },
                "fade",
                &env,
                window,
            );
            cx.new(|_| Host(shared, typeface, radius, false))
        })
        .unwrap();
    app.run_until_parked();
    (handle, menu)
}

fn capture(app: &mut HeadlessAppContext, handle: WindowHandle<Host>) -> RgbaImage {
    app.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    app.capture_screenshot(handle.into())
        .expect("actual WGPU menu pixels")
}

fn contrast(pixel: &image::Rgba<u8>, background: &image::Rgba<u8>) -> f32 {
    pixel.0[..3]
        .iter()
        .zip(&background.0[..3])
        .map(|(a, b)| f32::from(a.abs_diff(*b)))
        .sum()
}

// Compare the same repeated glyph pixels in a cut row and an unlit clear row.
fn edge_contrast(
    image: &RgbaImage,
    frame: Rect,
    scroll: i32,
    view: i32,
    top: bool,
    x: std::ops::Range<i32>,
) -> (usize, f32) {
    let reference = (scroll + ROW - 1) / ROW + 1;
    let edge = if top {
        scroll / ROW
    } else {
        (scroll + view - 1) / ROW
    };
    let origin = frame.y as i32 + PAD;
    let background = image.get_pixel(frame.x as u32 + 3, origin as u32 + 40);
    let mut count = 0;
    let mut clear = 0.;
    let mut faded = 0.;
    for dy in 0..ROW {
        let y = edge * ROW - scroll + dy;
        if y < 0 || y >= view {
            continue;
        }
        for dx in x.clone() {
            let x = (frame.x as i32 + dx) as u32;
            let ref_y = (origin + reference * ROW - scroll + dy) as u32;
            let original = contrast(image.get_pixel(x, ref_y), background);
            if original < 45. {
                continue;
            }
            count += 1;
            clear += original;
            faded += contrast(image.get_pixel(x, (origin + y) as u32), background);
        }
    }
    (count, faded / clear.max(1.))
}

#[test]
fn menu_edge_fade_attenuates_actual_glyphs_and_marks() {
    let mut app = app();
    let artifact = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../.superpowers/menu-edge-fade/run-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&artifact).unwrap();
    println!("FADE_ARTIFACT_DIR {}", artifact.display());
    let mut failures = Vec::new();
    for (theme, text) in [
        (
            "dark",
            "background='#1c120d'\nforeground='#c6b395'\ndark_background='#150d0a'",
        ),
        (
            "light",
            "background='#eff1f5'\nforeground='#4c4f69'\ndark_background='#e6e9ef'",
        ),
    ] {
        app.update(|cx| cx.set_global(Theme::from_text(text)));
        for radius in [0., 8.] {
            let (handle, menu) = open(&mut app, radius);
            for (state, scroll, view, edges) in [
                ("bottom", 0, 4 * ROW + ROW / 2, (false, true)),
                ("both", ROW / 2, 4 * ROW, (true, true)),
                (
                    "top",
                    20 * ROW - (4 * ROW + ROW / 2),
                    4 * ROW + ROW / 2,
                    (true, false),
                ),
            ] {
                let frame = {
                    let mut menu = menu.borrow_mut();
                    let card = menu.card.as_mut().unwrap();
                    card.content_y = scroll;
                    card.view_h = view;
                    card.frame.h = (view + 2 * PAD) as f32;
                    card.cursor = Some((scroll / ROW + 3) as usize);
                    assert_eq!(fit::fades(&card.entries, scroll, view), edges);
                    card.frame
                };
                let image = capture(&mut app, handle);
                let name = format!("{theme}-r{radius}-{state}");
                image.save(artifact.join(format!("{name}.png"))).unwrap();
                for (top, required) in [(true, edges.0), (false, edges.1)] {
                    if !required {
                        continue;
                    }
                    if radius == 8. {
                        let y = if top {
                            frame.y + 1.
                        } else {
                            frame.y + frame.h - 2.
                        } as u32;
                        for x in [frame.x + 1., frame.x + frame.w - 2.] {
                            if image.get_pixel(x as u32, y) != image.get_pixel(0, 0) {
                                failures.push(format!(
                                    "{name} top={top} frame corner painted at {x},{y}"
                                ));
                            }
                        }
                    }
                    for (part, x) in [("label", 43..77), ("mark", 15..33)] {
                        let (count, ratio) = edge_contrast(&image, frame, scroll, view, top, x);
                        println!("FADE_METRIC {name} top={top} part={part} pixels={count} contrast_ratio={ratio:.4}");
                        if count < 4 || !(0.02..0.85).contains(&ratio) {
                            failures.push(format!(
                                "{name} top={top} {part}: {count} pixels, ratio {ratio:.4}"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "edge fade left full-strength ink: {failures:#?}"
    );
}

#[test]
fn menu_edge_fade_parent_cannot_paint_over_an_overlapping_flyout() {
    let mut app = app();
    app.update(|cx| cx.set_global(Theme::default()));
    let (handle, menu) = open(&mut app, 8.);
    let (_, flyout_menu) = open(&mut app, 8.);
    let mut flyout = flyout_menu.borrow_mut().card.take().unwrap();
    let parent = menu.borrow().card.as_ref().unwrap().frame;
    flyout.frame.x = parent.x + 100.;
    flyout.frame.y = parent.y + 10.;
    flyout.content_y = ROW / 2;
    flyout.view_h = 4 * ROW;
    flyout.frame.h = (flyout.view_h + 2 * PAD) as f32;
    flyout.cursor = Some(3);
    let frame = flyout.frame;
    // A standalone card at the same location is the opaque flyout's pixel oracle.
    let original = menu.borrow_mut().card.replace(flyout).unwrap();
    let expected = capture(&mut app, handle);
    let flyout = menu.borrow_mut().card.replace(original).unwrap();
    menu.borrow_mut().flyout = Some(Flyout {
        parent: Some(0),
        action: "overlap".into(),
        card: flyout,
    });
    let actual = capture(&mut app, handle);
    // Stay away from rounded corner antialiasing, where the ground legitimately differs.
    for y in frame.y as u32 + 8..(frame.y + frame.h) as u32 - 8 {
        for x in frame.x as u32 + 1..(frame.x + frame.w) as u32 - 1 {
            assert_eq!(
                actual.get_pixel(x, y),
                expected.get_pixel(x, y),
                "parent rows or fades leaked into the flyout at {x},{y}"
            );
        }
    }
    menu.borrow_mut().flyout = None;
    let retired = capture(&mut app, handle);
    let (_, clean) = open(&mut app, 8.);
    menu.borrow_mut().card = clean.borrow_mut().card.take();
    let restored = capture(&mut app, handle);
    assert_eq!(
        retired, restored,
        "retiring the flyout leaves the parent intact"
    );
}

#[test]
fn menu_edge_fade_closing_card_keeps_its_own_ink_and_draws_once() {
    use crate::g2::mn1::menu::fade::Faded;
    use std::cell::Cell;

    let mut app = app();
    app.update(|cx| cx.set_global(Theme::default()));
    let (handle, menu) = open(&mut app, 8.);
    let (_, closing_menu) = open(&mut app, 8.);
    let mut closing = closing_menu.borrow_mut().card.take().unwrap();
    closing.frame.x += 100.;
    closing.frame.y += 10.;
    closing.content_y = ROW / 2;
    closing.view_h = 4 * ROW;
    closing.frame.h = (closing.view_h + 2 * PAD) as f32;
    closing.cursor = Some(3);
    let frame = closing.frame;
    let original = menu.borrow_mut().card.replace(closing).unwrap();
    let expected = capture(&mut app, handle);
    let closing = menu.borrow_mut().card.replace(original).unwrap();
    let (_, active_menu) = open(&mut app, 8.);
    menu.borrow_mut().flyout = Some(Flyout {
        parent: Some(0),
        action: "active".into(),
        card: active_menu.borrow_mut().card.take().unwrap(),
    });
    menu.borrow_mut().faded = Some(Faded {
        flyout: Flyout {
            parent: Some(0),
            action: "closing".into(),
            card: closing,
        },
        id: 1,
        started: None,
        from: 1.,
        painted: Rc::new(Cell::new(1.)),
    });
    handle.update(&mut app, |host, _, _| host.3 = true).unwrap();
    super::PAINTED.with(|painted| painted.borrow_mut().clear());
    let actual = capture(&mut app, handle);
    super::PAINTED.with(|painted| {
        let frames = painted.borrow();
        assert_eq!(
            frames.len(),
            1,
            "closing element must draw only its retained card"
        );
        assert_eq!(frames[0].origin, gpui::point(px(frame.x), px(frame.y)));
    });
    assert_eq!(
        actual, expected,
        "closing card must retain its own row ink and edge fades"
    );
}
