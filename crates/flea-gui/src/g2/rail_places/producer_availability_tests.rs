//! A cached unavailable diagnostic must not outlive a place returning.
use super::*;
use flea::ui_logic_rail_places::rail::{FavouriteError, RecordError};

#[gpui::test]
fn fx462_a_returned_favourite_retries_in_each_window_and_pane(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new();
    let target = fixture.0.join("late");
    let record = favourite(0, target.to_str().unwrap());
    let state = Json::Obj(vec![(
        "places".into(),
        Json::Obj(vec![
            ("showHome".into(), Json::Bool(false)),
            ("showTrash".into(), Json::Bool(false)),
            ("favourites".into(), Json::Arr(vec![record])),
        ]),
    )]);
    let backend = crate::cargo_artifacts::binary("flea");
    assert!(backend.is_file(), "build the matching flea backend first");
    std::fs::create_dir_all(fixture.0.join("state/flea")).unwrap();
    std::fs::write(
        fixture.0.join("state/flea/ui.json"),
        flea::jsondoc::render(&state),
    )
    .unwrap();
    let exe = fixture.0.join("inspect");
    flea::executable_file::write(
        &exe,
        format!(
            "#!/bin/sh\nXDG_STATE_HOME={} exec {} \"$@\"\n",
            quoted(&fixture.0.join("state")),
            quoted(&backend),
        )
        .as_bytes(),
        0o700,
    )
    .unwrap();
    let producers: Vec<_> = (0..2)
        .map(|_| {
            cx.new(|cx| {
                let mut producer = Producer::new(
                    Arc::new(crate::test_catalogs::english()),
                    &state,
                    exe.clone(),
                    cx,
                );
                producer.fixture_home("/fixture/home", "", cx);
                producer
                    .places
                    .update(cx, |places, cx| places.inspect(&[0], cx));
                producer
            })
        })
        .collect();
    cx.run_until_parked();
    for producer in &producers {
        producer.read_with(cx, |producer, _| {
            assert!(matches!(
                producer.entry(0).unwrap().error,
                Some(FavouriteError::Status(_))
            ));
        });
    }
    std::fs::create_dir(&target).unwrap();
    for producer in &producers {
        for side in [0, 1] {
            producer.update(cx, |producer, cx| {
                producer.side = side;
                producer.activate(0, cx);
                let requests = producer.drain_events();
                assert_eq!(requests.len(), 1, "a cached error swallowed the activation");
                assert_eq!(requests[0].side, side);
                assert_eq!(requests[0].event, Event::Open(target.display().to_string()));
            });
        }
    }
    cx.run_until_parked();
    for producer in &producers {
        producer.read_with(cx, |producer, _| {
            assert!(
                producer.entry(0).unwrap().error.is_none(),
                "the returned place stays red"
            );
        });
    }
}

#[gpui::test]
fn fx462_invalid_records_still_cannot_activate(cx: &mut gpui::TestAppContext) {
    let state = flea::jsondoc::parse(r#"{"places":{"showHome":false,"showTrash":false,"favourites":[{"label":"","path":"/missing"}]}}"#).unwrap();
    let producer = cx.new(|cx| {
        let mut producer = Producer::new(
            Arc::new(crate::test_catalogs::english()),
            &state,
            "/bin/false".into(),
            cx,
        );
        producer.places.update(cx, |places, _| {
            places
                .favourites
                .statuses
                .insert(0, "unavailable (NotFound)".into());
        });
        producer.fixture_home("/fixture/home", "", cx);
        producer
    });
    producer.update(cx, |producer, cx| {
        assert_eq!(
            producer.entry(0).unwrap().error,
            Some(FavouriteError::Record(RecordError::LabelBlank))
        );
        producer.activate(0, cx);
        assert!(producer.drain_events().is_empty());
    });
}

#[gpui::test]
fn fx462_a_rail_place_with_an_old_status_can_activate(cx: &mut gpui::TestAppContext) {
    let producer = cx.new(|cx| {
        Producer::from_sections(
            Arc::new(crate::test_catalogs::english()),
            vec![flea::ui_logic_rail_places::order::RailSection {
                heading: flea::ui_logic_rail_places::order::Heading::None,
                rows: vec![RailEntry {
                    path: "/returned".into(),
                    error: Some(FavouriteError::Status("unavailable (NotFound)".into())),
                    ..RailEntry::default()
                }],
            }],
            "/bin/false".into(),
            cx,
        )
    });
    producer.update(cx, |producer, cx| {
        producer.activate(0, cx);
        assert_eq!(
            producer.drain_events()[0].event,
            Event::Open("/returned".into())
        );
    });
}
