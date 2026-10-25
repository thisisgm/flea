use super::router::{Client, Native, Offered, Router};
use super::*;
use flea::ui_logic_dg2::{LinkKind, Marker, PasteAsk, Refusal, ROWS_MIME, URILIST};
use gpui::{NativeDragAction, NativeDragActions};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

struct Pane {
    folder: String,
    listing: usize,
    rows: Vec<(&'static str, bool)>,
    selected: Vec<usize>,
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

fn pane() -> Pane {
    Pane {
        folder: "/home/u/src".into(),
        listing: 11,
        rows: vec![("alpha", true), ("b c.txt", false), ("delta", true)],
        selected: vec![0, 1],
    }
}

fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn facts(src_dev: u64, dest_dev: u64) -> Facts {
    Facts { src_dev, dest_dev, deletable: true }
}

#[test]
fn m9_dg2_lift_carries_the_selection_with_real_paths() {
    let lift = lift(&pane(), 1, 1, (5., 6.), false, false, 3).expect("a row");
    assert_eq!(lift.rows, vec![0, 1]);
    assert_eq!(lift.paths, Some(s(&["/home/u/src/alpha", "/home/u/src/b c.txt"])));
    assert_eq!((lift.side, lift.listing, lift.serial), (1, 11, 3));
    assert_eq!(lift.marker.source, "/home/u/src");
    assert_eq!(lift.marker.instance, instance());
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
    assert!(payload.actions.copy && !payload.actions.move_, "a plain lift offers copy alone");
    assert_eq!(payload.actions.preferred, NativeDragAction::Copy);
    let shifted = super::lift(&pane(), 0, 1, (0., 0.), false, true, 10).expect("lift");
    let gpui::ExternalDragPayload::Mime(moving) = shifted.payload() else {
        panic!("a MIME payload");
    };
    assert!(!moving.actions.copy && moving.actions.move_);
    assert_eq!(moving.actions.preferred, NativeDragAction::Move);
    let uris = payload.data.get(URILIST).expect("uri-list");
    assert_eq!(&*uris, b"file:///home/u/src/alpha\r\nfile:///home/u/src/b%20c.txt\r\n");
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
    assert_eq!(spot(&p, Some(1), None), "/home/u/src", "a file row is the pane");
    assert_eq!(spot(&p, None, None), "/home/u/src");
    let own = lift(&p, 0, 0, (0., 0.), false, false, 1).expect("lift");
    assert_eq!(spot(&p, Some(0), Some(&own)), "/home/u/src", "a carried folder never takes its own drop");
    let other = Pane { listing: 12, ..pane() };
    assert_eq!(spot(&other, Some(0), Some(&own)), "/home/u/src/alpha", "another listing's row is not carried");
}

#[test]
fn m9_dg2_own_lift_moves_on_one_device_and_copies_across() {
    let own = lift(&pane(), 0, 1, (0., 0.), false, false, 1).expect("lift");
    let drop = Dropping::own(&own, false, false);
    let Landing::Ask(PasteAsk::Transfer { op, paths, dest, .. }) = decide(&drop, "/home/u/dst", facts(7, 7)) else {
        panic!("lands");
    };
    assert_eq!((op.as_str(), dest.as_str()), ("move", "/home/u/dst"));
    assert_eq!(paths, s(&["/home/u/src/alpha", "/home/u/src/b c.txt"]));
    assert!(matches!(decide(&drop, "/mnt/usb", facts(7, 8)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "copy"));
    let ctrl = Dropping::own(&own, true, false);
    assert!(matches!(decide(&ctrl, "/home/u/dst", facts(7, 7)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "copy"));
    let shift = Dropping::own(&own, false, true);
    assert!(matches!(decide(&shift, "/mnt/usb", facts(7, 8)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "move"));
    let both = Dropping::own(&own, true, true);
    assert!(matches!(decide(&both, "/home/u/dst", facts(7, 7)), Landing::Ask(PasteAsk::Link { kind: LinkKind::Relative, .. })));
    let locked = Facts { deletable: false, ..facts(7, 7) };
    assert!(matches!(decide(&drop, "/home/u/dst", locked), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "copy"));
}

#[test]
fn m9_dg2_own_lift_refuses_its_own_folder_and_itself() {
    let own = lift(&pane(), 0, 0, (0., 0.), false, false, 1).expect("lift");
    let drop = Dropping::own(&own, false, false);
    assert_eq!(decide(&drop, "/home/u/src", facts(7, 7)), Landing::Refused(Refusal::AlreadyHere));
    assert_eq!(decide(&drop, "/home/u/src/alpha/in", facts(7, 7)), Landing::Refused(Refusal::InsideDrag));
    assert_eq!(decide(&drop, "", facts(7, 7)), Landing::Silent);
}

#[test]
fn m9_dg2_wide_own_lift_lands_nowhere_without_paths() {
    let mut wide = pane();
    wide.selected = vec![1, 7];
    let own = lift(&wide, 0, 1, (0., 0.), false, false, 1).expect("lift");
    assert_eq!(decide(&Dropping::own(&own, false, false), "/elsewhere", facts(7, 7)), Landing::Silent);
}

#[test]
fn m9_dg2_foreign_drop_copies_unless_the_platform_moves() {
    let urls = s(&["file:///tmp/x/a.png"]);
    let mut drop = Dropping::native(None, &urls, None, false, false, None);
    assert!(matches!(decide(&drop, "/home/u", facts(0, 7)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "copy"));
    drop.proposed.move_ = true;
    assert!(matches!(decide(&drop, "/home/u", facts(0, 7)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "move"));
    let none: Vec<String> = Vec::new();
    let empty = Dropping::native(None, &none, Some("not a path"), false, false, None);
    assert_eq!(decide(&empty, "/home/u", facts(0, 7)), Landing::Refused(Refusal::NoLocalFiles));
}

#[test]
fn m9_dg2_another_flea_window_reads_the_marker_and_stat_facts() {
    let own = lift(&pane(), 0, 1, (0., 0.), false, false, 1).expect("lift");
    let urls: Vec<String> = own.paths.clone().unwrap().iter().map(|p| flea::ui_logic_dg2::uri_for(p)).collect();
    let marker = own.marker.encode();
    let drop = Dropping::native(Some(&marker), &urls, None, false, false, None);
    assert!(matches!(decide(&drop, "/home/u/dst", facts(7, 7)), Landing::Ask(PasteAsk::Transfer { ref op, .. }) if op == "move"));
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
fn m9_dg2_facts_read_devices_and_writability_from_disk() {
    let template = std::env::temp_dir().join("flea-dg2-facts.XXXXXXXX");
    let made = std::process::Command::new("mktemp").arg("-d").arg(&template).output().expect("mktemp runs");
    assert!(made.status.success(), "mktemp -d failed");
    let root = std::path::PathBuf::from(String::from_utf8(made.stdout).expect("utf-8 path").trim_end());
    assert!(root.is_absolute() && root.starts_with(std::env::temp_dir()) && root != std::env::temp_dir());
    std::fs::write(root.join(".flea-test-root"), b"").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a"), b"x").unwrap();
    let read = facts::read(Some(&root.join("src/a").to_string_lossy()), &root.to_string_lossy());
    assert!(read.src_dev != 0 && read.src_dev == read.dest_dev && read.deletable);
    let missing = facts::read(Some("/nonexistent/flea-dg2/a"), "/nonexistent/flea-dg2");
    assert_eq!((missing.src_dev, missing.dest_dev, missing.deletable), (0, 0, false));
    if root.join(".flea-test-root").is_file() && root.starts_with(std::env::temp_dir()) && root != std::env::temp_dir() {
        std::fs::remove_dir_all(&root).unwrap();
    }
}

struct Recorder {
    window: u64,
    seen: RefCell<Vec<(bool, Vec<u8>)>>,
}

impl Client for Recorder {
    fn offer(&self, offered: &Offered<'_>) -> Option<NativeDragActions> {
        (offered.window == self.window && offered.types.iter().any(|t| t == URILIST)).then(NativeDragActions::default)
    }
    fn land(&self, native: &Native<'_>) -> Option<bool> {
        if native.window != self.window {
            return None;
        }
        self.seen.borrow_mut().push((native.dropped, native.get(URILIST).map_or(Vec::new(), |b| b.to_vec())));
        Some(true)
    }
}

#[test]
fn m9_dg2_router_hands_each_window_to_its_own_client_and_refuses_the_rest() {
    let router = Rc::new(Router::default());
    let a = Rc::new(Recorder { window: 1, seen: RefCell::new(Vec::new()) });
    let b = Rc::new(Recorder { window: 2, seen: RefCell::new(Vec::new()) });
    let _ka = router.register(a.clone());
    let kb = router.register(b.clone());
    let types = s(&[URILIST]);
    let actions = router.offer(&Offered { window: 2, position: (0., 0.), types: &types });
    assert!(actions.copy);
    let refused = router.offer(&Offered { window: 3, position: (0., 0.), types: &types });
    assert!(!refused.copy && !refused.move_);
    let bytes: Arc<[u8]> = Arc::from(&b"file:///a\r\n"[..]);
    let get = |mime: &str| (mime == URILIST).then(|| bytes.clone());
    assert!(router.land(&Native::new(1, (0., 0.), true, &types, &get, None, false, false)));
    assert!(!router.land(&Native::new(3, (0., 0.), true, &types, &get, None, false, false)));
    assert_eq!(a.seen.borrow().len(), 1);
    assert!(b.seen.borrow().is_empty());
    drop(kb);
    let gone = router.offer(&Offered { window: 2, position: (0., 0.), types: &types });
    assert!(!gone.copy, "a dropped registration takes no more drags");
}

#[test]
fn m9_dg2_own_drag_coming_back_lands_once_through_the_restored_drag() {
    let (send, receive) = async_channel::unbounded();
    let held = Rc::new(std::cell::Cell::new(true));
    let files = join::Files { window: 4, send, held: held.clone() };
    let own = lift(&pane(), 0, 1, (0., 0.), false, false, 1).expect("lift").marker.encode();
    let foreign = Marker { instance: "other-process".into(), ..Marker::parse(&own).unwrap() }.encode();
    let uris: Arc<[u8]> = Arc::from(&b"file:///home/u/src/b%20c.txt\r\n"[..]);
    let types = s(&[URILIST, ROWS_MIME]);
    let land = |marker: &str| {
        let marker: Arc<[u8]> = Arc::from(marker.as_bytes());
        let uris = uris.clone();
        let get = move |mime: &str| match mime {
            URILIST => Some(uris.clone()),
            ROWS_MIME => Some(marker.clone()),
            _ => None,
        };
        files.land(&Native::new(4, (1., 1.), true, &types, &get, None, false, false))
    };
    assert_eq!(land(&own), Some(true));
    assert!(receive.try_recv().is_err(), "the restored pane drop lands it, not the native copy");
    assert_eq!(land(&foreign), Some(true));
    assert!(receive.try_recv().is_ok(), "another process's drag lands natively");
    held.set(false);
    assert_eq!(land(&own), Some(true));
    assert!(receive.try_recv().is_ok(), "with no lift held here the native drop lands");
}

#[test]
fn m9_dg2_the_file_client_never_claims_a_tab_lift() {
    let (send, receive) = async_channel::unbounded();
    let files = join::Files { window: 4, send, held: Rc::new(std::cell::Cell::new(false)) };
    let types = s(&[crate::tabs::transfer::TAB_MIME, URILIST, "text/plain"]);
    assert!(files.offer(&Offered { window: 4, position: (1., 1.), types: &types }).is_none());
    let bytes: Arc<[u8]> = Arc::from(&b"file:///a\r\n"[..]);
    let get = |_: &str| Some(bytes.clone());
    assert_eq!(files.land(&Native::new(4, (1., 1.), true, &types, &get, None, false, false)), None);
    assert!(receive.try_recv().is_err());
    let plain = s(&[URILIST]);
    let accepted = files.offer(&Offered { window: 4, position: (1., 1.), types: &plain }).expect("files");
    assert!(accepted.copy && accepted.move_, "a move-only Shift lift still negotiates");
    assert_eq!(accepted.preferred, NativeDragAction::Copy, "copy is preferred, so a source offering both never deletes");
}
