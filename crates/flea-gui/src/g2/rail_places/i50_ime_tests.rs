//! Late platform callbacks cannot edit a closed or replaced rail session.
use super::producer::Producer;
use flea::ui_logic_rail_places::order::{Heading, RailSection};
use flea::ui_logic_rail_places::rail::{Group, Kind, RailEntry};
use gpui::{EntityInputHandler, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn i50_producer_stale_composition_refused(cx: &mut TestAppContext) {
    let sections = vec![RailSection {
        heading: Heading::Plugin("Network".into()),
        rows: vec![RailEntry {
            group: Group::Network,
            kind: Kind::Share,
            label: "NAS".into(),
            ..Default::default()
        }],
    }];
    let (producer, visual) = cx.add_window_view(|_, cx| {
        Producer::from_sections(
            Arc::new(crate::test_catalogs::english()),
            sections,
            "/bin/false".into(),
            cx,
        )
    });
    visual.update(|window, cx| {
        producer.update(cx, |producer, cx| {
            assert!(producer.start_rename(0, cx));
            producer.replace_and_mark_text_in_range(None, "stale", None, window, cx);
            assert_eq!(
                producer.rename_text(),
                "NAS",
                "an unpainted session cannot receive a retained handler's composition"
            );
            producer.cancel_rename(cx);
            producer.replace_text_in_range(None, "late", window, cx);
            assert_eq!(producer.rename_text(), "NAS");
            assert!(producer.drain_events().is_empty());
        })
    });
}

#[gpui::test]
fn i50_producer_live_composition_utf16_and_enter(cx: &mut TestAppContext) {
    use gpui::{div, prelude::*, Context, Entity, FocusHandle, IntoElement, Render, Window};
    struct Host {
        producer: Entity<Producer>,
        focus: FocusHandle,
    }
    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(gpui::px(192.))
                .h(gpui::px(600.))
                .track_focus(&self.focus)
                .child(self.producer.clone())
        }
    }
    let sections = vec![RailSection {
        heading: Heading::Plugin("Network".into()),
        rows: vec![RailEntry {
            group: Group::Network,
            kind: Kind::Share,
            label: "NAS".into(),
            ..Default::default()
        }],
    }];
    let (host, visual) = cx.add_window_view(|window, cx| {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let producer = cx.new(|cx| {
            Producer::from_sections(
                Arc::new(crate::test_catalogs::english()),
                sections,
                "/bin/false".into(),
                cx,
            )
        });
        producer.update(cx, |producer, cx| {
            producer.model.borrow_mut().focused = true;
            assert!(producer.start_rename(0, cx));
        });
        Host { producer, focus }
    });
    visual.run_until_parked();
    let producer = host.read_with(visual, |host, _| host.producer.clone());
    visual.update(|window, cx| {
        producer.update(cx, |producer, cx| {
            producer.replace_and_mark_text_in_range(
                None,
                "\u{1d11e}\u{65e5}",
                Some(0..2),
                window,
                cx,
            );
            assert_eq!(producer.marked_text_range(window, cx), Some(0..3));
            assert_eq!(
                producer
                    .selected_text_range(false, window, cx)
                    .unwrap()
                    .range,
                0..2
            );
            producer.commit_rename(cx);
            assert!(
                producer.rename_index().is_some(),
                "candidate Enter cannot commit the bookmark"
            );
            assert!(producer.drain_events().is_empty());
            producer.replace_text_in_range(None, "\u{5831}\u{544a}", window, cx);
            producer.commit_rename(cx);
            assert_eq!(producer.drain_events().len(), 1);
            producer.commit_rename(cx);
            assert!(producer.drain_events().is_empty());
            assert!(producer.start_rename(0, cx));
            producer.replace_and_mark_text_in_range(None, "stale", None, window, cx);
            assert_eq!(
                producer.rename_text(),
                "NAS",
                "the earlier session's handler cannot reach its replacement"
            );
        })
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
        producer.update(cx, |producer, cx| {
            producer.replace_sections(Vec::new(), cx);
            producer.replace_text_in_range(None, "late", window, cx);
            assert!(producer.selected_text_range(false, window, cx).is_none());
            assert!(producer.drain_events().is_empty());
        })
    });
}
