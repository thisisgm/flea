use super::{
    host::{OutputEvent, OutputView},
    model::{Action, Card, Model},
    seam::Effect,
    tests::selected,
    wire,
};
use flea::{jsondoc, ui_logic_op4::output::Kind};
use gpui::TestAppContext;
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn ready(model: &mut Model) {
    let ticket = match model.card.as_ref().unwrap() {
        Card::Convert(c) => c.pending.unwrap(),
        Card::Combine(c) => c.pending.unwrap(),
        Card::Print(p) => {
            let id = p.pending.unwrap().request;
            assert!(model.line(&format!(r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{{"name":"Office","default":true}}],"printer":"Office","duplex":true,"twoSided":false,"color":true,"grayscale":false}}"#), &crate::test_catalogs::english()));
            return;
        }
    };
    model.checked(ticket, Ok(()), &crate::test_catalogs::english());
}

#[test]
fn m9_g2rows_21_output_cards_keep_captured_selection() {
    let catalog = crate::test_catalogs::english();
    for (action, command) in [
        (Action::Print, "print"),
        (Action::Convert, "convertbatch"),
        (Action::Combine, "combine"),
    ] {
        let captured = selected(Kind::Image, &["z.png", "a.png"]);
        let mut model = Model::default();
        assert!(model.open(action, captured.clone(), &catalog));
        ready(&mut model);
        let mut changed = selected(Kind::Image, &["other.png", "new.png"]);
        changed.generation += 1;
        assert!(!model.open(action, changed, &catalog));
        assert_eq!(model.selection.as_ref(), Some(&captured));
        model.submit(&catalog);
        let line = model
            .effects
            .iter()
            .rev()
            .find_map(|effect| match effect {
                Effect::Send(line) => Some(line),
                _ => None,
            })
            .unwrap();
        let request = jsondoc::parse(line).unwrap();
        assert_eq!(wire::text(&request, "c"), Some(command));
        assert_eq!(wire::number(&request, "menuId"), Some(captured.menu));
        assert_eq!(
            wire::strings(&request, "paths").unwrap(),
            ["/fixture/z.png", "/fixture/a.png"]
        );
        let ticket = model.run.as_ref().unwrap().ticket;
        let stem = if action == Action::Print {
            "print"
        } else if action == Action::Convert {
            "convert"
        } else {
            "combine"
        };
        model.effects.clear();
        assert!(!model.line(
            &format!(
                r#"{{"t":"{stem}started","id":7,"requestId":{},"source":"/fixture/z.png"}}"#,
                ticket.request + 1
            ),
            &catalog
        ));
        assert!(model.effects.is_empty());
        assert!(model.run.as_ref().unwrap().operation.is_none());
        if action == Action::Convert {
            assert!(!model.line(&format!(r#"{{"t":"convertstarted","id":7,"requestId":{},"source":"/fixture/other.png"}}"#, ticket.request), &catalog));
        }
    }
}

#[test]
fn m9_g2rows_21_convert_and_combine_validate_output_artifacts() {
    super::m9_g2rows_21_artifacts::check_artifacts();
}

#[gpui::test]
fn m9_g2rows_21_print_options_refusal_and_cancel_use_existing_wire(cx: &mut TestAppContext) {
    let catalog = crate::test_catalogs::english();
    let mut print = Model::default();
    assert!(print.open(
        Action::Print,
        selected(Kind::Pdf, &["report.pdf"]),
        &catalog
    ));
    ready(&mut print);
    if let Some(Card::Print(p)) = &mut print.card {
        p.copies = "3".into();
        p.pages = "1-3, 5".into();
        p.two_sided = true;
        p.grayscale = true;
    }
    print.submit(&catalog);
    let request = print.run.as_ref().unwrap().ticket;
    let line = print
        .effects
        .iter()
        .rev()
        .find_map(|effect| match effect {
            Effect::Send(line) => Some(line),
            _ => None,
        })
        .unwrap();
    let json = jsondoc::parse(line).unwrap();
    assert_eq!(wire::number(&json, "copies"), Some(3));
    assert_eq!(wire::text(&json, "pages"), Some("1-3, 5"));
    assert_eq!(wire::boolean(&json, "twoSided"), Some(true));
    assert_eq!(wire::boolean(&json, "grayscale"), Some(true));
    assert!(print.line(
        &format!(
            r#"{{"t":"printdone","id":7,"requestId":{},"ok":false,"err":"queue refused"}}"#,
            request.request
        ),
        &catalog
    ));
    assert!(print.error.contains("queue refused"));
    assert!(!print
        .effects
        .iter()
        .any(|e| matches!(e, Effect::Progress { .. })));

    for action in [Action::Convert, Action::Combine] {
        for early in [false, true] {
            let (view, vcx) = cx.add_window_view(|window, cx| {
                let palette = crate::g2::op1::palette::CardPalette::from_theme(
                    &window.text_style(),
                    crate::palette::Theme::default(),
                    8.,
                );
                OutputView::new(Arc::new(crate::test_catalogs::english()), palette, cx)
            });
            let sent = Rc::new(RefCell::new(Vec::new()));
            let capture = sent.clone();
            let _subscription = vcx.update(|_, cx| {
                cx.subscribe(&view, move |_, event: &OutputEvent, _| {
                    if let Effect::Send(line) = &event.0 {
                        capture.borrow_mut().push(line.clone());
                    }
                })
            });
            let stem = if action == Action::Convert {
                "convert"
            } else {
                "combine"
            };
            let request = view.update_in(vcx, |view, window, cx| {
                assert!(view.open(
                    action,
                    selected(Kind::Image, &["a.png", "b.png"]),
                    window,
                    cx
                ));
                ready(&mut view.model);
                view.model.submit(&view.catalog);
                view.flush(cx);
                view.model.run.as_ref().unwrap().ticket.request
            });
            vcx.run_until_parked();
            sent.borrow_mut().clear();
            if !early {
                view.update(vcx, |view, cx| {
                    assert!(view.line(&format!(r#"{{"t":"{stem}started","id":19,"requestId":{request},"source":"/fixture/a.png"}}"#), cx));
                });
            }
            if early {
                vcx.simulate_keystrokes("escape");
                assert!(
                    sent.borrow().is_empty(),
                    "no unscoped cancel before the operation id"
                );
                view.update(vcx, |view, cx| {
                    assert!(view.line(&format!(r#"{{"t":"{stem}started","id":19,"requestId":{request},"source":"/fixture/a.png"}}"#), cx));
                });
            } else {
                vcx.run_until_parked();
                let cancel = vcx.debug_bounds("op4-control-Cancel").unwrap();
                vcx.simulate_click(cancel.center(), gpui::Modifiers::default());
            }
            vcx.run_until_parked();
            assert_eq!(
                *sent.borrow(),
                [r#"{"c":"transfercancel","id":19}"#],
                "Cancel must reach only the accepted output job"
            );
            view.update(vcx, |view, _| {
                assert!(
                    !view.model.can_cancel(),
                    "Cancel is disabled while awaiting acknowledgement"
                );
            });
            vcx.simulate_keystrokes("escape escape");
            assert_eq!(sent.borrow().len(), 1, "repeated Cancel is idempotent");
            view.update_in(vcx, |view, window, cx| {
                assert!(view.model.busy && view.model.card.is_some(), "keep the complete card until acknowledgement");
                assert!(!view.line(&format!(r#"{{"t":"{stem}done","id":20,"requestId":{request},"source":"/fixture/a.png","ok":false,"paths":[],"path":"","err":"cancelled"}}"#), cx));
                assert!(view.model.busy);
                assert!(view.line(&format!(r#"{{"t":"{stem}done","id":19,"requestId":{request},"source":"/fixture/a.png","ok":false,"paths":[],"path":"","err":"cancelled"}}"#), cx));
                assert!(!view.model.busy && view.model.card.is_none());
                assert!(view.model.error.is_empty());
                assert!(view.open(action, selected(Kind::Image, &["next.png", "other.png"]), window, cx));
                assert!(!view.line(&format!(r#"{{"t":"{stem}done","id":19,"requestId":{request},"source":"/fixture/a.png","ok":false,"paths":[],"path":"","err":"cancelled"}}"#), cx));
                assert!(view.model.card.is_some(), "a stale cancellation cannot close the next draft");
            });
        }
    }
}
