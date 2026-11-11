// The open decision and the lit row, ported from tests/js/places.js.
use super::placeview::*;
use super::rail::{FavouriteError, Group, Kind, RailEntry, RecordError};
use super::testing::record;
use crate::jsondoc::Json;

fn entry(group: Group, kind: Kind, path: &str) -> RailEntry {
    RailEntry {
        group,
        kind,
        path: path.into(),
        ..RailEntry::default()
    }
}

pub(super) fn share(uri: &str, mounted: bool) -> RailEntry {
    RailEntry {
        group: Group::Network,
        kind: Kind::Share,
        uri: uri.into(),
        mounted: Some(mounted),
        ..RailEntry::default()
    }
}

fn fav(label: &str, path: &str) -> RailEntry {
    RailEntry {
        group: Group::Favourite,
        kind: Kind::Favourite,
        original: Some(record(label, path)),
        ..RailEntry::default()
    }
}

#[test]
fn rail_cursor_follows_identity_across_rebuilds() {
    let home = entry(Group::Home, Kind::Home, "/home/test");
    let trash = RailEntry {
        group: Group::Trash,
        kind: Kind::Trash,
        path: "trash:///".into(),
        ..RailEntry::default()
    };
    let network = share("smb://host/data", false);
    let device = RailEntry {
        group: Group::Device,
        kind: Kind::Disk,
        device: "/dev/nvme0n1".into(),
        path: "/".into(),
        ..RailEntry::default()
    };
    let (a, b) = (fav("A", "/a"), fav("B", "/a"));
    let old = vec![
        home.clone(),
        trash.clone(),
        a.clone(),
        network.clone(),
        device.clone(),
    ];
    let expanded = vec![
        home.clone(),
        trash.clone(),
        a.clone(),
        b.clone(),
        network.clone(),
        device.clone(),
    ];
    assert_eq!(
        rail_cursor_after(&old, &expanded, Some(3)),
        Some(4),
        "a Favorite addition preserves the Network target"
    );
    assert_eq!(
        rail_cursor_after(&expanded, &old, Some(5)),
        Some(4),
        "a Favorite removal preserves the Device target"
    );
    assert_eq!(
        rail_cursor_after(&[a.clone(), b.clone()], &[b.clone(), a.clone()], Some(0)),
        Some(1),
        "reordering keeps distinct labels"
    );
    assert_eq!(
        rail_cursor_after(
            &[a.clone(), a.clone(), b.clone()],
            &[b.clone(), a.clone(), a.clone()],
            Some(1)
        ),
        Some(2),
        "an unchanged duplicate occurrence stays selected"
    );
    assert_eq!(
        rail_cursor_after(
            &[a.clone(), a.clone(), b.clone()],
            &[a.clone(), b.clone()],
            Some(0)
        ),
        None,
        "removing an identical duplicate invalidates it"
    );
    assert_eq!(
        rail_cursor_after(
            &[a.clone(), b.clone()],
            &[a.clone(), a.clone(), b.clone()],
            Some(0)
        ),
        None,
        "adding an identical duplicate does not guess"
    );
    let removed = vec![home.clone(), trash.clone(), a.clone(), device.clone()];
    assert_eq!(
        rail_cursor_after(&old, &removed, Some(3)),
        None,
        "removing the selected row clears the rail cursor"
    );
    assert_eq!(
        rail_cursor_after(&[], &old, None),
        None,
        "a cleared cursor stays cleared"
    );
    assert_eq!(
        rail_cursor_after(&[], &old, Some(0)),
        Some(0),
        "the first arriving rail retains its initial Home cursor"
    );
    let counted = RailEntry {
        group: Group::Trash,
        kind: Kind::Trash,
        path: "trash:///".into(),
        ..RailEntry::default()
    };
    let mounted = RailEntry {
        mounted: Some(true),
        path: "/run/gvfs/data".into(),
        ..network.clone()
    };
    assert_eq!(
        rail_cursor_after(&[trash, network], &[counted, mounted], Some(1)),
        Some(1),
        "mount state changes do not change identity"
    );
    assert_ne!(
        rail_cursor_after(&old, &expanded, Some(3)),
        Some(3),
        "a moved rename target no longer has its old index"
    );
}

#[test]
fn a_refused_open_keeps_the_label_literal() {
    for label in ["50%2 off", "a$&b"] {
        let refused = RailEntry {
            kind: Kind::Favourite,
            label: label.into(),
            path: "/x".into(),
            original: Some(Json::Num("17".into())),
            ..RailEntry::default()
        };
        assert_eq!(
            open_entry(&refused),
            Open::Refused {
                label: label.into(),
                reason: FavouriteError::Record(RecordError::InvalidRecord)
            }
        );
        let catalog = super::testing::english();
        let said = catalog.tr_args(
            "places.favorite.open_failed",
            &[label, &catalog.tr(RecordError::InvalidRecord.key())],
        );
        assert_eq!(
            said,
            format!("Could not open {label} \u{b7} invalid favorite record")
        );
    }
    assert_eq!(
        open_entry(&RailEntry {
            kind: Kind::Trash,
            ..RailEntry::default()
        }),
        Open::Trash
    );
    let remote = RailEntry {
        kind: Kind::Favourite,
        label: "N".into(),
        path: "smb://nas/x".into(),
        original: Some(record("N", "smb://nas/x")),
        ..RailEntry::default()
    };
    assert_eq!(
        open_entry(&remote),
        Open::Network {
            uri: "smb://nas/x".into(),
            label: "N".into()
        }
    );
    let file = RailEntry {
        kind: Kind::Favourite,
        label: "F".into(),
        path: "file:///a%20b".into(),
        original: Some(record("F", "file:///a%20b")),
        ..RailEntry::default()
    };
    assert_eq!(
        open_entry(&file),
        Open::Path("/a b".into()),
        "a file:// favourite opens its decoded path"
    );
}
