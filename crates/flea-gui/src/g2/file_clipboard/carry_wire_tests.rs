use super::{
    carry::Flock,
    carry_ipc::State,
    carry_wire::{now, Offer},
};
use crate::row::marks::{Language, Mark};
use flea::ui_logic_file_clipboard::carry::{Lifted, Source};

fn offer() -> Offer {
    Offer::new(
        Lifted {
            rows: vec![1, 5],
            paths: vec!["/a/quo\"te".into(), "/a/new\nline".into()],
            source: Source {
                window: u64::MAX,
                side: 1,
                folder: "/a".into(),
                listing: u64::MAX - 1,
            },
            serial: 17,
        },
        Flock {
            marks: vec![
                Mark::FolderOpen,
                Mark::Language(Language::for_slug("rust").unwrap()),
            ],
            name: "quo\"te".into(),
        },
        0,
    )
}

#[test]
fn carry_wire_roundtrip_keeps_large_ids_exact_paths_and_language_marks() {
    let offer = offer();
    assert_eq!(Offer::decode(&offer.encode()), Some(offer));
}

#[test]
fn carry_wire_expired_or_partial_payload_never_rides() {
    let mut offer = offer();
    offer.expires = now() - 1;
    assert!(Offer::decode(&offer.encode()).is_none());
    offer.expires = now() + 30_000;
    offer.lifted.paths.pop();
    assert!(Offer::decode(&offer.encode()).is_none());
    offer.lifted.paths.push("relative".into());
    assert!(Offer::decode(&offer.encode()).is_none());
}

#[test]
fn carry_wire_cancel_before_offer_cannot_resurrect_it_or_replace_a_newer_lift() {
    let offer = offer();
    let mut state = State::default();
    state.retire(offer.token.clone());
    assert!(!state.accept(&offer));
    let newer = Offer::new(offer.lifted.clone(), offer.flock.clone(), offer.stamp);
    assert!(state.accept(&newer));
    assert!(!state.accept(&offer));
}

#[test]
fn carry_wire_refresh_cannot_replace_a_newer_lift() {
    let first = offer();
    let mut newer = Offer::new(first.lifted.clone(), first.flock.clone(), first.stamp);
    newer.pid += 1;
    newer.token = format!("{}-{}", newer.pid, newer.stamp);
    newer.lifted.serial += 1;
    let mut refresh = Offer::new(first.lifted.clone(), first.flock.clone(), newer.stamp);
    refresh.lift_stamp = first.lift_stamp;
    assert!(refresh.stamp > newer.stamp);
    let mut peer = State::default();
    assert!(peer.accept(&newer));
    assert!(
        !peer.accept(&refresh),
        "OLD_REFRESH_MUST_NOT_REPLACE_NEWER_LIFT"
    );
    let mut source = State::default();
    assert!(source.accept(&refresh));
    assert!(
        source.accept(&newer),
        "NEWER_LIFT_MUST_REPLACE_OLDER_REFRESH"
    );
}

#[test]
fn carry_wire_dropped_lift_refresh_cannot_return() {
    let first = offer();
    let mut peer = State::default();
    assert!(peer.accept(&first));
    peer.drop_lift(&first);
    let mut refresh = Offer::new(first.lifted.clone(), first.flock.clone(), first.stamp);
    refresh.lift_stamp = first.lift_stamp;
    assert!(!peer.accept(&refresh), "DROPPED_LIFT_REFRESH_MUST_STAY_OUT");
    let newer = Offer::new(first.lifted.clone(), first.flock.clone(), refresh.stamp);
    assert!(peer.accept(&newer), "NEWER_LIFT_MUST_STILL_RIDE");
}
