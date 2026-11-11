// The overlay as drawn: each board's sizes measured from real Elements, and the pointer on real buttons.
use super::host::Overlay;
use super::palette::CardPalette;
use super::tests::request;
use flea::ui_logic_file_ops::{collide, confirm, trash};
use gpui::TestAppContext;
use std::sync::Arc;

#[path = "jobs_draw_tests.rs"]
mod jobs;

fn measured(
    cx: &mut gpui::VisualTestContext,
    selector: &'static str,
) -> gpui::Bounds<gpui::Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"))
}

fn open_card(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
    open: impl FnOnce(&mut Overlay),
) -> (gpui::Entity<Overlay>, &mut gpui::VisualTestContext) {
    let (overlay, vcx) = cx.add_window_view(|window, cx| {
        window.set_rem_size(gpui::px(14.));
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = CardPalette::from_theme(&window.text_style(), theme, 8.);
        Overlay::new(
            Arc::new(crate::test_catalogs::english()),
            palette,
            "default",
        )
    });
    vcx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
    overlay.update(vcx, |o, cx| {
        open(o);
        cx.notify();
    });
    vcx.run_until_parked();
    if overlay.read_with(vcx, |o, _| o.model.jobs.unfinished() > 0) {
        super::draw_fixture::settle_progress(vcx);
    }
    (overlay, vcx)
}

// The shell's rhythm as drawn: widths, the 16 padding, 12 between parts, 22 above 30-tall right-aligned buttons.
#[gpui::test]
fn every_card_draws_the_shared_rhythm(cx: &mut TestAppContext) {
    use flea::ui_logic_file_ops::card;
    let (_, vcx) = open_card(cx, 1100., 700., |o| {
        o.model.confirm_delete(
            trash::Review {
                count: Some(1),
                token: "1".into(),
                ..Default::default()
            },
            trash::Scope::Items,
            confirm::Source::Menu(3),
            0,
        )
    });
    let f = |b: gpui::Pixels| f32::from(b);
    let scrim = measured(vcx, "op1-scrim");
    let shell = measured(vcx, "op1-card");
    let title = measured(vcx, "op1-title");
    let rule = measured(vcx, "op1-rule");
    let body = measured(vcx, "op1-body");
    let buttons = measured(vcx, "op1-buttons");
    let cancel = measured(vcx, "op1-button-0");
    let delete = measured(vcx, "op1-button-1");
    // oracle tests/dialog-board.qml:376
    assert_eq!((f(scrim.size.width), f(scrim.size.height)), (1100., 700.));
    assert_eq!(f(shell.size.width), card::width(card::Kind::Delete));
    // The card sits centred on the scrim.
    assert!((f(shell.origin.x) - (1100. - 416.) / 2.).abs() < 1.);
    assert!((f(shell.origin.y) + f(shell.size.height) / 2. - 350.).abs() < 1.);
    // oracle tests/dialog-board.qml:389
    assert_eq!(
        f(title.origin.y - shell.origin.y),
        card::BORDER + card::PADDING
    );
    assert_eq!(
        f(title.origin.x - shell.origin.x),
        card::BORDER + card::PADDING
    );
    // The hairline sits inside the padding, as delete.html draws it, one pixel tall.
    assert_eq!(
        f(rule.origin.y - (title.origin.y + title.size.height)),
        card::GAP
    );
    assert_eq!(
        f(rule.size.width),
        card::width(card::Kind::Delete) - 2. * (card::BORDER + card::PADDING)
    );
    assert_eq!(
        f(rule.origin.x - shell.origin.x),
        card::BORDER + card::PADDING
    );
    assert_eq!(f(rule.size.height), card::RULE);
    // At the test display's scale 2, the measured title encloses the board's 19.2 px line in 19.5 px.
    assert_eq!(f(title.size.height), 19.5);
    assert_eq!(
        f(body.origin.y - (rule.origin.y + rule.size.height)),
        card::GAP
    );
    // 22 of air above the button row, the row flush with the right padding.
    assert_eq!(
        f(buttons.origin.y - (body.origin.y + body.size.height)),
        card::GAP + card::BUTTONS_MARGIN
    );
    assert_eq!(f(cancel.size.height), card::BUTTON_HEIGHT);
    assert_eq!(
        f(delete.origin.x - (cancel.origin.x + cancel.size.width)),
        card::BUTTON_GAP
    );
    assert_eq!(
        f(shell.origin.x + shell.size.width - (delete.origin.x + delete.size.width)),
        card::BORDER + card::PADDING
    );
    assert_eq!(
        f(shell.origin.y + shell.size.height - (buttons.origin.y + buttons.size.height)),
        card::BORDER + card::PADDING
    );
}

#[gpui::test]
fn the_three_widths_and_a_narrow_window(cx: &mut TestAppContext) {
    use flea::ui_logic_file_ops::card;
    for (kind, w) in [(0, 448.), (1, 552.)] {
        let (_, vcx) = open_card(cx, 1100., 700., |o| {
            let cat = o.catalog.clone();
            if kind == 0 {
                o.model.ask(&cat, &request(), &[], false);
                o.model.line(&cat, r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":""}],"mergeable":false}"#, 0);
            } else {
                o.model.copy_to(3, false, "/home/u");
            }
        });
        assert_eq!(f32::from(measured(vcx, "op1-card").size.width), w);
    }
    let (_, vcx) = open_card(cx, 400., 700., |o| o.model.copy_to(3, true, "/x"));
    assert_eq!(
        f32::from(measured(vcx, "op1-card").size.width),
        400. - 2. * card::PADDING
    );
}

// A short window keeps the title and the buttons inside the card and scrolls only the body.
#[gpui::test]
fn a_short_window_pins_title_and_buttons(cx: &mut TestAppContext) {
    let (_, vcx) = open_card(cx, 1100., 220., |o| {
        let cat = o.catalog.clone();
        o.model.ask(&cat, &request(), &[], false);
        let names: Vec<String> = (0..30)
            .map(|i| format!("{{\"n\":\"file{i}.txt\",\"d\":false,\"i\":\"\"}}"))
            .collect();
        o.model.line(
            &cat,
            &format!(
                r#"{{"t":"collisions","id":1,"total":40,"names":[{}],"mergeable":true}}"#,
                names.join(",")
            ),
            0,
        );
    });
    let shell = measured(vcx, "op1-card");
    let title = measured(vcx, "op1-title");
    let buttons = measured(vcx, "op1-buttons");
    let body = measured(vcx, "op1-body");
    // oracle tests/dialog-board.qml:392
    assert!(f32::from(body.size.height) < 30. * 20.);
    // oracle tests/dialog-board.qml:404
    assert!(
        title.origin.y >= shell.origin.y
            && buttons.origin.y + buttons.size.height <= shell.origin.y + shell.size.height
    );
    // oracle tests/dialog-board.qml:405
    assert!(
        f32::from(shell.origin.y) >= 0. && f32::from(shell.origin.y + shell.size.height) <= 220.
    );
}

// collide.html: a 19 px mark beside the name, 12 between parts, the explanation at 12 on an 18 px line.
#[gpui::test]
fn the_collision_card_draws_its_board_parts(cx: &mut TestAppContext) {
    use flea::ui_logic_file_ops::card;
    let (_, vcx) = open_card(cx, 1100., 700., |o| {
        let cat = o.catalog.clone();
        o.model.ask(&cat, &request(), &[], false);
        o.model.line(&cat, r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":"text-x-generic"}],"mergeable":false}"#, 0);
    });
    let name = measured(vcx, "op1-name-0");
    let explain = measured(vcx, "op1-explain");
    let rule = measured(vcx, "op1-rule");
    assert_eq!(f32::from(name.size.height), card::NAME_MARK);
    assert_eq!(
        f32::from(name.origin.y - (rule.origin.y + rule.size.height)),
        card::GAP
    );
    assert_eq!(
        f32::from(explain.origin.y - (name.origin.y + name.size.height)),
        card::GAP
    );
    assert_eq!(f32::from(explain.size.height), card::EXPLAIN_LINE);
    // Four buttons without Merge, five with it, every one 30 tall.
    for at in 0..4 {
        assert_eq!(
            f32::from(
                measured(vcx, Box::leak(format!("op1-button-{at}").into_boxed_str()))
                    .size
                    .height
            ),
            card::BUTTON_HEIGHT
        );
    }
    // Negative: no fifth button on a card that cannot merge.
    assert!(vcx.debug_bounds("op1-button-4").is_none());
}

// copyto.html: the field name on a 14 px line, 5 above a 30 tall field the card's width less its padding.
#[gpui::test]
fn the_copy_to_field_draws_at_its_board_size(cx: &mut TestAppContext) {
    use flea::ui_logic_file_ops::card;
    let (_, vcx) = open_card(cx, 1100., 700., |o| o.model.copy_to(3, false, "/fx/v/work"));
    let label = measured(vcx, "op1-field-label");
    let field = measured(vcx, "op1-field");
    assert_eq!(f32::from(label.size.height), card::FIELD_LABEL_LINE);
    assert_eq!(
        f32::from(field.origin.y - (label.origin.y + label.size.height)),
        card::FIELD_LABEL_GAP
    );
    assert_eq!(f32::from(field.size.height), card::BUTTON_HEIGHT);
    assert_eq!(
        f32::from(field.size.width),
        card::width(card::Kind::CopyTo) - 2. * (card::BORDER + card::PADDING)
    );
}

fn centre(b: gpui::Bounds<gpui::Pixels>) -> gpui::Point<gpui::Pixels> {
    gpui::point(
        b.origin.x + b.size.width / 2.,
        b.origin.y + b.size.height / 2.,
    )
}

// A button takes hover and a held press, and a release over it does what Enter does.
#[gpui::test]
fn a_pointer_hovers_presses_and_clicks_a_card_button(cx: &mut TestAppContext) {
    use super::press::Press;
    let (overlay, vcx) = open_card(cx, 1100., 700., |o| {
        let cat = o.catalog.clone();
        o.model.ask(&cat, &request(), &[], false);
        o.model.line(&cat, r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":""}],"mergeable":false}"#, 0);
        o.model.outbox.clear();
    });
    let keep = centre(measured(vcx, "op1-button-3"));
    vcx.simulate_mouse_move(keep, None, gpui::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(
        overlay.read_with(vcx, |o, _| o.pointer.hovered),
        Some(Press::Collide(collide::Choice::Keep))
    );
    vcx.simulate_mouse_down(keep, gpui::MouseButton::Left, gpui::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(
        overlay.read_with(vcx, |o, _| o.pointer.pressed),
        Some(Press::Collide(collide::Choice::Keep))
    );
    // Negative: a held press sends nothing until it is released over the button.
    assert!(overlay.read_with(vcx, |o, _| o.model.outbox.is_empty()));
    vcx.simulate_mouse_up(keep, gpui::MouseButton::Left, gpui::Modifiers::default());
    vcx.run_until_parked();
    overlay.read_with(vcx, |o, _| {
        assert!(o.model.dialog.is_none());
        assert_eq!(o.model.outbox.len(), 1);
        assert!(
            o.model.outbox[0].contains(r#""collide":"keep""#),
            "{}",
            o.model.outbox[0]
        );
        assert_eq!(o.pointer.pressed, None);
    });
}

// A release away from the button it pressed does nothing, and an unavailable Delete ignores its press.
#[gpui::test]
fn a_press_released_elsewhere_or_on_an_unavailable_button_does_nothing(cx: &mut TestAppContext) {
    let (overlay, vcx) = open_card(cx, 1100., 700., |o| {
        o.model.confirm_delete(
            trash::Review::default(),
            trash::Scope::Items,
            confirm::Source::Menu(3),
            0,
        )
    });
    let delete = measured(vcx, "op1-button-1");
    let title = measured(vcx, "op1-title");
    vcx.simulate_mouse_down(
        centre(delete),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    vcx.simulate_mouse_up(
        centre(delete),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    vcx.run_until_parked();
    overlay.read_with(vcx, |o, _| {
        assert!(o.model.dialog.is_some());
        assert!(o.model.outbox.is_empty());
    });
    let cancel = measured(vcx, "op1-button-0");
    vcx.simulate_mouse_down(
        centre(cancel),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    vcx.simulate_mouse_move(
        centre(title),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    vcx.simulate_mouse_up(
        centre(title),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    vcx.run_until_parked();
    overlay.read_with(vcx, |o, _| {
        assert!(o.model.dialog.is_some());
        assert_eq!(o.pointer.pressed, None);
    });
}

// EY-400: at text size 20 the collision card's file mark is Theme.markSize, bodySmall 18 times 1.45, as v0.3.12 draws.
#[gpui::test]
fn ey400_the_collision_mark_follows_the_text_size(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(crate::list::Typeface::omarchy_default().at_base(20)));
    let (_, vcx) = open_card(cx, 1100., 700., |o| {
        let cat = o.catalog.clone();
        o.model.ask(&cat, &request(), &[], false);
        o.model.line(&cat, r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes.txt","d":false,"i":"text-x-generic"}],"mergeable":false}"#, 0);
    });
    let name = measured(vcx, "op1-name-0");
    assert_eq!(
        f32::from(name.size.height),
        26.,
        "a 26 px mark sets the row"
    );
}
