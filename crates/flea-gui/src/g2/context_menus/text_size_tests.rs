//! Trash rail menus use the same text-size geometry as the surrounding view.
use super::*;
use gpui::AppContext;

#[gpui::test]
fn ey380_trash_rail_menu_rows_and_width_follow_the_text_size(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (handle, shared) = window(cx);
    for (body, row_pitch, width) in [(14, 47., 257.), (20, 67., 367.)] {
        let mut fixture = Fixture::new();
        fixture.typeface = fixture.typeface.at_base(body);
        cx.update_window(handle.into(), |_, window, _| {
            let mut menu = shared.borrow_mut();
            menu.open(
                Face::Rail,
                flea::ui_logic_context_menus::build::trash_entries(1, false),
                (20., 140.),
                WIDE,
                "",
                &fixture.env(),
                window,
            );
            let card = menu.card.as_ref().expect("Trash rail menu opened");
            assert_eq!(card.built.typeface.body, px(body as f32));
            let rows: Vec<_> = card
                .entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| (!entry.separator).then_some(index))
                .collect();
            assert_eq!(
                card.row_screen_top(rows[1]) - card.row_screen_top(rows[0]),
                row_pitch,
                "Trash menu row and separator pitch at {body}"
            );
            assert_eq!(card.frame.w, width, "Trash menu width at {body}");
            let centre = if body == 14 { 71.5 } else { 102.5 };
            assert_eq!(
                card.row_at(card.frame.x + width / 2., card.frame.y + centre),
                Some(rows[1]),
                "native-sized Restore All centre hits its own row at {body}"
            );
        })
        .expect("test window exists");
        crate::g2::context_menus::element::MARKED.with(|marks| marks.borrow_mut().clear());
        draw(cx, handle);
        crate::g2::context_menus::element::MARKED.with(|marks| {
            let marks = marks.borrow();
            assert!(!marks.is_empty(), "the menu painted its actual SVG marks");
            let side = if body == 14 { 18. } else { 26. };
            for bounds in marks.iter() {
                assert_eq!(
                    (f32::from(bounds.size.width), f32::from(bounds.size.height)),
                    (side, side)
                );
            }
        });
    }
}
