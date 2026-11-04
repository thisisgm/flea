// The fleapicker target: every function answers from the window's own state or refuses as not ported.
use super::*;
use flea::ipc::wire::{Reply, Request};

struct Drawn;

impl ipc::PickerGeometry for Drawn {
    fn controls(&self) -> String {
        "[]".into()
    }
    fn row_centre(&self, index: usize) -> String {
        format!("10 {}", 20 + index)
    }
    fn cohesion_controls(&self) -> String {
        "[]".into()
    }
    fn save_field_centre(&self) -> String {
        "5 6".into()
    }
    fn path_bar_open(&self) -> bool {
        true
    }
    fn snapshot_members(&self) -> String {
        r#""view":"list""#.into()
    }
}

fn call(
    f: &mut Fixture,
    function: &str,
    arguments: &[&str],
    geometry: Option<&dyn ipc::PickerGeometry>,
) -> Reply {
    let request = Request::Call {
        target: "fleapicker".into(),
        function: function.into(),
        arguments: arguments
            .iter()
            .map(|argument| argument.to_string())
            .collect(),
    };
    f.read(|picker| picker.ipc(request, geometry).reply)
}

fn value(f: &mut Fixture, function: &str, arguments: &[&str]) -> String {
    let reply = call(f, function, arguments, Some(&Drawn));
    assert_eq!(reply.code, 0, "{function}: {reply:?}");
    reply
        .stdout
        .strip_suffix('\n')
        .unwrap_or(&reply.stdout)
        .to_owned()
}

#[gpui::test]
fn every_reader_answers_from_the_windows_state(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"multiple":true,"accept":"Send","filters":[{"label":"A"},{"label":"B"}],"current":"B"}"#,
        None,
    );
    f.update(|picker, cx| {
        picker.listing = ListingFacts {
            location: "/d".into(),
            total: 40,
            shown_total: 12,
            held: 10,
            names: vec!["a.txt".into(), "b \"q\".txt".into()],
            cursor: 11,
            state: "ready",
            save_name: "n".into(),
            ..picker.listing.clone()
        };
        picker.marks = logic::Mark::reviewed(&[], &[("/d/a #.txt".into(), 4), ("/d/c".into(), 5)]);
        picker.say("note", false, cx);
    });
    for (function, arguments, expected) in [
        ("ready", &[][..], "true"),
        ("path", &[], "/d"),
        ("total", &[], "40"),
        ("shownTotal", &[], "12"),
        ("cursor", &[], "11"),
        ("marks", &[], "/d/a #.txt,/d/c"),
        ("rowAt", &["10"], "a.txt"),
        ("rowAt", &["9"], ""),
        ("rowAt", &["-1"], ""),
        ("cursorName", &[], "b \"q\".txt"),
        ("state", &[], "ready"),
        ("recent", &[], "false"),
        ("accept", &[], "Send 2"),
        ("chip", &[], "1"),
        ("saveName", &[], "n"),
        ("message", &[], "note"),
        ("pathBarOpen", &[], "true"),
        (
            "markedUris",
            &[],
            r#"["file:///d/a%20%23.txt","file:///d/c"]"#,
        ),
        (
            "checks",
            &[],
            r#"{"marksBusy":false,"saveBusy":false,"submitting":false,"canAccept":true,"saveReady":false,"collision":false,"review":0}"#,
        ),
        ("rowCentre", &["3"], "10 23"),
        ("controls", &[], "[]"),
        ("cohesionControls", &[], "[]"),
        (
            "saveState",
            &[],
            r#"{"name":"n","path":"","collision":false,"error":"","field":"5 6"}"#,
        ),
        ("version", &[], env!("CARGO_PKG_VERSION")),
    ] {
        assert_eq!(value(&mut f, function, arguments), expected, "{function}");
    }
    let snapshot = flea::jsondoc::parse(&value(&mut f, "snapshot", &[]))
        .expect("the snapshot is one JSON document");
    for (key, expected) in [
        ("path", "\"/d\""),
        ("cursorName", "\"b \\\"q\\\".txt\""),
        ("filter", "1"),
        ("canAccept", "true"),
        ("view", "\"list\""),
    ] {
        assert_eq!(
            flea::jsondoc::render_at(snapshot.get(key).unwrap_or_else(|| panic!("{key}")), 0),
            expected,
            "{key}"
        );
    }
    assert_eq!(
        snapshot
            .get("rows")
            .and_then(flea::jsondoc::Json::as_array)
            .map(<[_]>::len),
        Some(2)
    );
}

#[gpui::test]
fn what_only_drawn_geometry_knows_is_refused_until_its_owner_joins(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    for (function, arguments) in [
        ("controls", &[][..]),
        ("rowCentre", &["0"]),
        ("cohesionControls", &[]),
        ("saveState", &[]),
        ("snapshot", &[]),
    ] {
        let reply = call(&mut f, function, arguments, None);
        assert_eq!(
            reply,
            Reply::error(3, format!("FLEA_IPC_NOT_PORTED fleapicker.{function}"))
        );
    }
    assert_eq!(call(&mut f, "path", &[], None), Reply::value("/home/test"));
}

#[gpui::test]
fn malformed_calls_and_other_targets_are_refused(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    assert_eq!(
        call(&mut f, "rowAt", &["x"], None).code,
        0,
        "the registry's own parse refusal is a value, as Quickshell prints it"
    );
    assert!(call(&mut f, "rowAt", &["x"], None)
        .stdout
        .starts_with("Unable to parse argument 1"));
    assert!(call(&mut f, "rowAt", &[], None)
        .stdout
        .starts_with("Too few arguments"));
    assert_eq!(
        call(&mut f, "nope", &[], None),
        Reply::value("Function not found.")
    );
    // Only fleapicker is loaded, so main-window calls receive Quickshell's no-target line.
    for function in ["path", "cursor", "total", "viewMode", "focusView", "nope"] {
        let other = Request::Call {
            target: "flea".into(),
            function: function.into(),
            arguments: vec![],
        };
        assert_eq!(
            f.read(|picker| picker.ipc(other, Some(&Drawn)).reply),
            Reply::value("Target not found."),
            "flea.{function}"
        );
    }
    for function in ["path", "cursor", "total"] {
        assert_eq!(
            call(&mut f, function, &[], Some(&Drawn)).code,
            0,
            "fleapicker.{function}"
        );
    }
    let tab = Request::Call {
        target: "fleatab".into(),
        function: "taken".into(),
        arguments: vec!["1".into()],
    };
    assert_eq!(
        f.read(|picker| picker.ipc(tab, None).reply),
        Reply::error(3, "FLEA_IPC_NOT_PORTED fleatab.taken")
    );
    let quit = Request::Call {
        target: "fleapicker".into(),
        function: "quit".into(),
        arguments: vec![],
    };
    assert!(
        f.read(|picker| picker.ipc(quit, None).quit),
        "quit is the host's close, which answers first"
    );
}

// The endpoint's events reach the window: a call answers from its state, and a quit refuses like a close.
#[gpui::test]
fn the_endpoint_serves_calls_and_a_quit_refuses(cx: &mut TestAppContext) {
    use crate::ipc::server::Event;
    let mut f = fixture(cx, "{}", None);
    let (events, received) = async_channel::unbounded();
    f.update(|picker, cx| {
        let served = launch::serve_ipc(received, (), cx);
        picker.keep(served);
    });
    let (answer, answered) = std::sync::mpsc::sync_channel(1);
    let request = Request::Call {
        target: "fleapicker".into(),
        function: "ready".into(),
        arguments: Vec::new(),
    };
    events
        .send_blocking(Event::Call { request, answer })
        .unwrap();
    f.cx.run_until_parked();
    let reply = answered.try_recv().expect("the call was answered").reply;
    assert_eq!((reply.code, reply.stdout.trim_end()), (0, "true"));
    events.send_blocking(Event::Quit).unwrap();
    f.settle(|f| f.quits() == (1, 1));
    assert_eq!(f.written().as_deref(), Some(r#"{"response":1}"#));
}
