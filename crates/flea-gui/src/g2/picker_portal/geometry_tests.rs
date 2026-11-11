// The real endpoint must read the controls and rows the picker actually drew.
use super::*;
use crate::ipc::server::Event;
use flea::ipc::wire::{Reply, Request};

fn endpoint(f: &mut Fixture, function: &str, arguments: &[&str]) -> Reply {
    let (send, receive) = async_channel::unbounded();
    f.update(|picker, cx| {
        let task = launch::serve_ipc(receive, (), cx);
        picker.keep(task);
    });
    let (answer, answered) = std::sync::mpsc::sync_channel(1);
    send.send_blocking(Event::Call {
        request: Request::Call {
            target: "fleapicker".into(),
            function: function.into(),
            arguments: arguments.iter().map(|text| text.to_string()).collect(),
        },
        answer,
    })
    .unwrap();
    f.cx.run_until_parked();
    answered.try_recv().expect("the endpoint answered").reply
}

fn seeded(f: &mut Fixture) {
    f.listing.say(Line::Reply(
        r#"{"t":"listed","n":1,"v":1,"w":true,"path":"/d"}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"notes.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"}]}"#.into()));
    f.cx.run_until_parked();
}

#[gpui::test]
fn followup_picker_controls_use_the_live_endpoint(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seeded(&mut f);
    let reply = endpoint(&mut f, "controls", &[]);
    assert_eq!(reply.code, 0, "{reply:?}");
    let rows = flea::jsondoc::parse(&reply.stdout).unwrap();
    let controls = rows.as_array().unwrap();
    assert!(controls.len() >= 2, "answers and the drawn header titles");
    assert!(controls.iter().all(|control| !control
        .get("centre")
        .unwrap()
        .as_str()
        .unwrap()
        .is_empty()));
}

#[gpui::test]
fn followup_picker_row_centre_reads_drawn_rows_only(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    f.cx.simulate_resize(gpui::size(gpui::px(720.), gpui::px(440.)));
    let rows = (0..80)
        .map(|index| {
            format!(
                r#"{{"n":"note-{index}.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-x-generic"}}"#
            )
        })
        .collect::<Vec<_>>();
    f.listing.say(Line::Reply(
        r#"{"t":"listed","n":80,"v":1,"w":true,"path":"/d"}"#.into(),
    ));
    f.listing.say(Line::Reply(format!(
        r#"{{"t":"rows","start":0,"listing":1,"rows":[{}]}}"#,
        rows.join(",")
    )));
    f.cx.run_until_parked();
    let reply = endpoint(&mut f, "rowCentre", &["0"]);
    assert_eq!(reply.code, 0, "{reply:?}");
    assert_eq!(reply.stdout.split_whitespace().count(), 2);
    assert!(
        endpoint(&mut f, "rowCentre", &["79"])
            .stdout
            .trim()
            .is_empty(),
        "held but off-screen is not drawn"
    );
    f.update(|picker, cx| picker.move_cursor(79, cx));
    assert_eq!(
        endpoint(&mut f, "rowCentre", &["79"])
            .stdout
            .split_whitespace()
            .count(),
        2
    );
    assert!(endpoint(&mut f, "rowCentre", &["0"])
        .stdout
        .trim()
        .is_empty());
    assert!(endpoint(&mut f, "rowCentre", &["80"])
        .stdout
        .trim()
        .is_empty());
}

#[gpui::test]
fn followup_picker_save_field_uses_its_painted_box(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"notes.txt"}"#,
        None,
    );
    f.cx.run_until_parked();
    let reply = endpoint(&mut f, "saveState", &[]);
    assert_eq!(reply.code, 0, "{reply:?}");
    let state = flea::jsondoc::parse(&reply.stdout).unwrap();
    let field = state.get("field").unwrap().as_str().unwrap();
    assert_eq!(field.split_whitespace().count(), 2);
    assert_eq!(state.get("name").unwrap().as_str(), Some("notes.txt"));
}

#[gpui::test]
fn followup_picker_snapshot_matches_the_same_live_frame(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seeded(&mut f);
    let reply = endpoint(&mut f, "snapshot", &[]);
    assert_eq!(reply.code, 0, "{reply:?}");
    let state = flea::jsondoc::parse(&reply.stdout).unwrap();
    assert_eq!(state.get("view").unwrap().as_str(), Some("list"));
    assert_eq!(state.get("cursorName").unwrap().as_str(), Some("notes.txt"));
    assert!(state.get("width").unwrap().as_f64().unwrap() > 0.);
    assert!(state.get("controls").unwrap().as_array().unwrap().len() >= 2);
}

#[gpui::test]
fn followup_picker_queued_geometry_calls_all_answer_after_resize(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    let (send, receive) = async_channel::unbounded();
    f.update(|picker, cx| {
        let task = launch::serve_ipc(receive, (), cx);
        picker.keep(task);
    });
    f.cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(600.)));
    let mut answers = Vec::new();
    for function in ["controls", "cohesionControls", "saveState", "snapshot"] {
        let (answer, reply) = std::sync::mpsc::sync_channel(1);
        send.send_blocking(Event::Call {
            request: Request::Call {
                target: "fleapicker".into(),
                function: function.into(),
                arguments: vec![],
            },
            answer,
        })
        .unwrap();
        answers.push(reply);
    }
    f.cx.run_until_parked();
    for answer in answers {
        let reply = answer
            .try_recv()
            .expect("every queued reader is answered")
            .reply;
        assert_eq!(reply.code, 0, "{reply:?}");
    }
}

#[gpui::test]
fn trash_font_picker_listing_follows_the_live_family(cx: &mut TestAppContext) {
    let f = fixture(cx, r#"{"folder":"/d"}"#, None);
    for family in ["DejaVu Sans Mono", "DejaVu Sans"] {
        f.cx.update(|window, cx| {
            let fonts = crate::presentation::Fonts::from_sources("", "", family.into());
            cx.set_global(fonts.typeface(&flea::jsondoc::Json::Null));
            cx.set_global(fonts);
            window.refresh();
        });
        f.cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        f.cx.update(|_, cx| {
            let picker = f.picker.read(cx);
            assert_eq!(
                picker
                    .browse
                    .listing
                    .read(cx)
                    .drawn()
                    .typeface
                    .font
                    .family
                    .as_ref(),
                family
            );
        });
    }
}

#[gpui::test]
fn picker_gm_path_bar_open_reads_the_drawn_entry(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    super::super::pointer_tests::seed(&mut f);
    let open = |f: &mut Fixture| {
        let call = endpoint(f, "pathBarOpen", &[]).stdout.trim().to_owned();
        let snapshot = flea::jsondoc::parse(&endpoint(f, "snapshot", &[]).stdout).unwrap();
        let member = snapshot
            .get("pathBarOpen")
            .and_then(|value| value.as_bool());
        (call, member)
    };
    assert_eq!(open(&mut f), ("false".into(), Some(false)));
    super::super::pointer_tests::path_click(&mut f, false);
    assert_eq!(
        open(&mut f),
        ("true".into(), Some(true)),
        "the opened entry answers open"
    );
    f.cx.simulate_keystrokes("escape");
    f.cx.run_until_parked();
    assert_eq!(
        open(&mut f),
        ("false".into(), Some(false)),
        "Escape closes it again"
    );
}

// EY-329: the visual lane clicks Recent by its `controls` centre, as PickerPlaces.qml controls() lists every rail row.
#[gpui::test]
fn ey329_controls_list_the_rail_rows_and_recent_opens_from_its_centre(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seeded(&mut f);
    let reply = endpoint(&mut f, "controls", &[]);
    let controls = flea::jsondoc::parse(&reply.stdout).unwrap();
    let recent = controls
        .as_array()
        .unwrap()
        .iter()
        .find(|control| control.get("name").and_then(|name| name.as_str()) == Some("Recent"))
        .unwrap_or_else(|| panic!("Recent is among the controls: {}", reply.stdout))
        .clone();
    assert_eq!(
        recent.get("enabled").and_then(|on| on.as_bool()),
        Some(true)
    );
    let centre: Vec<f32> = recent
        .get("centre")
        .and_then(|centre| centre.as_str())
        .unwrap()
        .split_whitespace()
        .map(|part| part.parse().unwrap())
        .collect();
    assert_eq!(centre.len(), 2, "Recent has a drawn centre: {recent:?}");
    f.cx.simulate_click(
        gpui::point(gpui::px(centre[0]), gpui::px(centre[1])),
        gpui::Modifiers::none(),
    );
    f.cx.run_until_parked();
    assert_eq!(
        endpoint(&mut f, "recent", &[]).stdout.trim(),
        "true",
        "the press at Recent's centre opens Recent"
    );
}

// EY-329: the lane presses ":" and types "Doc"; the entry opens on the drawn folder and takes the letters.
#[gpui::test]
fn ey329_colon_opens_the_path_entry_and_typing_lands_in_it(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seeded(&mut f);
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.run_until_parked();
    f.cx.simulate_input(":");
    f.cx.run_until_parked();
    assert_eq!(endpoint(&mut f, "pathBarOpen", &[]).stdout.trim(), "true");
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    let entry = |f: &mut Fixture| {
        f.cx.update(|_, cx| {
            let entry = &path.read(cx).entry;
            (entry.text.clone(), entry.selected.clone())
        })
    };
    assert_eq!(
        entry(&mut f),
        ("/d/".to_owned(), 0..3),
        "the entry opens on the drawn folder, all of it selected, and the colon stays out"
    );
    f.cx.simulate_input("Doc");
    f.cx.run_until_parked();
    assert_eq!(
        entry(&mut f),
        ("Doc".to_owned(), 3..3),
        "the typed letters replace the selected folder"
    );
}

#[path = "ey329_rail_tests.rs"]
mod ey329_rail_tests;

#[gpui::test]
fn vf_readers_picker_recent_has_painted_control(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seeded(&mut f);
    f.update(|picker, cx| picker.start_places(&flea::jsondoc::Json::Obj(vec![]), cx));
    f.cx.run_until_parked();
    let reply = endpoint(&mut f, "controls", &[]);
    let controls = flea::jsondoc::parse(&reply.stdout).unwrap();
    let recent: Vec<_> = controls
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c.get("name").and_then(flea::jsondoc::Json::as_str) == Some("Recent"))
        .collect();
    assert_eq!(
        recent.len(),
        1,
        "the painted row has exactly one control: {controls:?}"
    );
    let recent = recent[0];
    assert!(!recent
        .get("centre")
        .and_then(flea::jsondoc::Json::as_str)
        .unwrap()
        .is_empty());
    assert_eq!(
        recent.get("enabled").and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    f.update(|picker, cx| picker.accept(cx));
    f.cx.run_until_parked();
    assert!(f.read(|picker| picker.accept_state().submitting()));
    let reply = endpoint(&mut f, "controls", &[]);
    let controls = flea::jsondoc::parse(&reply.stdout).unwrap();
    let recent: Vec<_> = controls
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c.get("name").and_then(flea::jsondoc::Json::as_str) == Some("Recent"))
        .collect();
    assert_eq!(recent.len(), 1);
    assert_eq!(
        recent[0]
            .get("enabled")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
}

#[gpui::test]
fn vf_readers_picker_path_bar_open_follows_the_entry(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    seeded(&mut f);
    let focus = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&focus, cx));
    f.cx.simulate_keystrokes("ctrl-l");
    assert_eq!(endpoint(&mut f, "pathBarOpen", &[]), Reply::value("true"));
    f.cx.simulate_keystrokes("escape");
    assert_eq!(endpoint(&mut f, "pathBarOpen", &[]), Reply::value("false"));
}

#[path = "fixreaders_geometry_tests.rs"]
mod fixreaders;

#[path = "geometry_cohesion_tests.rs"]
mod cohesion;
