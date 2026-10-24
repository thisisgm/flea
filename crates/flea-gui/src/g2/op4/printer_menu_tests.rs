use super::{
    controls::Control,
    host::OutputView,
    model::{Action, Card, Focus},
    tests::selected,
};
use flea::{
    i18n::I18n,
    ui_logic_op4::{output::Kind, printing::Destination},
};
use gpui::TestAppContext;
use std::sync::Arc;

#[gpui::test]
fn op4_printer_popup_uses_shared_menu_without_resizing_or_dismissing_the_card(
    cx: &mut TestAppContext,
) {
    let (view, vcx) = cx.add_window_view(|window, cx| {
        let p =
            crate::list::Palette::inherited(&window.text_style(), crate::palette::Theme::default());
        OutputView::new(
            Arc::new(I18n::default()),
            crate::g2::op1::palette::CardPalette::from_list(&p, 8.),
            cx,
        )
    });
    vcx.simulate_resize(gpui::size(gpui::px(1100.), gpui::px(700.)));
    view.update_in(vcx, |view, window, cx| {
        view.open(
            Action::Print,
            selected(Kind::Pdf, &["report.pdf"]),
            window,
            cx,
        );
        let Some(Card::Print(p)) = &mut view.model.card else {
            panic!()
        };
        p.queried = true;
        p.pending = None;
        p.printer = "Printer0".into();
        p.printer_name = p.printer.clone();
        p.destinations = (0..256)
            .map(|i| Destination {
                name: format!("Printer{i}"),
                default: i == 0,
            })
            .collect();
        cx.notify();
    });
    vcx.run_until_parked();
    let before = vcx.debug_bounds("op4-card").unwrap();
    let printer = vcx.debug_bounds("op4-printer").unwrap();
    let at = printer.center();
    vcx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    vcx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    vcx.run_until_parked();
    assert_eq!(vcx.debug_bounds("op4-card").unwrap(), before);
    view.read_with(vcx, |view, _| {
        assert!(view.printer_menu.opened());
        let menu = view.printer_menu.menu.borrow();
        let card = menu.card.as_ref().unwrap();
        assert_eq!(card.entries.len(), 256);
        assert!(card.frame.h < 700.);
        assert_eq!(card.entries[0].check, Some(true));
        assert_eq!(card.built.lines[0].word.as_ref().unwrap().text, "default");
    });
    vcx.simulate_keystrokes("escape");
    assert!(view.read_with(vcx, |view, _| view.model.card.is_some()
        && !view.printer_menu.opened()));
    view.update_in(vcx, |view, window, cx| {
        view.activate(Control::Field(Focus::Printer), window, cx)
    });
    vcx.simulate_keystrokes("down enter");
    assert!(view.read_with(vcx, |view, _| matches!(&view.model.card, Some(Card::Print(p)) if p.pending_printer == "Printer1" && p.pending.is_some())));
    assert!(view.read_with(vcx, |view, _| !view.printer_menu.opened()
        && view.model.run.is_none()));
    // A popup choice captured before the source generation moved cannot target another activation.
    view.update_in(vcx, |view, window, cx| {
        if let Some(Card::Print(p)) = &mut view.model.card {
            p.pending = None;
        }
        view.activate(Control::Field(Focus::Printer), window, cx);
        view.model.selection.as_mut().unwrap().generation += 1;
    });
    vcx.simulate_keystrokes("down enter");
    assert!(view.read_with(
        vcx,
        |view, _| matches!(&view.model.card, Some(Card::Print(p)) if p.pending.is_none())
    ));
    view.update_in(vcx, |view, window, cx| {
        view.activate(Control::Field(Focus::Printer), window, cx);
    });
    vcx.run_until_parked();
    let second = view.read_with(vcx, |view, _| {
        let menu = view.printer_menu.menu.borrow();
        let card = menu.card.as_ref().unwrap();
        gpui::point(
            gpui::px(card.frame.x + 40.),
            gpui::px(card.row_screen_top(1) + 18.),
        )
    });
    vcx.simulate_mouse_down(second, gpui::MouseButton::Left, gpui::Modifiers::default());
    vcx.simulate_mouse_up(second, gpui::MouseButton::Left, gpui::Modifiers::default());
    assert!(view.read_with(vcx, |view, _| matches!(&view.model.card, Some(Card::Print(p)) if p.pending_printer == "Printer1" && p.pending.is_some())));
    view.update_in(vcx, |view, window, cx| {
        if let Some(Card::Print(p)) = &mut view.model.card {
            p.pending = None;
        }
        view.activate(Control::Field(Focus::Printer), window, cx);
    });
    vcx.run_until_parked();
    let outside = gpui::point(gpui::px(1.), gpui::px(1.));
    vcx.simulate_mouse_down(outside, gpui::MouseButton::Left, gpui::Modifiers::default());
    vcx.simulate_mouse_up(outside, gpui::MouseButton::Left, gpui::Modifiers::default());
    assert!(view.read_with(vcx, |view, _| view.model.card.is_some()
        && !view.printer_menu.opened()));
    view.update_in(vcx, |view, window, cx| {
        view.activate(Control::Field(Focus::Printer), window, cx);
        view.model.backend_died(&view.catalog);
        view.flush(cx);
        assert!(!view.printer_menu.opened());
    });
    vcx.simulate_keystrokes("escape");
    assert!(view.read_with(vcx, |view, _| view.model.card.is_none()));
}
