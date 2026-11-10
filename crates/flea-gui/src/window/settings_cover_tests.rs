//! 0401-settings-wide-b: the card is 760px, narrows with the window to 630px at 646px, and a chrome or rail mark under it is whole or absent.
use super::{configured, read};
use crate::{backend::Backend, path::chrome::GLYPHS};
use gpui::{point, px, size, Bounds, Pixels, TestAppContext};

fn card(reply: String) -> Bounds<Pixels> {
    let n: Vec<f32> = reply
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    Bounds::new(point(px(n[0]), px(n[1])), size(px(n[2]), px(n[3])))
}

#[test]
fn f11_the_card_is_760_and_narrows_to_630_at_a_646_window() {
    use crate::g2::settings_pages::geometry::{card_size, Scale, CARD_FLOOR};
    let s = Scale::default();
    for (window, width) in [
        (2560., 760.),
        (1100., 760.),
        (900., 760.),
        (776., 760.),
        (700., 684.),
        (646., CARD_FLOOR),
    ] {
        assert_eq!(
            card_size(window, 700., 0., s).0,
            width,
            "at a {window}px window"
        );
    }
    for window in [646., 630., 600., 400., 320.] {
        assert_eq!(
            card_size(window, 700., 0., s).0,
            CARD_FLOOR,
            "at a {window}px window"
        );
    }
}

#[gpui::test]
fn f11_chrome_and_rail_marks_under_the_card_are_not_drawn(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = configured(backend, cx);
        shell.mount_rail(cx);
        window.focus(&shell.focus, cx);
        shell
    });
    let marks = |cx: &mut gpui::VisualTestContext| {
        shell.update_in(cx, |shell, _, cx| {
            let chrome = shell.chrome.view.as_ref().expect("the chrome").read(cx);
            let frame = chrome.frame.borrow();
            (0..GLYPHS.len())
                .filter(|&at| frame.controls[at].visible)
                .map(|at| {
                    (
                        GLYPHS[at],
                        frame.controls[at].bounds.unwrap(),
                        frame.controls[at].mark_bounds,
                    )
                })
                .collect::<Vec<_>>()
        })
    };
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        marks(cx).iter().all(|(_, _, mark)| mark.is_some()),
        "every mark is drawn before Settings opens"
    );
    cx.simulate_keystrokes(",");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let card = shell.update_in(cx, |shell, window, cx| {
        card(read(shell, "settingsCardRect", window, cx).stdout)
    });
    assert_eq!(
        card,
        Bounds::new(point(px(70.), px(8.)), size(px(760.), px(684.)))
    );
    let shown = marks(cx);
    for (glyph, slot, mark) in &shown {
        match mark {
            Some(mark) => assert!(
                !mark.intersects(&card),
                "{glyph} is drawn cut by the card at {mark:?}"
            ),
            None => assert!(
                slot.intersects(&card),
                "{glyph} is hidden though the card misses it"
            ),
        }
    }
    let hidden: Vec<_> = shown
        .iter()
        .filter(|(_, _, mark)| mark.is_none())
        .map(|(glyph, _, _)| *glyph)
        .collect();
    assert!(
        hidden.contains(&"search") && hidden.contains(&"list"),
        "{hidden:?}"
    );
    assert!(shown
        .iter()
        .any(|(glyph, _, mark)| *glyph == "sliders" && mark.is_some()));
    // A shell update can reuse the Settings view, so its cover must survive an unrelated render.
    shell.update(cx, |_, cx| cx.notify());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(marks(cx), shown);
    // The rail reads the same card, so its marks under it go too.
    shell.update_in(cx, |shell, _, cx| {
        let rail = shell
            .rail
            .as_ref()
            .expect("the rail")
            .places
            .read(cx)
            .model
            .borrow();
        assert!(rail
            .cover
            .hides(Bounds::new(card.center(), size(px(1.), px(1.)))));
        assert!(!rail
            .cover
            .hides(Bounds::new(point(px(0.), px(0.)), size(px(8.), px(8.)))));
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        marks(cx).iter().all(|(_, _, mark)| mark.is_some()),
        "every mark is drawn again once Settings closes"
    );
    shell.update_in(cx, |shell, _, cx| {
        let rail = shell
            .rail
            .as_ref()
            .expect("the rail")
            .places
            .read(cx)
            .model
            .borrow();
        assert!(!rail
            .cover
            .hides(Bounds::new(card.center(), size(px(1.), px(1.)))));
    });
}

/// What one frame painted: the tag row's square, the share row's state dot, and the chrome's dividers.
fn painted(
    shell: &gpui::Entity<crate::state::Shell>,
    cx: &mut gpui::VisualTestContext,
) -> (bool, bool, usize) {
    use crate::g2::{
        rail_places::rows::{DOT, TAG},
        settings_pages::geometry::CLAMP,
    };
    cx.update(|window, cx| window.draw(cx).clear(cx));
    shell.update_in(cx, |shell, window, cx| {
        let places = shell.rail.as_ref().expect("the rail").places.read(cx);
        let laid = places.laid.borrow();
        let scale = window.scale_factor();
        // Each quad as its logical x, y, width and height.
        let quads: Vec<[f32; 4]> = window
            .painted_quads()
            .iter()
            .map(|quad| {
                let (at, side) = (quad.bounds.origin, quad.bounds.size);
                [at.x, at.y, side.width, side.height].map(|v| v.as_f32() / scale)
            })
            .collect();
        let square = |side: f32, x: f32, y: f32, width: f32, height: f32| {
            quads.iter().any(|q| {
                q[2] == side
                    && q[3] == side
                    && q[0] >= x
                    && q[0] + side <= x + width
                    && q[1] >= y
                    && q[1] + side <= y + height
            })
        };
        let row = |label: &str| {
            laid.rows
                .iter()
                .find(|row| row.label == label)
                .unwrap_or_else(|| panic!("the {label} row is laid"))
        };
        let (tag, slot) = (row("Red").rect, row("NAS share").indicator_rect);
        (
            square(TAG, tag.x, tag.y, tag.width, tag.height),
            square(DOT, slot.x, slot.y, slot.width, slot.height),
            // A divider is one hairline wide and starts above the card's top edge, the card's inset from the window.
            quads
                .iter()
                .filter(|q| q[2] == 1. && q[3] > q[2] && q[1] < CLAMP)
                .count(),
        )
    })
}

#[gpui::test]
fn f11_rail_squares_and_chrome_dividers_under_the_card_are_not_painted(cx: &mut TestAppContext) {
    use flea::ui_logic_rail_places::{
        order::{Heading, RailSection},
        rail::{Group, Kind, RailEntry},
    };
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let mut shell = configured(backend, cx);
        shell.mount_rail(cx);
        window.focus(&shell.focus, cx);
        shell
    });
    cx.run_until_parked();
    let rows = vec![
        RailEntry {
            group: Group::Tag,
            kind: Kind::Tag,
            label: "Red".into(),
            tag: Some("Red"),
            ..RailEntry::default()
        },
        RailEntry {
            group: Group::Network,
            kind: Kind::Share,
            label: "NAS share".into(),
            uri: "smb://nas/share".into(),
            glyph: "server",
            ..RailEntry::default()
        },
    ];
    shell.update(cx, |shell, cx| {
        let places = shell.rail.as_ref().expect("the rail").places.clone();
        places.update(cx, |producer, cx| {
            let heading = Heading::Key("places.heading.places");
            producer.replace_sections(vec![RailSection { heading, rows }], cx)
        });
    });
    cx.simulate_resize(size(px(900.), px(700.)));
    let closed = painted(&shell, cx);
    cx.simulate_keystrokes(",");
    let at_900 = painted(&shell, cx);
    cx.simulate_resize(size(px(700.), px(700.)));
    let at_700 = painted(&shell, cx);
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.simulate_keystrokes("escape");
    let closed_again = painted(&shell, cx);
    assert_eq!(
        [closed, at_900, at_700, closed_again],
        [
            (true, true, 2),
            // At 900px the card starts at x 70: it covers the trailing dot and the first divider, and misses the tag square.
            (true, false, 1),
            // At 700px the card starts at x 8, so it covers both rail marks and every divider.
            (false, false, 0),
            (true, true, 2),
        ],
        "closed, open at 900px, open at 700px, closed again"
    );
}
