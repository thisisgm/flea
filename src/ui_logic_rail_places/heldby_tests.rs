// Which rail row lights for what a pane shows, ported from tests/js/places.js and split from placeview_tests.rs.
use super::placeview::*;
use super::placeview_tests::share;
use super::rail::{FavouriteError, Group, Kind, RailEntry, RecordError};

fn row(kind: Kind, path: &str, mounted: Option<bool>) -> RailEntry {
    RailEntry {
        kind,
        path: path.into(),
        mounted,
        ..RailEntry::default()
    }
}

fn tag(name: &'static str) -> RailEntry {
    RailEntry {
        kind: Kind::Tag,
        tag: Some(name),
        ..RailEntry::default()
    }
}

fn dir(share_key: &str) -> View {
    View::Dir {
        share_key: share_key.into(),
    }
}

#[test]
fn held_by_lights_the_deepest_row_holding_the_view() {
    let home = row(Kind::Home, "/home/gm", None);
    let documents = row(Kind::Home, "/home/gm/Documents", None);
    let fav_work = RailEntry {
        group: Group::Favourite,
        ..row(Kind::Favourite, "/home/gm/Work", None)
    };
    let fav_docs = RailEntry {
        group: Group::Favourite,
        ..row(Kind::Favourite, "/home/gm/Documents", None)
    };
    let mounted_share = RailEntry {
        group: Group::Network,
        ..share("smb://nas/data", true)
    };
    let sleeping = share("smb://nas/idle", false);
    let kernel = RailEntry {
        uri: "/mnt/nas".into(),
        ..row(Kind::Share, "/mnt/nas", Some(true))
    };
    let disk = row(Kind::Disk, "/", Some(true));
    let usb = row(Kind::Volume, "/run/media/gm/USB", Some(true));
    let ejected = row(Kind::Volume, "", Some(false));
    let rail = vec![
        home.clone(),
        row(Kind::Recent, "flea:recent", None),
        documents.clone(),
        row(Kind::Trash, "trash:///", None),
        fav_work,
        tag("Red"),
        tag("Blue"),
        mounted_share,
        sleeping,
        kernel,
        disk.clone(),
        usb,
        ejected,
    ];
    let at = |path: &str| held_by(&rail, path, &dir(""));
    assert_eq!(
        at("/home/gm/Documents/claude"),
        Some(2),
        "Documents lights inside it"
    );
    assert_eq!(
        at("/home/gm/Documents"),
        Some(2),
        "a row's own folder lights that row"
    );
    assert_eq!(
        held_by(
            &[home.clone(), documents.clone(), disk.clone()],
            "/home/gm/Work/field",
            &dir("")
        ),
        Some(0),
        "Home lights when no favourite holds Work"
    );
    assert_eq!(
        at("/home/gm/Work/field"),
        Some(4),
        "the deepest row wins over Home"
    );
    assert_eq!(
        at("/home/gm/Pictures"),
        Some(0),
        "Home lights for a folder no other row holds"
    );
    assert_eq!(
        at("/home/gm/Documents2"),
        Some(0),
        "a name-prefix sibling is not held"
    );
    assert_eq!(
        at("/fx/v/work"),
        Some(10),
        "slash holds every local path nothing else holds"
    );
    assert_eq!(at("/"), Some(10), "slash reaches slash itself");
    assert_eq!(
        at("/run/media/gm/USB/photos"),
        Some(11),
        "a volume wins over slash"
    );
    assert_eq!(at("/mnt/nas"), Some(9), "a kernel share wins over slash");
    assert_eq!(
        held_by(
            &[fav_docs, documents.clone()],
            "/home/gm/Documents/x",
            &dir("")
        ),
        Some(0),
        "a tie goes to the first row"
    );
    assert_eq!(
        held_by(
            &[
                row(Kind::Volume, "/run/media/gm/USB", Some(false)),
                disk.clone()
            ],
            "/run/media/gm/USB/photos",
            &dir("")
        ),
        Some(1),
        "an unmounted volume never lights"
    );
    let invalid = RailEntry {
        error: Some(FavouriteError::Record(RecordError::LabelBlank)),
        ..row(Kind::Favourite, "/a", None)
    };
    assert_eq!(
        held_by(&[invalid], "/a", &dir("")),
        None,
        "an invalid record never lights"
    );
    assert_eq!(
        held_by(&[home.clone(), documents.clone()], "/srv/x", &dir("")),
        None,
        "nothing holding the folder answers none"
    );
    assert_eq!(
        held_by(&[], "/a", &dir("")),
        None,
        "an empty rail answers none"
    );
    assert_eq!(held_by(&rail, "", &dir("")), None, "no path answers none");
    assert_eq!(
        held_by(&rail, "/", &View::Recent),
        Some(1),
        "Recent lights its own row"
    );
    assert_eq!(
        held_by(&rail, "/home/gm", &View::Trash),
        Some(3),
        "Trash lights its own row"
    );
    assert_eq!(
        held_by(&[home.clone(), disk.clone()], "/", &View::Recent),
        None,
        "a view whose row is absent lights nothing"
    );
    assert_eq!(
        held_by(&rail, "/home/gm", &View::Tag("Blue".into())),
        Some(6),
        "a tag view lights that tag"
    );
    assert_eq!(
        held_by(&rail, "/home/gm", &View::Tag("Green".into())),
        None,
        "a tag with no row lights nothing"
    );
    let gvfs = "/run/user/1000/gvfs/smb-share:server=nas,share=data/photos";
    assert_eq!(
        held_by(&rail, gvfs, &dir("smb://nas/data")),
        Some(7),
        "a gvfs path lights the share opened under it"
    );
    assert_eq!(
        held_by(&rail, gvfs, &dir("")),
        None,
        "no remembered share lights nothing"
    );
    assert_eq!(
        held_by(&rail, gvfs, &dir("smb://nas/idle")),
        None,
        "an unmounted share lights nothing"
    );
    assert_eq!(
        held_by(&[disk, home], gvfs, &dir("smb://other/x")),
        None,
        "a gvfs path never lights by prefix"
    );
    let other = share("smb://nas/a", true);
    assert_eq!(
        held_by(
            &[other, share("smb://nas/data", true)],
            gvfs,
            &dir("smb://nas/data")
        ),
        Some(1),
        "another address is passed over"
    );
    let alias = RailEntry {
        mount_uri: "smb://nas/data".into(),
        ..share("smb://nas/alias", true)
    };
    assert_eq!(
        held_by(&[alias], gvfs, &dir("smb://nas/data")),
        Some(0),
        "a share answers by the address it is mounted under"
    );
}

#[test]
fn lit_view_picks_the_view_the_pane_shows() {
    assert_eq!(
        lit_view(true, true, true, "tag:Red", None),
        View::Trash,
        "an open Trash wins"
    );
    assert_eq!(
        lit_view(false, true, false, "", None),
        View::Recent,
        "Recent mode is the Recent view"
    );
    assert_eq!(
        lit_view(false, false, true, "tag:Red", None),
        View::Tag("Red".into()),
        "a landed tag walk is that tag's view"
    );
    assert_eq!(
        lit_view(false, false, false, "tag:Red", None),
        dir(""),
        "a tag query with no walk is no tag view"
    );
    assert_eq!(
        lit_view(false, false, true, "report", None),
        dir(""),
        "an ordinary walk is a folder view"
    );
    assert_eq!(
        lit_view(false, false, false, "", Some("smb://nas/data")),
        dir("smb://nas/data"),
        "the remembered share's key rides along"
    );
}
