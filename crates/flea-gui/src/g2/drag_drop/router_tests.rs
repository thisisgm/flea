//! The process router and the window's file client: per-window claims, tab exclusion, self re-entry and stale lifts.
use super::router::{Client, Native, Offered, Router};
use super::tests::{pane, s, Pane};
use super::*;
use flea::ui_logic_drag_drop::{Marker, ROWS_MIME, URILIST};
use gpui::{NativeDragAction, NativeDragActions};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

struct Recorder {
    window: u64,
    seen: RefCell<Vec<(bool, Vec<u8>)>>,
}

impl Client for Recorder {
    fn offer(&self, offered: &Offered<'_>) -> Option<NativeDragActions> {
        (offered.window == self.window && offered.types.iter().any(|t| t == URILIST))
            .then(NativeDragActions::default)
    }
    fn land(&self, native: &Native<'_>) -> Option<bool> {
        if native.window != self.window {
            return None;
        }
        self.seen.borrow_mut().push((
            native.dropped,
            native.get(URILIST).map_or(Vec::new(), |b| b.to_vec()),
        ));
        Some(true)
    }
}

#[test]
fn m9_dg2_router_hands_each_window_to_its_own_client_and_refuses_the_rest() {
    let router = Rc::new(Router::default());
    let a = Rc::new(Recorder {
        window: 1,
        seen: RefCell::new(Vec::new()),
    });
    let b = Rc::new(Recorder {
        window: 2,
        seen: RefCell::new(Vec::new()),
    });
    let _ka = router.register(a.clone());
    let kb = router.register(b.clone());
    let types = s(&[URILIST]);
    let actions = router.offer(&Offered {
        window: 2,
        position: (0., 0.),
        types: &types,
    });
    assert!(actions.copy);
    let refused = router.offer(&Offered {
        window: 3,
        position: (0., 0.),
        types: &types,
    });
    assert!(!refused.copy && !refused.move_);
    let bytes: Arc<[u8]> = Arc::from(&b"file:///a\r\n"[..]);
    let get = |mime: &str| (mime == URILIST).then(|| bytes.clone());
    assert!(router.land(&Native::new(
        1,
        (0., 0.),
        true,
        &types,
        &get,
        None,
        false,
        false
    )));
    assert!(!router.land(&Native::new(
        3,
        (0., 0.),
        true,
        &types,
        &get,
        None,
        false,
        false
    )));
    assert_eq!(a.seen.borrow().len(), 1);
    assert!(b.seen.borrow().is_empty());
    drop(kb);
    let gone = router.offer(&Offered {
        window: 2,
        position: (0., 0.),
        types: &types,
    });
    assert!(!gone.copy, "a dropped registration takes no more drags");
}

#[test]
fn m9_dg2_own_drag_coming_back_lands_once_through_the_restored_drag() {
    let (send, receive) = async_channel::unbounded();
    let held_lift = lift(&pane(), 0, 1, (0., 0.), false, false, 1).expect("lift");
    let files = stale_holder(send, &held_lift);
    let own = held_lift.marker.encode();
    let foreign = Marker {
        instance: "other-process#1".into(),
        ..held_lift.marker.clone()
    }
    .encode();
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
        files.land(&Native::new(
            4,
            (1., 1.),
            true,
            &types,
            &get,
            None,
            false,
            false,
        ))
    };
    assert_eq!(land(&own), Some(true));
    assert!(
        receive.try_recv().is_err(),
        "the restored pane drop lands it, not the native copy"
    );
    assert_eq!(land(&foreign), Some(true));
    assert!(
        receive.try_recv().is_ok(),
        "another process's drag lands natively"
    );
    files.source(&gpui::NativeDragEvent {
        source_id: 1,
        action: None,
        phase: gpui::NativeDragPhase::Finished,
    });
    assert_eq!(land(&own), Some(true));
    assert!(
        receive.try_recv().is_ok(),
        "once its session ended elsewhere the lift is no longer held, so it lands natively"
    );
}

#[test]
fn m9_dg2_the_file_client_never_claims_a_tab_lift() {
    let (send, receive) = async_channel::unbounded();
    let files = files::Files {
        window: 4,
        send,
        held: Rc::new(RefCell::new(None)),
        hover: Rc::default(),
        ended: None,
    };
    let types = s(&[crate::tabs::transfer::TAB_MIME, URILIST, "text/plain"]);
    assert!(files
        .offer(&Offered {
            window: 4,
            position: (1., 1.),
            types: &types
        })
        .is_none());
    let bytes: Arc<[u8]> = Arc::from(&b"file:///a\r\n"[..]);
    let get = |_: &str| Some(bytes.clone());
    assert_eq!(
        files.land(&Native::new(
            4,
            (1., 1.),
            true,
            &types,
            &get,
            None,
            false,
            false
        )),
        None
    );
    assert!(receive.try_recv().is_err());
    let plain = s(&[URILIST]);
    let accepted = files
        .offer(&Offered {
            window: 4,
            position: (1., 1.),
            types: &plain,
        })
        .expect("files");
    assert!(
        accepted.copy && accepted.move_,
        "a move-only Shift lift still negotiates"
    );
    assert_eq!(
        accepted.preferred,
        NativeDragAction::Copy,
        "copy is preferred, so a source offering both never deletes"
    );
}

#[test]
fn m9_dg2_a_stale_lift_never_swallows_another_lifts_drop() {
    let stale =
        lift(&pane(), 0, 1, (0., 0.), false, false, 1).expect("A's lift that left the window");
    let other_pane = Pane {
        folder: "/home/u/other".into(),
        listing: 3,
        rows: vec![("c.txt", false)],
        selected: vec![],
    };
    let next = lift(&other_pane, 0, 0, (0., 0.), false, false, 2).expect("B's later lift");
    let (send, receive) = async_channel::unbounded();
    let files = stale_holder(send, &stale);
    let types = s(&[URILIST, ROWS_MIME]);
    let dropped = |marker: String, uri: &'static [u8]| {
        let marker: Arc<[u8]> = Arc::from(marker.as_bytes());
        let uris: Arc<[u8]> = Arc::from(uri);
        let get = move |mime: &str| match mime {
            URILIST => Some(uris.clone()),
            ROWS_MIME => Some(marker.clone()),
            _ => None,
        };
        files.land(&Native::new(
            4,
            (1., 1.),
            true,
            &types,
            &get,
            None,
            false,
            false,
        ))
    };
    assert_eq!(
        dropped(next.marker.encode(), b"file:///home/u/other/c.txt\r\n"),
        Some(true)
    );
    let arrived = receive
        .try_recv()
        .expect("another lift's drop lands, never swallowed by a stale held lift");
    assert_eq!(arrived.urls, s(&["file:///home/u/other/c.txt"]));
    assert_eq!(
        dropped(stale.marker.encode(), b"file:///home/u/src/b%20c.txt\r\n"),
        Some(true)
    );
    assert!(
        receive.try_recv().is_err(),
        "the held lift itself still lands once, through its restored drag"
    );
}

// The file client of window 4 while it holds `lift`.
fn stale_holder(send: async_channel::Sender<files::Arrived>, lift: &Lift) -> files::Files {
    let held = Rc::new(RefCell::new(Some((
        lift.marker.instance.clone(),
        lift.serial,
    ))));
    files::Files {
        window: 4,
        send,
        held,
        hover: Rc::default(),
        ended: None,
    }
}
