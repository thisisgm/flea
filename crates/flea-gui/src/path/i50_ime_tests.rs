//! Mounted path composition must retain current geometry and delay navigation until commit.
use super::*;
use crate::list::Typeface;
use crate::path::{
    paint::Size,
    rank,
    suggestions::{Ranker, Unranked},
    view::{Inputs, Out},
    Answer, Jumped, Peeked,
};
use gpui::{
    div, prelude::*, px, size, AppContext, Entity, Render, Subscription, TestAppContext,
    VisualTestContext,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

struct Host {
    path: Entity<PathView>,
    width: f32,
    height: f32,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
                .absolute()
                .left(px(80.))
                .top(px(20.))
                .w(px(self.width))
                .h(px(self.height))
                .child(self.path.clone()),
        )
    }
}

struct Bar<'a> {
    host: Entity<Host>,
    view: Entity<PathView>,
    visual: &'a mut VisualTestContext,
    events: Rc<RefCell<Vec<Out>>>,
    _subscription: Subscription,
}

fn bar(cx: &mut TestAppContext, ranker: Box<dyn Ranker>) -> Bar<'_> {
    let (host, visual) = cx.add_window_view(|_, cx| Host {
        path: cx.new(|cx| PathView::new(Arc::new(crate::test_catalogs::english()), ranker, cx)),
        width: 280.,
        height: 26.,
    });
    visual.simulate_resize(size(px(700.), px(360.)));
    let view = host.read_with(visual, |host, _| host.path.clone());
    let events = Rc::new(RefCell::new(Vec::new()));
    let log = events.clone();
    let subscription = visual.update(|_, cx| {
        cx.subscribe(&view, move |_, out: &Out, _| {
            log.borrow_mut().push(out.clone())
        })
    });
    let mut result = Bar {
        host,
        view,
        visual,
        events,
        _subscription: subscription,
    };
    result.update(|view, window, cx| {
        view.sync(
            Inputs {
                paths: ["/test".into(), Default::default()],
                home: "/test".into(),
                ..Default::default()
            },
            cx,
        );
        view.open(window, cx);
    });
    result
}

impl Bar<'_> {
    fn update<R>(
        &mut self,
        apply: impl FnOnce(&mut PathView, &mut Window, &mut Context<PathView>) -> R,
    ) -> R {
        let view = self.view.clone();
        let result = self
            .visual
            .update(|window, cx| view.update(cx, |view, cx| apply(view, window, cx)));
        self.visual.run_until_parked();
        result
    }
    fn read<R>(&mut self, apply: impl FnOnce(&PathView) -> R) -> R {
        self.view.read_with(self.visual, |view, _| apply(view))
    }
    fn jump_id(&self) -> u64 {
        self.events
            .borrow()
            .iter()
            .rev()
            .find_map(|event| match event {
                Out::Send { line, .. } if crate::path::is_jump_line(line) => {
                    flea::json::field_usize(line, "id").map(|id| id as u64)
                }
                _ => None,
            })
            .expect("the mounted path emitted its jump request")
    }
    fn opens(&self) -> Vec<Out> {
        self.events
            .borrow()
            .iter()
            .filter(|event| matches!(event, Out::Open { .. } | Out::Network(_)))
            .cloned()
            .collect()
    }
}

#[gpui::test]
fn i50_path_preedit_bounds(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Unranked));
    let text = format!("{}\u{1d11e}\u{65e5}{}", "a".repeat(35), "z".repeat(35));
    let end = text.encode_utf16().count();
    bar.update(|view, window, cx| {
        view.replace_and_mark_text_in_range(None, &text, Some(end..end), window, cx);
        assert_eq!(view.marked_text_range(window, cx), Some(0..end));
        assert_eq!(
            view.selected_text_range(false, window, cx).unwrap().range,
            end..end
        );
        let mut adjusted = None;
        assert_eq!(
            view.text_for_range(36..37, &mut adjusted, window, cx)
                .as_deref(),
            Some("\u{1d11e}")
        );
        assert_eq!(adjusted, Some(35..37));
    });
    bar.update(|view, window, cx| {
        assert!(
            view.line.borrow().scroll > px(0.),
            "long preedit must scroll the actual field"
        );
        let frame = view.paint.borrow().frame.unwrap();
        let caret = view
            .bounds_for_range(end..end, Default::default(), window, cx)
            .unwrap();
        assert_eq!(
            (caret.top(), caret.bottom()),
            (frame.top() - px(3.), frame.bottom() + px(3.))
        );
        assert!(caret.left() >= frame.left() && caret.left() <= frame.right());
        assert_eq!(
            view.character_index_for_point(caret.origin, window, cx),
            Some(end)
        );
        assert_eq!(
            view.line.borrow().run.as_ref().unwrap().line().font_size,
            view.paint.borrow().pixels(Size::Field)
        );
        // Launches install the fonts first; only then does the field follow the window's text size.
        cx.set_global(crate::presentation::Fonts::from_sources(
            "",
            "",
            "monospace".into(),
        ));
        window.set_rem_size(px(20.));
        cx.notify();
    });
    bar.host.update(bar.visual, |host, cx| {
        host.width = 190.;
        host.height = 36.;
        cx.notify();
    });
    bar.visual.run_until_parked();
    bar.update(|view, window, cx| {
        assert_eq!(
            view.line.borrow().run.as_ref().unwrap().line().font_size,
            px(18.),
            "the native bounds must use the new shaped size without a draft edit"
        );
        let frame = view.paint.borrow().frame.unwrap();
        let caret = view
            .bounds_for_range(end..end, Default::default(), window, cx)
            .unwrap();
        assert_eq!(
            (caret.top(), caret.bottom()),
            (frame.top() - px(3.), frame.bottom() + px(3.))
        );
        assert!(caret.left() >= frame.left() && caret.left() <= frame.right());
        assert_eq!(
            view.character_index_for_point(caret.origin, window, cx),
            Some(end)
        );
    });
    let run = bar.read(|view| view.line.borrow().run.clone().unwrap());
    bar.update(|_, _, cx| {
        let mut face = Typeface::omarchy_default();
        face.font.weight = gpui::FontWeight::BOLD;
        cx.set_global(face);
        cx.notify();
    });
    assert!(!Rc::ptr_eq(
        &run,
        &bar.read(|view| view.line.borrow().run.clone().unwrap())
    ));
}

#[gpui::test]
fn i50_path_suggestion_commit_after_compose(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(rank::Jump::default()));
    let paths = vec![
        "/test/Projects-e\u{301}".to_owned(),
        "/test/Projects-\u{65e5}".to_owned(),
    ];
    let id = bar.jump_id();
    bar.update(|view, window, cx| {
        view.receive(
            Answer::Jumped(Jumped {
                id,
                favourites: paths.clone(),
                ..Default::default()
            }),
            window,
            cx,
        );
        view.history_read(Vec::new(), cx);
    });
    let id = bar.jump_id();
    bar.update(|view, window, cx| {
        view.receive(
            Answer::Jumped(Jumped {
                id,
                favourites: paths.clone(),
                ..Default::default()
            }),
            window,
            cx,
        );
        view.replace_and_mark_text_in_range(None, "pro", Some(3..3), window, cx);
    });
    assert_eq!(bar.read(|view| view.jump.entries.len()), 2);
    let cursor = bar.read(|view| view.jump.cursor);
    bar.visual.simulate_keystrokes("down");
    assert_eq!(
        bar.read(|view| view.jump.cursor),
        cursor,
        "candidate arrows must not step the app's suggestions"
    );
    bar.visual.simulate_keystrokes("enter");
    assert!(bar.read(|view| view.entry.editing && view.entry.marked.is_some()));
    assert!(bar.opens().is_empty(), "candidate Enter cannot navigate");
    bar.update(|view, _, cx| view.commit(cx));
    bar.update(|view, _, cx| view.choose(paths[0].clone(), cx));
    assert!(
        bar.opens().is_empty(),
        "a retained commit callback cannot navigate while composing"
    );
    bar.update(|view, window, cx| view.replace_text_in_range(None, "pro", window, cx));
    bar.visual.simulate_keystrokes("down");
    let chosen = bar.read(|view| view.jump.entries[view.jump.cursor.unwrap()].path.clone());
    bar.visual.simulate_keystrokes("enter enter");
    bar.update(|view, _, cx| {
        view.commit(cx);
        view.choose(paths[0].clone(), cx);
    });
    assert_eq!(
        bar.opens(),
        [Out::Open {
            side: 0,
            path: chosen
        }]
    );
    assert_eq!(
        bar.events
            .borrow()
            .iter()
            .filter(|event| **event == Out::Closed)
            .count(),
        1,
        "the completed source dispatches its close exactly once"
    );
}

#[gpui::test]
fn i50_path_pending_completion_preserves_preedit(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Unranked));
    bar.update(|view, window, cx| view.replace_text_in_range(None, "Doc", window, cx));
    bar.visual.simulate_keystrokes("tab");
    bar.update(|view, window, cx| {
        view.entry.select_all();
        view.replace_and_mark_text_in_range(None, "Docu", Some(4..4), window, cx);
        view.receive(
            Answer::Peeked(Peeked {
                path: "/test".into(),
                dirs: vec!["Documents".into()],
                ..Default::default()
            }),
            window,
            cx,
        );
        assert_eq!(
            view.entry.text, "Docu",
            "a pending Tab answer cannot replace an IME draft"
        );
        assert_eq!(view.marked_text_range(window, cx), Some(0..4));
    });
    assert!(bar.opens().is_empty());
    let seen = bar.events.borrow().len();
    bar.update(|view, window, cx| view.replace_text_in_range(None, "Docu", window, cx));
    bar.visual.simulate_keystrokes("tab");
    assert_eq!(bar.read(|view| view.entry.text.clone()), "Documents/");
    assert!(
        !bar.events.borrow()[seen..]
            .iter()
            .any(|event| matches!(event, Out::Send { .. })),
        "the reply kept during composition answers the next Tab"
    );
}

#[gpui::test]
fn i50_path_reopened_owner_refuses_old_handler(cx: &mut TestAppContext) {
    let mut bar = bar(cx, Box::new(Unranked));
    bar.update(|view, window, cx| {
        view.replace_and_mark_text_in_range(None, "draft", None, window, cx);
        view.close(cx);
        let text = view.entry.text.clone();
        view.replace_text_in_range(None, "closed", window, cx);
        assert_eq!(view.entry.text, text);
        assert!(view.selected_text_range(false, window, cx).is_none());
        view.open(window, cx);
        let text = view.entry.text.clone();
        view.replace_and_mark_text_in_range(None, "stale", None, window, cx);
        assert_eq!(view.entry.text, text, "a new session needs its own paint");
        assert!(view.marked_text_range(window, cx).is_none());
    });
    bar.update(|view, window, cx| {
        view.replace_text_in_range(None, "/test/\u{65e5}", window, cx);
        view.commit(cx);
        view.commit(cx);
    });
    assert_eq!(
        bar.opens(),
        [Out::Open {
            side: 0,
            path: "/test/\u{65e5}".into()
        }]
    );
}

#[path = "i50_mozc_tests.rs"]
mod mozc;
