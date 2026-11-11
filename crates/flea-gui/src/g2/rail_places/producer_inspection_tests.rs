//! The painted rail asks the real favorites CLI about its visible local paths.
use super::*;

use std::process::Command;

struct Rail(gpui::Entity<Producer>);

impl Render for Rail {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(gpui::px(192.))
            .h(gpui::px(160.))
            .child(self.0.clone())
    }
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let out = Command::new("mktemp")
            .args(["-d", "/tmp/flea-rail-favorites.XXXXXXXX"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let root = PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
        std::fs::write(root.join(".flea-test-sandbox"), b"rail favorites").unwrap();
        Self(root)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(self.0.is_absolute());
        assert_eq!(self.0.parent(), Some(std::path::Path::new("/tmp")));
        assert!(self.0.join(".flea-test-sandbox").is_file());
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn quoted(path: &std::path::Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn favourite(i: usize, path: &str) -> Json {
    Json::Obj(vec![
        ("label".into(), Json::Str(format!("F{i}"))),
        ("path".into(), Json::Str(path.into())),
    ])
}

// Forty favourites whose inspector logs every call and always fails, drawn once.
fn failing_rail(
    cx: &mut gpui::TestAppContext,
) -> (
    Fixture,
    PathBuf,
    gpui::Entity<Producer>,
    &mut gpui::VisualTestContext,
) {
    let fixture = Fixture::new();
    let exe = fixture.0.join("inspect");
    let calls = fixture.0.join("calls");
    std::fs::write(
        &exe,
        format!(
            "#!/bin/sh\nprintf 'inspect\\n' >> {}\nexit 1\n",
            quoted(&calls)
        ),
    )
    .unwrap();
    flea::executable_file::write(&exe, &std::fs::read(&exe).unwrap(), 0o700).unwrap();
    let records = (0..40)
        .map(|i| favourite(i, &format!("/missing/{i}")))
        .collect();
    let state = Json::Obj(vec![(
        "places".into(),
        Json::Obj(vec![
            ("showHome".into(), Json::Bool(false)),
            ("showTrash".into(), Json::Bool(false)),
            ("favourites".into(), Json::Arr(records)),
        ]),
    )]);
    let (parent, view) = cx.add_window_view(|_, cx| {
        Rail(cx.new(|cx| {
            let mut producer =
                Producer::new(Arc::new(crate::test_catalogs::english()), &state, exe, cx);
            producer.fixture_home("/fixture/home", "", cx);
            producer.set_polling(true, cx);
            producer
        }))
    });
    let producer = parent.read_with(view, |rail, _| rail.0.clone());
    draw(view);
    (fixture, calls, producer, view)
}

fn time_out(producer: &gpui::Entity<Producer>, view: &mut gpui::VisualTestContext) {
    producer.update(view, |producer, cx| {
        producer.places.update(cx, |places, cx| {
            places.favourites.inspected(None, "");
            cx.notify();
        });
    });
    draw(view);
}

#[gpui::test]
fn fx_uish_def3_a_record_set_restored_during_inspection_retries_after_timeout(
    cx: &mut gpui::TestAppContext,
) {
    let (_fixture, calls, producer, view) = failing_rail(cx);
    let before = std::fs::read_to_string(&calls).unwrap().lines().count();
    let hang = |producer: &gpui::Entity<Producer>, view: &mut gpui::VisualTestContext| {
        producer.update(view, |producer, cx| {
            producer
                .places
                .update(cx, |places, _| places.favourites.inspecting = true);
        });
    };
    hang(&producer, view);
    time_out(&producer, view);
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap().lines().count(),
        before,
        "a timeout over an unchanged record set starts no read"
    );
    hang(&producer, view);
    // The visible set changes and returns without a scroll while the first read hangs.
    for path in ["/moved/0", "/missing/0"] {
        producer.update(view, |producer, cx| {
            producer.places.update(cx, |places, cx| {
                places.favourites.records[0] = favourite(0, path);
                cx.notify();
            });
        });
        draw(view);
    }
    time_out(&producer, view);
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap().lines().count(),
        before + 1
    );
}

#[gpui::test]
fn fx_uish_def3_a_viewport_changed_during_inspection_retries_after_timeout(
    cx: &mut gpui::TestAppContext,
) {
    let (_fixture, calls, producer, view) = failing_rail(cx);
    let before = std::fs::read_to_string(&calls).unwrap();
    let visible = producer.read_with(view, |producer, _| {
        producer
            .laid
            .borrow()
            .rows
            .iter()
            .map(|row| row.index)
            .collect::<Vec<_>>()
    });
    producer.update(view, |producer, cx| {
        producer
            .places
            .update(cx, |places, _| places.favourites.inspecting = true);
    });
    for delta in [-1., 1.] {
        view.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(gpui::px(60.), gpui::px(80.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(delta))),
            ..Default::default()
        });
        draw(view);
        producer.read_with(view, |producer, _| {
            assert_eq!(
                producer.model.borrow().scroll,
                if delta < 0. { 1. } else { 0. }
            );
            assert_eq!(
                producer
                    .laid
                    .borrow()
                    .rows
                    .iter()
                    .map(|row| row.index)
                    .collect::<Vec<_>>(),
                visible,
                "the viewport moved without changing its visible records"
            );
        });
    }
    time_out(&producer, view);
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap().lines().count(),
        before.lines().count() + 1
    );
    let retried = std::fs::read_to_string(&calls).unwrap();
    draw(view);
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap(),
        retried,
        "redraws must not loop failed inspections"
    );
    producer.update(view, |producer, cx| {
        producer.places.update(cx, |places, _| {
            places.favourites.inspecting = false;
            places.favourites.statuses = (0..40).map(|i| (i, String::new())).collect();
        });
    });
    for delta in [-1., 1.] {
        view.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(gpui::px(60.), gpui::px(80.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(delta))),
            ..Default::default()
        });
        draw(view);
    }
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap(),
        retried,
        "a scroll over answered favourites starts no read"
    );
}

#[gpui::test]
fn rail_favourites_inspect_real_visible_paths_and_newly_scrolled_rows(
    cx: &mut gpui::TestAppContext,
) {
    let fixture = Fixture::new();
    let root = &fixture.0;
    let backend = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("flea");
    assert!(
        backend.is_file(),
        "build the matching flea backend before this test: {}",
        backend.display()
    );
    std::fs::create_dir(root.join("healthy")).unwrap();
    std::fs::create_dir_all(root.join("state/flea")).unwrap();
    let records: Vec<_> = (0..12)
        .map(|index| {
            Json::Obj(vec![
                ("label".into(), Json::Str(format!("Favorite {index}"))),
                (
                    "path".into(),
                    Json::Str(
                        root.join(if index == 0 {
                            "healthy".into()
                        } else {
                            format!("missing-{index}")
                        })
                        .display()
                        .to_string(),
                    ),
                ),
            ])
        })
        .collect();
    let state = Json::Obj(vec![(
        "places".into(),
        Json::Obj(vec![
            ("showHome".into(), Json::Bool(false)),
            ("showTrash".into(), Json::Bool(false)),
            ("favourites".into(), Json::Arr(records)),
        ]),
    )]);
    std::fs::write(
        root.join("state/flea/ui.json"),
        flea::jsondoc::render(&state),
    )
    .unwrap();
    let exe = root.join("inspect");
    std::fs::write(&exe, format!(
        "#!/bin/sh\nexport XDG_STATE_HOME={}\nexport XDG_CONFIG_HOME={}\nprintf '%s\\n' \"$2\" >> {}\nexec {} \"$@\"\n",
        quoted(&root.join("state")), quoted(root), quoted(&root.join("requests")), quoted(&backend),
    )).unwrap();
    flea::executable_file::write(&exe, &std::fs::read(&exe).unwrap(), 0o700).unwrap();
    let (parent, view) = cx.add_window_view(|_, cx| {
        Rail(cx.new(|cx| {
            let mut producer =
                Producer::new(Arc::new(crate::test_catalogs::english()), &state, exe, cx);
            producer.fixture_home("/fixture/home", "", cx);
            producer
        }))
    });
    let producer = parent.read_with(view, |rail, _| rail.0.clone());
    draw(view);
    assert!(
        !root.join("requests").exists(),
        "a rail that cannot poll inspects nothing"
    );
    producer.update(view, |producer, cx| producer.set_polling(true, cx));
    draw(view);
    let checked = producer.read_with(view, |producer, cx| {
        let statuses = &producer.places.read(cx).favourites.statuses;
        assert_eq!(statuses.get(&0).map(String::as_str), Some(""));
        assert!(statuses
            .get(&1)
            .is_some_and(|error| error.contains("NotFound")));
        assert!(
            !statuses.contains_key(&11),
            "offscreen favorites must not be inspected"
        );
        let row = producer
            .entries()
            .into_iter()
            .find(|entry| entry.favourite_index == Some(1))
            .unwrap();
        assert!(matches!(
            row.error,
            Some(flea::ui_logic_rail_places::rail::FavouriteError::Status(_))
        ));
        statuses.len()
    });
    let requests = std::fs::read_to_string(root.join("requests")).unwrap();
    draw(view);
    assert_eq!(
        std::fs::read_to_string(root.join("requests")).unwrap(),
        requests,
        "a redraw never repeats an inspection"
    );
    producer.update(view, |producer, cx| {
        producer.model.borrow_mut().scroll = 300.;
        cx.notify();
    });
    draw(view);
    producer.read_with(view, |producer, cx| {
        let statuses = &producer.places.read(cx).favourites.statuses;
        assert!(statuses.len() > checked);
        assert!(statuses
            .get(&11)
            .is_some_and(|error| error.contains("NotFound")));
    });
}
