use super::*;
use flea::ui_logic_drag_drop::{LinkKind, PasteAsk, Refusal, ROWS_MIME, URILIST};
use gpui::NativeDragAction;

pub(super) struct Pane {
    pub(super) folder: String,
    pub(super) listing: usize,
    pub(super) rows: Vec<(&'static str, bool)>,
    pub(super) selected: Vec<usize>,
}

impl Rows for Pane {
    fn folder(&self) -> &str {
        &self.folder
    }
    fn listing(&self) -> usize {
        self.listing
    }
    fn row(&self, index: usize) -> Option<(&str, bool)> {
        self.rows.get(index).copied()
    }
    fn selected(&self) -> &[usize] {
        &self.selected
    }
}

pub(super) fn pane() -> Pane {
    Pane {
        folder: "/home/u/src".into(),
        listing: 11,
        rows: vec![("alpha", true), ("b c.txt", false), ("delta", true)],
        selected: vec![0, 1],
    }
}

pub(super) fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn facts(src_dev: u64, dest_dev: u64) -> Facts {
    Facts {
        src_dev,
        dest_dev,
        deletable: true,
        refused: None,
    }
}

#[test]
fn m9_dg2_lift_carries_the_selection_with_real_paths() {
    let lift = lift(&pane(), 1, 1, (5., 6.), false, false, 3).expect("a row");
    assert_eq!(lift.rows, vec![0, 1]);
    assert_eq!(
        lift.paths,
        Some(s(&["/home/u/src/alpha", "/home/u/src/b c.txt"]))
    );
    assert_eq!((lift.side, lift.listing, lift.serial), (1, 11, 3));
    assert_eq!(lift.marker.source, "/home/u/src");
    assert_eq!(lift.marker.instance, format!("{}#3", instance()));
    let one = super::lift(&pane(), 0, 2, (0., 0.), false, false, 4).expect("unselected row");
    assert_eq!(one.rows, vec![2]);
    assert!(super::lift(&pane(), 0, 9, (0., 0.), false, false, 5).is_none());
}

#[test]
fn m9_dg2_a_row_that_does_not_resolve_offers_no_paths() {
    let mut wide = pane();
    wide.selected = vec![1, 7];
    let lift = lift(&wide, 0, 1, (0., 0.), false, false, 1).expect("pressed row exists");
    assert_eq!(lift.paths, None);
    let offered = lift.representations();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].0, ROWS_MIME);
}

#[test]
fn m9_dg2_payload_offers_marker_uris_and_the_lift_action_only() {
    let lift = lift(&pane(), 0, 1, (0., 0.), false, false, 9).expect("lift");
    let gpui::ExternalDragPayload::Mime(payload) = lift.payload() else {
        panic!("a MIME payload carries the marker");
    };
    assert_eq!(payload.source_id, 9);
    assert!(
        payload.actions.copy && !payload.actions.move_,
        "a plain lift offers copy alone"
    );
    assert_eq!(payload.actions.preferred, NativeDragAction::Copy);
    let shifted = super::lift(&pane(), 0, 1, (0., 0.), false, true, 10).expect("lift");
    let gpui::ExternalDragPayload::Mime(moving) = shifted.payload() else {
        panic!("a MIME payload");
    };
    assert!(!moving.actions.copy && moving.actions.move_);
    assert_eq!(moving.actions.preferred, NativeDragAction::Move);
    let uris = payload.data.get(URILIST).expect("uri-list");
    assert_eq!(
        &*uris,
        b"file:///home/u/src/alpha\r\nfile:///home/u/src/b%20c.txt\r\n"
    );
    assert!(payload.data.get(ROWS_MIME).is_some());
}

#[test]
fn m9_dg2_travel_below_the_slop_is_a_tap_not_a_drop() {
    let lift = lift(&pane(), 0, 1, (10., 10.), false, false, 1).expect("lift");
    assert!(!lift.travelled((17., 16.)));
    assert!(lift.travelled((20., 10.)));
}

#[test]
fn m9_dg2_spot_is_the_folder_row_under_the_pointer_else_the_pane() {
    let p = pane();
    assert_eq!(spot(&p, Some(2), None), "/home/u/src/delta");
    assert_eq!(
        spot(&p, Some(1), None),
        "/home/u/src",
        "a file row is the pane"
    );
    assert_eq!(spot(&p, None, None), "/home/u/src");
    let own = lift(&p, 0, 0, (0., 0.), false, false, 1).expect("lift");
    assert_eq!(
        spot(&p, Some(0), Some(&own)),
        "/home/u/src",
        "a carried folder never takes its own drop"
    );
    let other = Pane {
        listing: 12,
        ..pane()
    };
    assert_eq!(
        spot(&other, Some(0), Some(&own)),
        "/home/u/src/alpha",
        "another listing's row is not carried"
    );
}

#[test]
fn m9_dg2_own_lift_moves_on_one_device_and_copies_across() {
    let own = lift(&pane(), 0, 1, (0., 0.), false, false, 1).expect("lift");
    let drop = Dropping::own(&own, false, false);
    let Landing::Ask(PasteAsk::Transfer {
        op, paths, dest, ..
    }) = decide(&drop, "/home/u/dst", facts(7, 7))
    else {
        panic!("lands");
    };
    assert_eq!((op.as_str(), dest.as_str()), ("move", "/home/u/dst"));
    assert_eq!(paths, s(&["/home/u/src/alpha", "/home/u/src/b c.txt"]));
    assert!(
        matches!(decide(&drop, "/mnt/usb", facts(7, 8)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "copy")
    );
    let ctrl = Dropping::own(&own, true, false);
    assert!(
        matches!(decide(&ctrl, "/home/u/dst", facts(7, 7)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "copy")
    );
    let shift = Dropping::own(&own, false, true);
    assert!(
        matches!(decide(&shift, "/mnt/usb", facts(7, 8)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "move")
    );
    let both = Dropping::own(&own, true, true);
    assert!(matches!(
        decide(&both, "/home/u/dst", facts(7, 7)),
        Landing::Ask(PasteAsk::Link {
            kind: LinkKind::Relative,
            ..
        })
    ));
    let locked = Facts {
        deletable: false,
        ..facts(7, 7)
    };
    assert!(
        matches!(decide(&drop, "/home/u/dst", locked), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "copy")
    );
}

#[test]
fn m9_dg2_own_lift_refuses_its_own_folder_and_itself() {
    let own = lift(&pane(), 0, 0, (0., 0.), false, false, 1).expect("lift");
    let drop = Dropping::own(&own, false, false);
    assert_eq!(
        decide(&drop, "/home/u/src", facts(7, 7)),
        Landing::Refused(Refusal::AlreadyHere)
    );
    assert_eq!(
        decide(&drop, "/home/u/src/alpha/in", facts(7, 7)),
        Landing::Refused(Refusal::InsideDrag)
    );
    assert_eq!(decide(&drop, "", facts(7, 7)), Landing::Silent);
}

#[test]
fn m9_dg2_wide_own_lift_lands_nowhere_without_paths() {
    let mut wide = pane();
    wide.selected = vec![1, 7];
    let own = lift(&wide, 0, 1, (0., 0.), false, false, 1).expect("lift");
    assert_eq!(
        decide(
            &Dropping::own(&own, false, false),
            "/elsewhere",
            facts(7, 7)
        ),
        Landing::Silent
    );
}

#[test]
fn m9_dg2_foreign_drop_copies_unless_the_platform_moves() {
    let urls = s(&["file:///tmp/x/a.png"]);
    let mut drop = Dropping::native(None, &urls, None, false, false, None);
    assert!(
        matches!(decide(&drop, "/home/u", facts(0, 7)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "copy")
    );
    drop.proposed.move_ = true;
    assert!(
        matches!(decide(&drop, "/home/u", facts(0, 7)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "move")
    );
    let none: Vec<String> = Vec::new();
    let empty = Dropping::native(None, &none, Some("not a path"), false, false, None);
    assert_eq!(
        decide(&empty, "/home/u", facts(0, 7)),
        Landing::Refused(Refusal::NoLocalFiles)
    );
}

#[test]
fn m9_dg2_another_flea_window_reads_the_marker_and_stat_facts() {
    let own = lift(&pane(), 0, 1, (0., 0.), false, false, 1).expect("lift");
    let urls: Vec<String> = own
        .paths
        .clone()
        .unwrap()
        .iter()
        .map(|p| flea::ui_logic_drag_drop::uri_for(p))
        .collect();
    let marker = own.marker.encode();
    let drop = Dropping::native(Some(&marker), &urls, None, false, false, None);
    assert!(
        matches!(decide(&drop, "/home/u/dst", facts(7, 7)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "move")
    );
    let copied = super::lift(&pane(), 0, 1, (0., 0.), true, false, 2).expect("ctrl lift");
    let copy_marker = copied.marker.encode();
    let unseen = Dropping::native(Some(&copy_marker), &urls, None, false, false, None);
    assert!(
        matches!(decide(&unseen, "/home/u/dst", facts(7, 7)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "copy"),
        "a key this window never saw keeps the lift's copy bit"
    );
    let bad = Dropping::native(Some("garbage"), &urls, None, false, false, None);
    assert_eq!(decide(&bad, "/home/u/dst", facts(7, 7)), Landing::Silent);
}

#[test]
fn native_current_key_pair_replaces_the_other_windows_lift_keys() {
    let cases = [
        (
            false,
            true,
            true,
            false,
            Some(NativeDragAction::Move),
            "copy",
        ),
        (
            true,
            false,
            false,
            true,
            Some(NativeDragAction::Copy),
            "move",
        ),
        (
            false,
            true,
            true,
            true,
            Some(NativeDragAction::Move),
            "link",
        ),
        (
            true,
            false,
            true,
            true,
            Some(NativeDragAction::Copy),
            "link",
        ),
        (true, false, false, false, None, "copy"),
        (false, true, false, false, None, "move"),
    ];
    for (lift_ctrl, lift_shift, ctrl, shift, action, expected) in cases {
        let lift = lift(&pane(), 0, 1, (0., 0.), lift_ctrl, lift_shift, 1).unwrap();
        let marker = lift.marker.encode();
        let urls: Vec<String> = lift
            .paths
            .unwrap()
            .iter()
            .map(|path| core::uri_for(path))
            .collect();
        let drop = Dropping::native(Some(&marker), &urls, None, ctrl, shift, action);
        let landing = decide(&drop, "/mnt/usb", facts(7, 8));
        let actual = match &landing {
            Landing::Ask(PasteAsk::Link { .. }) => "link",
            Landing::Ask(PasteAsk::Transfer { op, .. }) => op.as_str(),
            _ => panic!("unexpected landing: {landing:?}"),
        };
        assert_eq!(
            actual, expected,
            "lift=({lift_ctrl},{lift_shift}) current=({ctrl},{shift}) action={action:?}"
        );
    }
}

#[test]
fn m9_dg2_facts_read_devices_and_writability_from_disk() {
    let template = std::env::temp_dir().join("flea-dg2-facts.XXXXXXXX");
    let made = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(&template)
        .output()
        .expect("mktemp runs");
    assert!(made.status.success(), "mktemp -d failed");
    let root = std::path::PathBuf::from(
        String::from_utf8(made.stdout)
            .expect("utf-8 path")
            .trim_end(),
    );
    assert!(
        root.is_absolute()
            && root.starts_with(std::env::temp_dir())
            && root != std::env::temp_dir()
    );
    std::fs::write(root.join(".flea-test-root"), b"").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a"), b"x").unwrap();
    let read = facts::read(
        &[root.join("src/a").to_string_lossy().into_owned()],
        &root.to_string_lossy(),
    );
    assert!(read.src_dev != 0 && read.src_dev == read.dest_dev && read.deletable);
    let missing = facts::read(
        &["/nonexistent/flea-dg2/a".to_owned()],
        "/nonexistent/flea-dg2",
    );
    assert_eq!(
        (missing.src_dev, missing.dest_dev, missing.deletable),
        (0, 0, false)
    );
    if root.join(".flea-test-root").is_file()
        && root.starts_with(std::env::temp_dir())
        && root != std::env::temp_dir()
    {
        std::fs::remove_dir_all(&root).unwrap();
    }
}
