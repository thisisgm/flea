// Review r5's findings as tests: one held delete per channel, and the busy menu card in the oracle's own look.
use super::host::Overlay;
use super::model::Operations;
use super::palette::CardPalette;
use super::tests::trash_source;
use flea::ui_logic_op1::{confirm::Source, trash};
use gpui::TestAppContext;
use std::sync::Arc;

fn review(token: &str) -> trash::Review {
    trash::Review {
        count: Some(2),
        token: token.into(),
        ..Default::default()
    }
}

// r5-1 sequence A: a Trash delete out, a menu card over it, Delete on the menu card, then any card.
// The menu delete moved aside must not drop the held Trash delete: the seam stays true and its start lands.
#[test]
fn a_menu_delete_moved_aside_keeps_a_held_trash_delete() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    assert!(ops.trash_request_out());
    ops.confirm_delete(review("9"), trash::Scope::Items, Source::Menu(9), 100);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    ops.copy_to(7, false, "/x");
    assert!(
        ops.trash_request_out(),
        "a held Trash delete must still hold the seam"
    );
    ops.line(
        &cat,
        r#"{"t":"trashdeletestarted","id":40,"requestId":1}"#,
        200,
    );
    assert_eq!((ops.emptying.id, ops.emptying.running), (40, true));
}

// r5-1 sequence B, the reverse: a menu delete out, a Trash card over it, Delete on the Trash card, then any card.
// The Trash delete moved aside must not drop the held menu delete: op1 still says its result.
#[test]
fn a_trash_delete_moved_aside_keeps_a_held_menu_delete() {
    let cat = crate::test_catalogs::english();
    let mut ops = Operations::new("default");
    ops.confirm_delete(review("9"), trash::Scope::Items, Source::Menu(6), 0);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    ops.confirm_delete(review("4"), trash::Scope::TrashItems, trash_source(), 100);
    ops.key(&cat, "tab", false);
    ops.key(&cat, "enter", false);
    ops.copy_to(7, false, "/x");
    assert!(
        ops.trash_request_out(),
        "a held Trash delete must still hold the seam"
    );
    ops.outbox.clear();
    ops.line(&cat, r#"{"t":"menuaction","id":6,"op":"delete","ok":true,"deleted":2,"failed":0,"cancelled":false,"error":""}"#, 300);
    assert_eq!(
        ops.said.last().map(|s| s.text.as_str()),
        Some("Deleted 2 of 2 items")
    );
}

// r5-2: while a menu refresh is out the card carries the oracle's own title, not the review's.
#[test]
fn the_busy_menu_card_uses_the_oracles_title() {
    let cat = crate::test_catalogs::english();
    assert_eq!(
        super::view::delete_title(&cat, &review("3"), true),
        "Delete permanently"
    );
    assert_eq!(
        super::view::delete_title(&cat, &review("3"), false),
        trash::confirm_title(&cat, &review("3"))
    );
}

// r5-2: while a menu refresh is out only Cancel is drawn, no second Delete button.
#[gpui::test]
fn the_busy_menu_card_draws_cancel_only(cx: &mut TestAppContext) {
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
    vcx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    overlay.update(vcx, |o, cx| {
        let cat = o.catalog.clone();
        o.model
            .confirm_delete(review("3"), trash::Scope::Items, Source::Menu(6), 0);
        o.model.tick(&cat, 2500);
        o.model.line(
            &cat,
            r#"{"t":"menuaction","id":6,"op":"checkDelete","ok":true,"valid":false}"#,
            2600,
        );
        o.model.outbox.clear();
        cx.notify();
    });
    vcx.run_until_parked();
    // A busy card is progress, so it draws once the shared 150 ms hold has run.
    assert!(vcx.debug_bounds("op1-button-0").is_none());
    vcx.executor()
        .advance_clock(std::time::Duration::from_millis(150));
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("op1-button-0").is_some());
    assert!(
        vcx.debug_bounds("op1-button-1").is_none(),
        "a busy menu card draws Cancel only"
    );
}
