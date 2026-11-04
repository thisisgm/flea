//! Removed rows must be harmless even when an actual older prepaint snapshot is retained.
use super::*;

#[gpui::test]
fn gcomp36_removed_settings_row_rejects_right_and_middle_from_retained_paint(
    cx: &mut gpui::TestAppContext,
) {
    for button in [MouseButton::Right, MouseButton::Middle] {
        let state =
            flea::jsondoc::parse(r#"{"places":{"showHome":true,"showTrash":true}}"#).unwrap();
        let (parent, view) = cx.add_window_view(|_, cx| SizedProducer {
            producer: cx.new(|cx| {
                let mut producer = Producer::new(
                    Arc::new(crate::test_catalogs::english()),
                    &state,
                    "/bin/false".into(),
                    cx,
                );
                // The real rail host enables polling before its producer can load.
                producer.set_polling(true, cx);
                producer
            }),
        });
        let producer = parent.read_with(view, |parent, _| parent.producer.clone());
        view.run_until_parked();
        producer.read_with(view, |producer, _| {
            assert!(
                producer.loaded,
                "the real background source load must finish before paint"
            );
            assert_eq!(producer.directory_reads, 1);
        });
        view.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        view.run_until_parked();
        producer.update(view, |producer, cx| {
            let prior = producer.laid.borrow().clone();
            let row = prior.rows.last().expect("a real saved place was painted");
            let position = gpui::point(gpui::px(row.rect.x + 2.), gpui::px(row.rect.y + 2.));
            let removed = row.index;
            producer.adopt_state(&flea::jsondoc::parse(
                r#"{"places":{"showHome":false,"showTrash":false,"showRecent":false,"showTags":false}}"#,
            ).unwrap(), cx);
            assert!(producer.entry(removed).is_none());
            assert!(producer.laid.borrow().rows.is_empty(), "normal rebuild must invalidate its hit geometry");
            producer.press(&MouseDownEvent { position, button, ..Default::default() }, cx);
            assert!(producer.drain_events().is_empty());
            // Replay real prior prepaint indices and geometry to exercise the local entry guard against a retained snapshot.
            *producer.laid.borrow_mut() = prior;
            producer.press(&MouseDownEvent { position, button, ..Default::default() }, cx);
            assert!(producer.drain_events().is_empty(), "stale hit enqueued a request for a removed row");
        });
    }
}

struct SizedProducer {
    producer: gpui::Entity<Producer>,
}

impl Render for SizedProducer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(gpui::px(192.))
            .h(gpui::px(600.))
            .child(self.producer.clone())
    }
}
