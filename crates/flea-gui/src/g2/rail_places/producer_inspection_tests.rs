//! The painted rail asks the real favorites CLI about its visible local paths.
use super::*;
use std::os::unix::fs::PermissionsExt;
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
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
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
