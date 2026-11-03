use super::{
    host::OutputView,
    model::{Action, Card, Model},
    seam::Effect,
    tests::selected,
};
use flea::ui_logic_op4::output::Kind;
use gpui::TestAppContext;
use std::sync::Arc;

#[test]
fn fix1_print_terminal_names_all_refusals_and_deduplicates_only_the_visible_failure() {
    let catalog = crate::test_catalogs::english();
    for (paths, outcomes) in [
        (vec!["a.pdf"], vec![false]),
        (vec!["a.pdf", "b.pdf"], vec![false, false]),
        (vec!["a.pdf", "b.pdf", "c.pdf"], vec![false, false, true]),
    ] {
        let mut model = Model::default();
        assert!(model.open(Action::Print, selected(Kind::Pdf, &paths), &catalog));
        let Some(Card::Print(p)) = &model.card else {
            panic!()
        };
        let options = p.pending.unwrap().request;
        assert!(model.line(&format!(r#"{{"t":"printoptions","id":{options},"ok":true,"destinations":[{{"name":"Office","default":true}}],"printer":"Office","duplex":false,"twoSided":false,"color":false,"grayscale":false,"setupArgv":[]}}"#), &catalog));
        model.submit(&catalog);
        let request = model.run.as_ref().unwrap().ticket.request;
        assert!(model.line(
            &format!(r#"{{"t":"printstarted","id":7,"requestId":{request}}}"#),
            &catalog
        ));
        model.effects.clear();
        let mut refused = Vec::new();
        for (name, ok) in paths.iter().zip(&outcomes) {
            let message = if *ok {
                format!("Sent {name} to Office")
            } else {
                format!("{name} was not printed: queue rejected the file.")
            };
            if !ok {
                refused.push(message.clone());
            }
            assert!(model.line(&format!(r#"{{"t":"printed","id":7,"requestId":{request},"path":"/fixture/{name}","ok":{ok},"message":"{message}"}}"#), &catalog));
        }
        let error = refused.join(" ");
        let sent = outcomes.iter().filter(|ok| **ok).count();
        assert!(model.line(&format!(r#"{{"t":"printdone","id":7,"requestId":{request},"ok":false,"sent":{sent},"message":"ignored last message","err":"{error}"}}"#), &catalog));
        let statuses = model
            .effects
            .iter()
            .filter_map(|e| match e {
                Effect::Status { error, text } => Some((*error, text.as_str())),
                _ => None,
            })
            .collect::<Vec<_>>();
        eprintln!(
            "refused={refused:?} terminal statuses={statuses:?}; error={:?}",
            model.error
        );
        assert_eq!(model.error, error);
        assert_eq!(statuses.last(), Some(&(true, error.as_str())));
        assert_eq!(
            statuses.len(),
            paths.len() + usize::from(refused.len() != 1)
        );
        assert!(!model.busy && model.run.is_none());
    }
}

#[gpui::test]
fn fix1_keyboard_tabs_reveal_metadata_and_pages_with_pinned_chrome(cx: &mut TestAppContext) {
    for (kind, name, downs, target) in [
        (Kind::Image, "a.png", 6, "op4-control-Check(Check)"),
        (Kind::Pdf, "report.pdf", 3, "op4-control-Field(Pages)"),
    ] {
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
        vcx.simulate_resize(gpui::size(gpui::px(800.), gpui::px(400.)));
        view.update_in(vcx, |view, window, cx| {
            assert!(view.open(Action::Convert, selected(kind, &[name]), window, cx));
            if kind == Kind::Pdf {
                let Some(Card::Convert(c)) = &view.model.card else {
                    panic!()
                };
                let ticket = c.count_pending.unwrap();
                view.model.document_pages(ticket, Ok(3), &view.catalog);
                view.flush(cx);
            }
        });
        vcx.run_until_parked();
        view.read_with(vcx, |view, _| view.body_scroll.painted.borrow_mut().clear());
        let title = vcx.debug_bounds("op4-title").unwrap();
        let buttons = vcx.debug_bounds("op4-buttons").unwrap();
        for _ in 0..downs {
            vcx.simulate_keystrokes("down");
        }
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
        let body = vcx.debug_bounds("op4-body").unwrap();
        let control = vcx.debug_bounds(target).unwrap();
        eprintln!("{kind:?} Tab target={control:?}; body={body:?}");
        assert!(
            control.top() >= body.top() && control.bottom() <= body.bottom(),
            "{kind:?} Tab target is clipped"
        );
        if kind == Kind::Image {
            let outline = control.dilate(gpui::px(4.));
            assert!(
                outline.top() >= body.top() && outline.bottom() <= body.bottom(),
                "metadata focus outline is clipped: {outline:?}; body={body:?}"
            );
        }
        assert_eq!(vcx.debug_bounds("op4-title").unwrap(), title);
        assert_eq!(vcx.debug_bounds("op4-buttons").unwrap(), buttons);
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
        assert_eq!(vcx.debug_bounds("op4-title").unwrap(), title);
        assert_eq!(vcx.debug_bounds("op4-buttons").unwrap(), buttons);
        vcx.simulate_keystrokes("shift-tab shift-tab");
        for _ in 0..downs {
            vcx.simulate_keystrokes("up");
        }
        vcx.run_until_parked();
        let first = vcx.debug_bounds("review-op4-format-0").unwrap();
        assert!(
            first.top() >= body.top() && first.bottom() <= body.bottom(),
            "first format did not reveal on return"
        );
        let frames = view.read_with(vcx, |view, _| view.body_scroll.painted.borrow().clone());
        assert!(!frames.is_empty(), "no control paints were observed");
        assert!(
            frames
                .iter()
                .all(|(target, viewport)| target.top() >= viewport.top()
                    && target.bottom() <= viewport.bottom()),
            "keyboard target was clipped in an intermediate painted frame: {frames:?}"
        );
        eprintln!(
            "{kind:?}: {} painted keyboard targets, none clipped",
            frames.len()
        );
    }
}
