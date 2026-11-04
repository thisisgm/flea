use super::{host::OutputView, model::Action, tests::selected};
use flea::ui_logic_op4::output::Kind;
use gpui::TestAppContext;
use std::sync::Arc;

#[gpui::test]
fn op4_real_entity_draws_convert_anatomy_and_native_keys(cx: &mut TestAppContext) {
    let (view, vcx) = cx.add_window_view(|window, cx| {
        let theme = crate::palette::Theme::default();
        let p = crate::list::Palette::inherited(&window.text_style(), theme);
        let palette = crate::g2::op1::palette::CardPalette::from_list(&p, 8.);
        OutputView::new(Arc::new(crate::test_catalogs::english()), palette, cx)
    });
    vcx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    view.update_in(vcx, |view, window, cx| {
        assert!(view.open(
            Action::Convert,
            selected(Kind::Image, &["photo.png"]),
            window,
            cx
        ));
    });
    vcx.run_until_parked();
    let frame = vcx.debug_bounds("op4-card").unwrap();
    let title = vcx.debug_bounds("op4-title").unwrap();
    let rule = vcx.debug_bounds("op4-rule").unwrap();
    assert_eq!(f32::from(frame.size.width), 394.);
    let body = vcx.debug_bounds("op4-body").unwrap();
    let buttons = vcx.debug_bounds("op4-buttons").unwrap();
    eprintln!(
        "OP4_GEOMETRY card={} body={} title={} rule={} buttons={}",
        f32::from(frame.size.height),
        f32::from(body.size.height),
        f32::from(title.size.height),
        f32::from(rule.size.height),
        f32::from(buttons.size.height)
    );
    assert!(
        (f32::from(frame.size.height) - 529.2).abs() < 1.,
        "card height {}",
        f32::from(frame.size.height)
    );
    assert_eq!(f32::from(title.origin.x - frame.origin.x), 17.);
    assert_eq!(f32::from(rule.size.width), 392.);
    assert!((f32::from(frame.origin.y + frame.size.height / 2.) - 350.).abs() < 1.);
    vcx.simulate_keystrokes("tab space");
    assert!(view.read_with(
        vcx,
        |v, _| matches!(&v.model.card, Some(super::model::Card::Convert(c)) if c.strip)
    ));
    vcx.simulate_keystrokes("escape");
    assert!(view.read_with(vcx, |v, _| v.model.card.is_none()));
}

#[gpui::test]
fn op4_print_and_combine_have_board_width_and_preview_height(cx: &mut TestAppContext) {
    for action in [Action::Print, Action::Combine] {
        let (view, vcx) = cx.add_window_view(|window, cx| {
            let p = crate::list::Palette::inherited(
                &window.text_style(),
                crate::palette::Theme::default(),
            );
            OutputView::new(
                Arc::new(crate::test_catalogs::english()),
                crate::g2::op1::palette::CardPalette::from_list(&p, 8.),
                cx,
            )
        });
        vcx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
        view.update_in(vcx, |view, window, cx| {
            view.open(
                action,
                selected(Kind::Image, &["a.png", "b.jpg"]),
                window,
                cx,
            );
        });
        vcx.run_until_parked();
        assert_eq!(
            f32::from(vcx.debug_bounds("op4-card").unwrap().size.width),
            552.
        );
        if action == Action::Combine {
            assert_eq!(
                f32::from(vcx.debug_bounds("op4-preview").unwrap().size.height),
                280.
            );
            assert_eq!(
                vcx.debug_bounds("op4-columns").unwrap().origin.x,
                vcx.debug_bounds("op4-background").unwrap().origin.x
            );
        }
    }
}

#[gpui::test]
fn op4_held_images_land_atomically_and_pointer_keys_reorder_the_real_preview(
    cx: &mut TestAppContext,
) {
    let (view, vcx) = cx.add_window_view(|window, cx| {
        let p =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        OutputView::new(
            Arc::new(crate::test_catalogs::english()),
            crate::g2::op1::palette::CardPalette::from_list(&p, 8.),
            cx,
        )
    });
    vcx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    view.update_in(vcx, |view, window, cx| {
        view.open(
            Action::Combine,
            selected(Kind::Image, &["a.png", "b.jpg"]),
            window,
            cx,
        );
        let image = Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::from_raw(1, 1, vec![255; 4]).unwrap(),
        )]));
        let thumbs = ["a.png", "b.jpg"]
            .iter()
            .map(|name| super::seam::Thumbnail {
                path: format!("/fixture/{name}"),
                size: (100, 100),
                image: image.clone(),
            })
            .collect::<Vec<_>>();
        assert!(!view.set_thumbnails(10, thumbs.clone(), cx));
        assert!(view.set_thumbnails(11, thumbs.clone(), cx));
        assert!(!view.set_thumbnails(11, thumbs[..1].to_vec(), cx));
        assert_eq!(view.thumbnails.len(), 2);
    });
    vcx.run_until_parked();
    let bounds = view.read_with(vcx, |v, _| v.preview_bounds.get().unwrap());
    let left = bounds.origin + gpui::point(bounds.size.width / 4., bounds.size.height / 2.);
    let right = bounds.origin + gpui::point(bounds.size.width * 3. / 4., bounds.size.height / 2.);
    vcx.simulate_mouse_down(left, gpui::MouseButton::Left, gpui::Modifiers::default());
    vcx.simulate_mouse_move(
        right,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    vcx.simulate_mouse_up(right, gpui::MouseButton::Left, gpui::Modifiers::default());
    vcx.run_until_parked();
    assert!(
        view.read_with(vcx, |v, _| v.model.selection.as_ref().unwrap().files[0]
            .path
            .ends_with("b.jpg"))
    );
    vcx.simulate_keystrokes("[");
    assert!(
        view.read_with(vcx, |v, _| v.model.selection.as_ref().unwrap().files[0]
            .path
            .ends_with("a.png"))
    );
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert!(view.read_with(vcx, |v, _| v.thumbnails.is_empty()));
}
