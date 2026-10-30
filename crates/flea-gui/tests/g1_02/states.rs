use super::*;
use flea::ipc::wire;
use flea_gui::{
    ipc::{pane::PaneAdapter, Surface},
    pane::states::{hero_span, State},
};

#[test]
fn locked_and_error_use_board_copy_and_only_the_proven_mode() {
    let catalog = crate::test_catalogs::english();
    for (mode, detail) in [
        (0, ""),
        (0o40000, "---------"),
        (0o40700, "rwx------ · not yours"),
    ] {
        let mut pane = settled();
        pane.begin("/fixture/Locked".into(), Options::default(), 1000);
        assert!(pane.fail(
            "scan",
            "/fixture/Locked",
            "Permission denied (os error 13)",
            mode,
            1040
        ));
        assert_eq!(pane.state.kind, Kind::Locked);
        assert_eq!(pane.state.locked_target(), Some("/fixture/Locked"));
        let text = pane.state.text(&catalog, 0);
        assert_eq!(
            text.message,
            "You don’t have permission to open this folder."
        );
        assert_eq!(text.detail, detail);
        assert_eq!(text.status, "unavailable");
        assert_eq!(pane.held.total, 0);
        assert!(pane.row_action(vec![0]).is_none());
        assert!(pane.begin("/fixture/next".into(), Options::default(), 2000));
        assert!(pane.swap.holding);
    }
    let state = State {
        kind: Kind::Error,
        ..State::default()
    };
    let text = state.text(&catalog, 0);
    assert_eq!(text.message, "That folder could not be read.");
    assert_eq!(state.locked_target(), None);
    assert!(text.detail.is_empty());
}

#[test]
fn empty_words_loop_three_cycles_without_advancing_hidden_or_reduced_states() {
    let catalog = crate::test_catalogs::english();
    let mut state = State {
        kind: Kind::Empty,
        ..State::default()
    };
    let phrases = [
        "NOTHING HERE YET",
        "A VERY TIDY FOLDER",
        "NOT A FILE IN SIGHT",
        "QUIET IN HERE",
    ];
    for _ in 0..3 {
        for expected in phrases {
            assert_eq!(state.text(&catalog, 0).message, expected);
            assert_eq!(state.text(&catalog, 0).status, "0 items");
            assert!(state.empty_beat(false, true));
        }
    }
    state.beat = 0;
    assert!(!state.empty_beat(true, true));
    assert_eq!(state.beat, 0);
    assert!(!state.empty_beat(false, false));
    assert_eq!(state.beat, 0);
    state.kind = Kind::Ready;
    assert!(!state.empty_beat(false, true));
    assert_eq!(state.beat, 0);
}

#[test]
fn all_states_have_one_semantic_hero_and_no_false_permission_detail() {
    let catalog = crate::test_catalogs::english();
    for kind in [
        Kind::Loading,
        Kind::Empty,
        Kind::Ready,
        Kind::Error,
        Kind::Locked,
        Kind::Waiting,
    ] {
        let state = State {
            kind,
            ..State::default()
        };
        let text = state.text(&catalog, 0);
        assert_eq!(text.message.is_empty(), kind == Kind::Ready);
        assert!(text.detail.is_empty());
        assert!(!text.message.contains("pane."));
    }
}

#[test]
fn columns_heroes_clear_the_active_track_using_the_producers_natural_width() {
    for count in [2, 3, 4] {
        let width = 1880.;
        let active = (count - 2) as f32 * (width / count as f32).floor();
        for natural in [48., 240., 400.] {
            let (start, span) = hero_span(width, active, natural);
            let middle = start + span / 2.;
            assert!(middle - natural / 2. >= active);
            assert_eq!(start + span, width);
        }
    }
}

#[test]
fn adapters_report_real_snapshot_counters_and_refuse_unowned_functions() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    let catalog = crate::test_catalogs::english();
    let adapter = PaneAdapter {
        pane: &pane,
        catalog: &catalog,
    };
    let surface = Surface {
        path: pane.path(),
        total: pane.held.total,
        cursor: pane.cursor,
        selected: &[],
    };
    for (name, expected) in [
        ("listInFlight", "true"),
        ("listRequests", "2"),
        ("inputToRows", "1000 0"),
        ("state", "ready"),
    ] {
        let answer = surface.dispatch_to(
            wire::Request::Call {
                target: "flea".into(),
                function: name.into(),
                arguments: vec![],
            },
            &[&adapter],
        );
        assert_eq!(answer.reply, wire::Reply::value(expected));
    }
    for name in ["listingWindowState", "swapState"] {
        let answer = surface.dispatch_to(
            wire::Request::Call {
                target: "flea".into(),
                function: name.into(),
                arguments: vec![],
            },
            &[&adapter],
        );
        let data = flea::jsondoc::parse(answer.reply.stdout.trim()).unwrap();
        assert!(data
            .get(if name == "swapState" {
                "holds"
            } else {
                "loaded"
            })
            .is_some());
    }
    let answer = surface.dispatch_to(
        wire::Request::Call {
            target: "flea".into(),
            function: "previewState".into(),
            arguments: vec![],
        },
        &[&adapter],
    );
    assert_ne!(answer.reply.code, 0);
}
