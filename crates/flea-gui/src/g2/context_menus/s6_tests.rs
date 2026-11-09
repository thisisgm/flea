//! EY-576: the available-update hint retains v0.3.12's accent square and its own width.
use super::{lines, menu::Face, tests};
use flea::ui_logic_context_menus::build::{listing_entries, Context};
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn s6_update_menu_paints_the_accent_square_before_the_version(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (handle, menu) = tests::window(cx);
    tests::with(cx, handle, |menu, env, window| {
        let entries = listing_entries(&Context {
            has_row: false,
            hidden: Vec::new(),
            update_version: "0.4.1".into(),
            ..Context::default()
        });
        let entry = entries
            .iter()
            .find(|entry| entry.action == "updateFlea")
            .unwrap();
        assert!(entry.hint_square);
        let mut plain = entry.clone();
        plain.hint_square = false;
        let with_square = lines::build(std::slice::from_ref(entry), env, window, 0., 1100.);
        let without_square = lines::build(&[plain], env, window, 0., 1100.);
        assert_eq!(with_square.widest - without_square.widest, 13.);
        menu.open(
            Face::Background,
            entries,
            (20., 20.),
            tests::WIDE,
            "ground",
            env,
            window,
        );
    });
    let (bounds, row_top) = {
        let held = menu.borrow();
        let card = held.card.as_ref().unwrap();
        let at = card
            .entries
            .iter()
            .position(|entry| entry.action == "updateFlea")
            .unwrap();
        assert_eq!(card.built.lines[at].word.as_ref().unwrap().text, "0.4.1");
        let line = &card.built.lines[at];
        let top = card.frame.y
            + card.built.metrics.pad as f32
            + card.built.metrics.row_top(&card.entries, at) as f32;
        (card.frame, (top, line.word.as_ref().unwrap().width()))
    };
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scale = window.scale_factor();
        let squares: Vec<_> = window
            .painted_quads()
            .into_iter()
            .filter(|quad| {
                (quad.bounds.size.width.0 / scale - 6.).abs() < 0.01
                    && (quad.bounds.size.height.0 / scale - 6.).abs() < 0.01
            })
            .collect();
        assert_eq!(squares.len(), 1, "one update status square is painted");
        let square = &squares[0];
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        assert_eq!(square.background.as_solid().unwrap(), theme.accent);
        let expected_x = bounds.x + bounds.w - 1. - 14. - 12. - 9. - row_top.1 - 7. - 6.;
        assert!((square.bounds.origin.x.0 / scale - expected_x).abs() < 0.01);
        assert!((square.bounds.origin.y.0 / scale - row_top.0 - 15.5).abs() < 0.01);
    })
    .unwrap();
}
