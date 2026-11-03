// Review round 1 probes for g2-pl4 in GPUI, kept as regression tests: the rail tag ground, the writer, the history reads, the menus and the manifest.
use super::content::{build, ink, theme_tags, RowMark};
use super::seams::ThemeRoles;
use super::service::Places;
use flea::i18n::{I18n, Paths};
use flea::jsondoc::Json;
use flea::ui_logic_pl4::favourites::Operation;
use flea::ui_logic_pl4::order::{rail_sections, Shown};
use flea::ui_logic_pl4::tagtheme::{parse, ratio_of};
use gpui::{AppContext, TestAppContext};
use std::path::PathBuf;
use std::sync::Arc;

fn english() -> I18n {
    I18n::load(Paths {
        omarchy: PathBuf::from("/nonexistent"),
        language_file: PathBuf::from("/nonexistent"),
        catalogs: vec![crate::test_catalogs::english_only()],
    })
}

fn record(label: &str, path: &str) -> Json {
    Json::Obj(vec![
        ("label".into(), Json::Str(label.into())),
        ("path".into(), Json::Str(path.into())),
    ])
}

// A fresh marked root of this test's own, created exclusively.
fn root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rev-pl4-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join(".flea-test-sandbox"), b"").unwrap();
    dir
}

fn drop_root(dir: &PathBuf) {
    let parent = std::env::temp_dir().canonicalize().ok();
    if dir.is_absolute()
        && dir.join(".flea-test-sandbox").is_file()
        && dir.parent().and_then(|p| p.canonicalize().ok()) == parent
    {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn review_the_rail_tag_swatch_holds_three_to_one_on_the_rail_surface() {
    // catppuccin-latte: the rail draws on the surface (dark_background), as the menu does.
    let roles = [
        ("red", "#d20f39"),
        ("orange", "#d84e2b"),
        ("yellow", "#df8e1d"),
        ("green", "#40a02b"),
        ("blue", "#1e66f5"),
        ("magenta", "#ea76cb"),
        ("dark_foreground", "#9ca0b0"),
    ];
    let theme = ThemeRoles {
        roles: roles.map(|(r, h)| (r.to_owned(), h.to_owned())).to_vec(),
        background: "#eff1f5".into(),
        surface: "#e3e4e8".into(),
    };
    let s = rail_sections(
        &[],
        Shown {
            tags: true,
            ..Shown::default()
        },
        Vec::new(),
        &[true; 7],
        Vec::new(),
        &[],
    );
    let drawn = build(&english(), &s, &theme_tags(&theme));
    let tags = drawn
        .iter()
        .find(|d| d.heading.as_ref() == "TAGS")
        .expect("TAGS");
    let tt = theme_tags(&theme);
    let mut under = Vec::new();
    for (at, row) in tags.rows.iter().enumerate() {
        let RowMark::Tag(mark) = row.mark else {
            panic!("a tag mark")
        };
        // The probe measures the colour the rail draws, which must be the surface-lifted one.
        let hex = [&tt.colors[at], &tt.menu_colors[at]]
            .into_iter()
            .find(|hex| ink(hex) == mark.fill)
            .expect("a theme colour");
        let r = ratio_of(parse(hex).unwrap(), parse("#e3e4e8").unwrap());
        if r < 3.0 {
            under.push(format!(
                "{} {hex} {r:.2} (listing colour {}, menu colour {})",
                row.label, tt.colors[at], tt.menu_colors[at]
            ));
        }
    }
    assert!(
        under.is_empty(),
        "rail swatches under 3:1 on the rail surface: {under:?}"
    );
}

#[gpui::test]
fn review_a_second_edit_names_the_records_the_first_committed(cx: &mut TestAppContext) {
    let dir = root("cli");
    let exe = dir.join("flea-fake");
    let argv = dir.join("argv");
    std::fs::write(&exe, format!("#!/bin/sh\nprintf '%s\\n' \"$2\" >> '{}'\nprintf '%s' '{{\"places\":{{\"favourites\":[{{\"label\":\"B\",\"path\":\"/b\"}}]}}}}'\n", argv.display())).unwrap();
    std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let places = cx.new(|_| Places::new(Arc::new(english()), exe.clone()));
    places.update(cx, |p, _| {
        p.favourites.records = vec![record("A", "/a"), record("B", "/b")]
    });
    places.update(cx, |p, cx| p.write(&Operation::Remove { index: 0 }, cx));
    cx.run_until_parked();
    assert_eq!(
        places.read_with(cx, |p, _| p.said.clone()),
        None,
        "the fake CLI ran and committed"
    );
    places.update(cx, |p, cx| p.write(&Operation::Remove { index: 0 }, cx));
    cx.run_until_parked();
    let sent = std::fs::read_to_string(&argv).unwrap();
    let second = sent.lines().nth(1).unwrap_or("").to_owned();
    drop_root(&dir);
    assert!(
        second.contains(r#""expected":[{"label":"B","path":"/b"}]"#),
        "the second edit was decided against stale records: {second}"
    );
}

#[gpui::test(iterations = 40)]
fn review_the_newest_history_read_wins(cx: &mut TestAppContext) {
    let dir = root("hist");
    let old = dir.join("old.xbel");
    let new = dir.join("new.xbel");
    std::fs::write(
        &old,
        r#"<xbel><bookmark href="file:///old" visited="2026-08-01T00:00:00Z"/></xbel>"#,
    )
    .unwrap();
    std::fs::write(&new, r#"<xbel><bookmark href="file:///n1" visited="2026-08-01T00:00:00Z"/><bookmark href="file:///n2" visited="2026-08-02T00:00:00Z"/></xbel>"#).unwrap();
    let places = cx.new(|_| Places::new(Arc::new(english()), PathBuf::from("/bin/false")));
    places.update(cx, |p, cx| {
        p.reread_history(old.to_string_lossy().into_owned(), cx);
        p.reread_history(new.to_string_lossy().into_owned(), cx);
    });
    cx.run_until_parked();
    let paths = places.read_with(cx, |p, _| {
        p.history.iter().map(|v| v.path.clone()).collect::<Vec<_>>()
    });
    drop_root(&dir);
    assert_eq!(
        paths,
        ["/n2", "/n1"],
        "an older read landed after the newer one"
    );
}

#[test]
fn remove_all_tags_keeps_its_x_mark() {
    use super::menus::{tags_menu, MenuRow};
    use flea::ui_logic_pl4::tags::State;
    let mut set = [State::Absent; 7];
    set[0] = State::Applied;
    let rows = tags_menu(&english(), &theme_tags(&ThemeRoles::default()), &set);
    assert!(
        matches!(rows.last(), Some(MenuRow::Item { glyph: "x", .. })),
        "v0.3.12 Tags.js:103 gives the row glyph x"
    );
}

#[test]
fn the_visual_manifest_names_boards_relative_to_the_design_root() {
    let manifest = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/visual/g2/pl4.json"),
    )
    .unwrap();
    let doc = flea::jsondoc::parse(&manifest).unwrap();
    assert!(
        !manifest.contains("/home/"),
        "no operator path in a committed manifest"
    );
    let scenes = doc.get("scenes").and_then(Json::as_array).expect("scenes");
    for scene in scenes {
        for key in ["board_png", "board_html", "before_png"] {
            let path = scene.get(key).and_then(Json::as_str).unwrap_or("");
            assert!(
                !path.starts_with('/') && !path.is_empty(),
                "{key} is design-root relative: {path:?}"
            );
        }
    }
}

#[gpui::test]
fn a_change_another_window_made_is_counted_for_the_rows(cx: &mut TestAppContext) {
    let dir = root("ext");
    let exe = dir.join("flea-fake");
    std::fs::write(&exe, "#!/bin/sh\nprintf '%s' '{\"places\":{\"favourites\":[{\"label\":\"C\",\"path\":\"/c\"}]}}'\n").unwrap();
    std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let places = cx.new(|_| Places::new(Arc::new(english()), exe.clone()));
    places.update(cx, |p, _| p.favourites.records = vec![record("A", "/a")]);
    places.update(cx, |p, cx| p.write(&Operation::Remove { index: 0 }, cx));
    cx.run_until_parked();
    let (records, changes) = places.read_with(cx, |p, _| {
        (p.favourites.records.clone(), p.external_changes)
    });
    drop_root(&dir);
    assert_eq!((records, changes), (vec![record("C", "/c")], 1));
}

#[gpui::test]
fn an_inspection_lands_its_statuses_and_a_hung_one_times_out(cx: &mut TestAppContext) {
    let dir = root("inspect");
    let exe = dir.join("flea-fake");
    std::fs::write(&exe, "#!/bin/sh\ncase \"$2\" in *hang*) exec sleep 30 ;; esac\nprintf '%s' '{\"statuses\":[{\"index\":0,\"record\":{\"label\":\"A\",\"path\":\"/a\"},\"error\":\"gone\"}]}'\n").unwrap();
    std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let places = cx.new(|_| Places::new(Arc::new(english()), exe.clone()));
    places.update(cx, |p, _| p.favourites.records = vec![record("A", "/a")]);
    places.update(cx, |p, cx| p.inspect(&[0], cx));
    cx.run_until_parked();
    let landed = places.read_with(cx, |p, _| {
        (
            p.favourites.statuses.get(&0).cloned(),
            p.favourites.inspecting,
        )
    });
    assert_eq!(landed, (Some("gone".to_owned()), false));
    let hang = dir.join("hang");
    std::fs::write(
        &hang,
        format!("#!/bin/sh\nexec '{}' \"$1\" hang\n", exe.display()),
    )
    .unwrap();
    std::fs::set_permissions(&hang, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let started = std::time::Instant::now();
    places.update(cx, |p, cx| {
        p.exe = hang.clone();
        p.inspect_wait = std::time::Duration::from_millis(200);
        p.favourites.statuses.clear();
        p.inspect(&[0], cx);
    });
    cx.run_until_parked();
    let (inspecting, said) = places.read_with(cx, |p, _| (p.favourites.inspecting, p.said.clone()));
    drop_root(&dir);
    assert!(
        !inspecting && said.is_none() && started.elapsed() < std::time::Duration::from_secs(10),
        "the timeout frees the guard and says nothing"
    );
}

#[gpui::test]
fn the_saved_state_fills_the_favourites_and_counts_a_change(cx: &mut TestAppContext) {
    let places = cx.new(|_| Places::new(Arc::new(english()), PathBuf::from("/bin/false")));
    let state = flea::jsondoc::parse(
        r#"{"places":{"showRecent":true,"favourites":[{"label":"A","path":"/a"}]}}"#,
    )
    .unwrap();
    places.update(cx, |p, cx| p.adopt_state(&state, cx));
    places.update(cx, |p, cx| p.adopt_state(&state, cx));
    let (records, changes) = places.read_with(cx, |p, _| {
        (p.favourites.records.clone(), p.external_changes)
    });
    assert_eq!(
        (records, changes),
        (vec![record("A", "/a")], 1),
        "the same state twice is one change"
    );
}
