// Ported from tests/js/places.js and tests/js/userdirsfixture.js, case for case.
use super::places::*;
use super::rail::{FavouriteError, Group, Kind, RecordError};
use super::testing::record;
use crate::jsondoc::Json;

#[test]
fn stored_favourites_keep_every_record() {
    let records = vec![
        record("A", "/a"),
        record("Again", "/a"),
        Json::Num("17".into()),
        record("", "bad"),
    ];
    let stored = stored_entries(&records, "/home/test");
    assert_eq!(stored.len(), 4, "Flea keeps duplicate paths");
    assert_eq!(stored[1].label, "Again", "each duplicate label is kept");
    assert_eq!(
        stored[2].original,
        Some(Json::Num("17".into())),
        "an invalid favourite remains identifiable"
    );
    assert!(stored[2].error.is_some(), "an invalid favourite is marked");
    assert_eq!(
        stored[2].label, "17",
        "an invalid original value keeps its identifying label"
    );
    assert_eq!(
        (stored[3].label.as_str(), stored[3].label_key),
        ("", "places.favorite.invalid_label"),
        "an empty label reads Invalid favorite"
    );
    assert_eq!(
        stored[2].error,
        Some(FavouriteError::Record(RecordError::InvalidRecord)),
        "the refusal follows the ruled spelling"
    );
    assert!(
        stored_entries(&[], "/home/test").is_empty(),
        "new favourites start empty"
    );
    let tilde = stored_entries(&[record("Home", "~/docs")], "/home/test");
    assert_eq!(
        tilde[0].path, "/home/test/docs",
        "tilde expands only for consumption"
    );
    assert_eq!(
        tilde[0].stored_path, "~/docs",
        "the stored tilde stays intact"
    );
    let labelless = Json::Obj(vec![("path".into(), Json::Str("/x".into()))]);
    assert_eq!(
        stored_entries(&[labelless], "/h")[0].label,
        r#"{"path":"/x"}"#,
        "a record with no label shows its JSON"
    );
}

#[test]
fn record_errors_follow_the_validation_order() {
    assert_eq!(record_error(&record("A", "/a")), None);
    assert_eq!(record_error(&record("A", "~")), None);
    assert_eq!(record_error(&record("A", "~/x")), None);
    assert_eq!(record_error(&record("A", "smb://nas/x")), None);
    assert_eq!(
        record_error(&record("A", "smb://")),
        Some(RecordError::NotAPlace),
        "a bare scheme is no address"
    );
    assert_eq!(
        record_error(&record(" ", "/a")),
        Some(RecordError::LabelBlank)
    );
    assert_eq!(
        record_error(&record("A", "/a\nb")),
        Some(RecordError::InvalidPath)
    );
    assert_eq!(
        record_error(&record("A", "relative")),
        Some(RecordError::NotAPlace)
    );
    assert_eq!(
        record_error(&Json::Arr(vec![])),
        Some(RecordError::InvalidRecord)
    );
}

#[test]
fn sidebar_width_snaps_to_its_stops() {
    assert_eq!(sidebar_width(Some(0.0)), 160, "clamps low");
    assert_eq!(sidebar_width(Some(999.0)), 256, "clamps high");
    assert_eq!(sidebar_width(Some(231.0)), 224, "snaps to the nearer stop");
    assert_eq!(sidebar_width(None), 192, "a bad width type falls back");
    assert_eq!(
        sidebar_width(Some(f64::NAN)),
        192,
        "a non-finite width falls back"
    );
}

#[test]
fn bookmarks_keep_file_lines_only() {
    let b = bookmarks("file:///home/gm/Downloads Downloads\nfile:///home/gm/My%20Files\nsmb://192.168.1.10/ NAS\n");
    assert_eq!(b.len(), 2, "a non-file bookmark is skipped");
    assert_eq!(b[0].label, "Downloads", "the trailing label wins");
    assert_eq!(b[1].path, "/home/gm/My Files", "a percent escape decodes");
    assert_eq!(b[1].label, "My Files", "no label falls back to the leaf");
    assert_eq!(
        bookmarks("file:///home/gm/bad%zz Bad")[0].path,
        "/home/gm/bad%zz",
        "a malformed escape keeps the raw path"
    );
    assert_eq!(
        bookmarks("file:///home/gm/My%20Files Mine")[0].path,
        "/home/gm/My Files",
        "a well-formed escape still decodes"
    );
}

const NET: &str = "file:///home/gm/Downloads Downloads\nsmb://192.168.1.10/data NAS\nsmb://10.0.0.9/backups Backups\n";
const NET_RELABELLED: &str = "file:///home/gm/Downloads Downloads\nsmb://192.168.1.10/data Homelab\nsmb://10.0.0.9/backups Backups\n";

#[test]
fn relabel_rewrites_matching_lines_or_appends_one() {
    assert_eq!(
        relabel(NET, "smb://192.168.1.10/data", "Homelab"),
        NET_RELABELLED,
        "only the matched line's label changes"
    );
    assert_eq!(
        relabel(NET, "smb://192.168.1.10/data/", "Homelab"),
        NET_RELABELLED,
        "a trailing-slash uri still matches"
    );
    assert_eq!(
        relabel(NET, "smb://192.168.1.10/isos/", "ISOs"),
        format!("{NET}smb://192.168.1.10/isos ISOs\n"),
        "an unmatched uri is appended"
    );
    assert_eq!(
        relabel("", "smb://host/share", "Share"),
        "smb://host/share Share\n",
        "an empty file needs no leading blank line"
    );
    assert_eq!(
        relabel("smb://a/b Existing", "smb://host/share", "Share"),
        "smb://a/b Existing\nsmb://host/share Share\n",
        "a body missing its newline still starts a new line"
    );
    assert_eq!(
        relabel(NET, "smb://192.168.1.10/data", "   "),
        NET,
        "a blank submitted name is a no-op"
    );
    let dupes = "smb://192.168.1.10/data NAS\nfile:///home/gm/Downloads Downloads\nsmb://192.168.1.10/data/ Old NAS\n";
    assert_eq!(relabel(dupes, "smb://192.168.1.10/data", "Homelab"), "smb://192.168.1.10/data Homelab\nfile:///home/gm/Downloads Downloads\nsmb://192.168.1.10/data/ Homelab\n", "every matching line rewrites");
    assert_eq!(
        relabel(
            "  smb://192.168.1.10/data NAS\n",
            "smb://192.168.1.10/data",
            "Homelab"
        ),
        "smb://192.168.1.10/data Homelab\n",
        "an indented line rewrites rather than duplicating"
    );
    assert_eq!(
        relabel(NET, "smb://192.168.1.10/data", "Home\nlab"),
        NET_RELABELLED,
        "an embedded newline cannot fork a new line"
    );
}

#[test]
fn favorites_merge_home_dirs_and_bookmarks() {
    let dirs = "XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\nXDG_DOCUMENTS_DIR=\"$HOME/Documents\"\n";
    let marks =
        "file:///home/gm/Downloads Grabbed\nfile:///srv/media Media\nsmb://192.168.1.10/data NAS\n";
    let favs = favorites("/home/gm", dirs, marks, |label| {
        if label == "Downloads" {
            "download"
        } else {
            "folder"
        }
    });
    assert_eq!(
        (favs[0].path.as_str(), favs[0].label_key),
        ("/home/gm", "places.row.home"),
        "Home leads and is never parsed out of a file"
    );
    assert_eq!(
        (favs[1].label.as_str(), favs[2].label.as_str()),
        ("Downloads", "Documents"),
        "the XDG dirs follow in file order"
    );
    let labels: Vec<&str> = favs.iter().skip(1).map(|e| e.label.as_str()).collect();
    assert_eq!(
        labels,
        ["Downloads", "Documents", "Media"],
        "a path already placed keeps its first position and label"
    );
    assert_eq!(favs.len(), 4, "a non-file bookmark never reaches FAVORITES");
    assert_eq!(
        (favs[3].group, favs[3].kind),
        (Group::Favorite, Kind::Favorite),
        "every row is tagged as a favourite"
    );
    assert_eq!(favs[1].glyph, "download", "the caller resolves the mark");
    assert_eq!(
        favorites("/home/gm", "", "", |_| "m").len(),
        1,
        "a box with neither file still gets Home"
    );
}

#[test]
fn replace_rewrites_the_saved_line_in_place() {
    let saved = "file:///home/gm/Downloads Downloads\nsmb://nas:445/isos NAS isos\nsmb://other/data Other\n";
    assert_eq!(
        replace(saved, "smb://nas/isos", "sftp://nas2/isos", "NAS isos"),
        "file:///home/gm/Downloads Downloads\nsftp://nas2/isos NAS isos\nsmb://other/data Other\n"
    );
    assert_eq!(
        replace(saved, "smb://nas:445/isos/", "smb://nas/plates", "Plates"),
        "file:///home/gm/Downloads Downloads\nsmb://nas/plates Plates\nsmb://other/data Other\n",
        "the default port is matched as a rename matches it"
    );
    assert_eq!(
        replace(saved, "smb://absent/", "smb://nas/new", "New"),
        format!("{saved}smb://nas/new New\n"),
        "an address nothing saved appends"
    );
    assert_eq!(
        replace("", "", "smb://nas/plates", ""),
        "smb://nas/plates plates\n",
        "a label nobody typed falls back to the share's name"
    );
    assert_eq!(
        replace(saved, "smb://nas/isos", "", "NAS"),
        saved,
        "an empty new address rewrites nothing"
    );
}

#[test]
fn home_entries_lead_with_home() {
    let rows = home_entries("/home/gm", "XDG_MUSIC_DIR=\"$HOME/Music\"", |label| {
        if label == "Home" {
            "house"
        } else {
            "music"
        }
    });
    assert_eq!(
        rows.iter()
            .map(|r| (r.path.as_str(), r.glyph))
            .collect::<Vec<_>>(),
        [("/home/gm", "house"), ("/home/gm/Music", "music")]
    );
    assert!(rows
        .iter()
        .all(|r| r.group == Group::Home && r.kind == Kind::Home));
}

#[test]
fn uri_helpers_match_the_mount_rules() {
    use super::{normalize, under_gvfs};
    assert_eq!(normalize("smb://host:445/data/"), "smb://host/data");
    assert_eq!(
        normalize("smb://host"),
        "smb://host/",
        "a bare root keeps its slash"
    );
    assert_eq!(
        normalize("sftp://host:2222/x"),
        "sftp://host:2222/x",
        "a non-default port stays"
    );
    assert_eq!(
        normalize("ftps://host:21/x"),
        "ftps://host/x",
        "gio drops ftps 21"
    );
    assert!(under_gvfs(
        "/run/user/1000/gvfs/smb-share:server=nas,share=data"
    ));
    assert!(
        !under_gvfs("/run/user/1000/gvfs/"),
        "no scheme, no gvfs mount"
    );
}
