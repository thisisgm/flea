use super::*;
use crate::i18n::{I18n, Paths};

fn english() -> I18n {
    let none = std::path::PathBuf::from("/nonexistent/flea-dg2");
    I18n::load(Paths {
        omarchy: none.clone(),
        language_file: none,
        catalogs: Vec::new(),
    })
}

fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn marker(instance: &str) -> Marker {
    Marker {
        instance: instance.into(),
        rows: vec![1, 3],
        copy: false,
        source: "/home/u/a b".into(),
        dev: 42,
        deletable: true,
        shift: false,
        listing: 7,
    }
}

#[test]
fn pressed_row_in_the_selection_carries_the_selection_else_itself() {
    assert_eq!(carried(&[2, 5, 9], 5), vec![2, 5, 9]);
    assert_eq!(carried(&[2, 5, 9], 4), vec![4]);
    assert_eq!(carried(&[], 0), vec![0]);
}

#[test]
fn marker_round_trips_with_a_percent_encoded_source() {
    let lifted = marker("me");
    let text = lifted.encode();
    assert_eq!(text, "me\n1,3\nmove\n%2Fhome%2Fu%2Fa%20b\n42\n1\n0\n7");
    assert_eq!(Marker::parse(&text), Some(lifted));
}

#[test]
fn legacy_markers_parse_and_malformed_ones_refuse() {
    let five = Marker::parse("x\n0\ncopy\n/raw/src\n9").expect("five fields");
    assert!(five.copy && five.deletable && !five.shift);
    assert_eq!(
        (five.source.as_str(), five.dev, five.listing),
        ("/raw/src", 9, 0)
    );
    let six = Marker::parse("x\n0\nmove\n/raw\n9\n0").expect("six fields");
    assert!(!six.deletable);
    for bad in [
        "",
        "x\n0\nlink\n/s\n9",
        "x\n0\nmove\n/s\nnine",
        "x\n0\nmove\n/s\n9\n2",
        "x\n0\nmove\n%2Fs\n9\n1\n3",
        "x\n0\nmove\n%zz\n9\n1\n0",
        "x\n0\nmove\n%2Fs\n9\n1\n0\n-1",
        "x\n0\nmove\n/s",
    ] {
        assert_eq!(Marker::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn uri_encodes_each_component_and_keeps_the_separators() {
    assert_eq!(uri_for("/d/a b#c?.txt"), "file:///d/a%20b%23c%3F.txt");
    assert_eq!(
        file_path("file:///d/a%20b.txt").as_deref(),
        Some("/d/a b.txt")
    );
    assert_eq!(file_path("file://localhost/d/x").as_deref(), Some("/d/x"));
    assert_eq!(file_path("file://host/d/x"), None);
    assert_eq!(file_path("http://e/y"), None);
}

#[test]
fn foreign_plain_text_adds_absolute_lines_but_an_own_drag_never_rereads_it() {
    let urls = s(&["file:///a/x.txt"]);
    let plain = "/a/x.txt\r\n/b/y\nhttp://e/z\n";
    assert_eq!(sources(&urls, Some(plain), false), s(&["/a/x.txt", "/b/y"]));
    assert_eq!(sources(&urls, Some(plain), true), s(&["/a/x.txt"]));
}

#[test]
fn refusals_come_in_the_oracle_order() {
    let paths = s(&["/a/dir", "/a/f"]);
    assert_eq!(
        refusal(&paths, "/a/dir/sub", false),
        Some(Refusal::InsideDrag)
    );
    assert_eq!(refusal(&paths, "/a/dir", false), Some(Refusal::InsideDrag));
    assert_eq!(refusal(&paths, "/a", false), Some(Refusal::AlreadyHere));
    assert_eq!(
        refusal(&s(&["/top"]), "/", false),
        Some(Refusal::AlreadyHere)
    );
    assert_eq!(refusal(&[], "/b", false), Some(Refusal::NoLocalFiles));
    assert_eq!(refusal(&[], "/b", true), None);
    assert_eq!(refusal(&paths, "/b", false), None);
    assert_eq!(refusal(&s(&["/a/dirx"]), "/a/dir", false), None);
}

#[test]
fn refusal_sentences_are_the_frozen_ones() {
    let i18n = english();
    assert_eq!(
        Refusal::InsideDrag.say(&i18n),
        "That folder is inside the drag."
    );
    assert_eq!(Refusal::AlreadyHere.say(&i18n), "Already in this folder.");
    assert_eq!(
        Refusal::NoLocalFiles.say(&i18n),
        "That drag has no local files."
    );
}

#[test]
fn verb_follows_keys_then_device_then_deletable() {
    assert_eq!(verb_for(true, true, 1, 2, true), Verb::Link);
    assert_eq!(verb_for(true, false, 1, 1, true), Verb::Copy);
    assert_eq!(verb_for(false, true, 1, 2, true), Verb::Move);
    assert_eq!(verb_for(false, false, 4, 4, true), Verb::Move);
    assert_eq!(verb_for(false, false, 4, 5, true), Verb::Copy);
    assert_eq!(verb_for(false, false, 0, 4, true), Verb::Copy);
    assert_eq!(verb_for(false, false, 4, 0, true), Verb::Copy);
    assert_eq!(verb_for(false, false, 4, 4, false), Verb::Copy);
}

#[test]
fn seen_keys_override_the_lift_bits() {
    assert!(drop_held(true, Seen::Unseen));
    assert!(!drop_held(true, Seen::Released));
    assert!(drop_held(false, Seen::Held));
}

#[test]
fn foreign_drag_reads_the_platform_action() {
    let only = |copy, move_, link| Proposed { copy, move_, link };
    assert_eq!(foreign_held(only(false, true, false)), (false, true));
    assert_eq!(foreign_held(only(true, false, false)), (true, false));
    assert_eq!(foreign_held(only(false, false, true)), (true, true));
    assert_eq!(foreign_held(only(true, true, false)), (false, false));
    assert_eq!(foreign_held(Proposed::default()), (false, false));
}

#[test]
fn drop_verb_uses_the_marker_then_the_platform() {
    let own = marker("me");
    let none = Proposed::default();
    assert_eq!(
        drop_verb(Some(&own), none, 42, Seen::Unseen, Seen::Unseen),
        Verb::Move
    );
    assert_eq!(
        drop_verb(Some(&own), none, 43, Seen::Unseen, Seen::Unseen),
        Verb::Copy
    );
    assert_eq!(
        drop_verb(Some(&own), none, 42, Seen::Held, Seen::Unseen),
        Verb::Copy
    );
    let move_only = Proposed {
        copy: false,
        move_: true,
        link: false,
    };
    assert_eq!(
        drop_verb(None, move_only, 9, Seen::Unseen, Seen::Unseen),
        Verb::Move
    );
    assert_eq!(
        drop_verb(None, none, 9, Seen::Unseen, Seen::Unseen),
        Verb::Copy
    );
}

#[test]
fn lift_offers_the_marker_always_and_paths_only_when_every_row_resolved() {
    let own = marker("me");
    let whole = lift_payload(&own, Some(&s(&["/d/a b", "/d/c"])));
    assert_eq!(whole[0], (ROWS_MIME, own.encode().into_bytes()));
    assert_eq!(
        whole[1],
        (URILIST, b"file:///d/a%20b\r\nfile:///d/c\r\n".to_vec())
    );
    assert_eq!(whole[2], (PLAIN, b"/d/a b\n/d/c".to_vec()));
    let wide = lift_payload(&own, None);
    assert_eq!(wide, vec![(ROWS_MIME, own.encode().into_bytes())]);
}

#[test]
fn a_drop_becomes_one_transfer_or_one_link_into_the_folder() {
    let own = marker("me");
    let dropped = Dropped {
        marker: Some(&own),
        urls: &s(&["file:///home/u/a%20b/x"]),
        plain: None,
        proposed: Proposed::default(),
        ctrl: Seen::Unseen,
        shift: Seen::Unseen,
    };
    let ask = drop_into(&dropped, "/home/u/c", 42).expect("lands");
    assert_eq!(
        ask,
        PasteAsk::Transfer {
            op: "move".into(),
            paths: s(&["/home/u/a b/x"]),
            dest: "/home/u/c".into(),
            spends_cut: false,
        }
    );
    let linked = Dropped {
        ctrl: Seen::Held,
        shift: Seen::Held,
        ..dropped
    };
    assert!(matches!(
        drop_into(&linked, "/home/u/c", 42),
        Ok(PasteAsk::Link {
            kind: LinkKind::Relative,
            ..
        })
    ));
    assert_eq!(
        drop_into(&dropped, "/home/u/a b", 42),
        Err(Some(Refusal::AlreadyHere))
    );
    assert_eq!(drop_into(&dropped, "", 42), Err(None));
}

#[test]
fn a_malformed_marker_never_drops() {
    let urls = s(&["file:///a/x"]);
    assert!(!marker_ok(Some("garbage")));
    assert!(marker_ok(None));
    assert!(marker_ok(Some(&marker("me").encode())));
    let parsed = Marker::parse("garbage");
    let dropped = Dropped {
        marker: parsed.as_ref(),
        urls: &urls,
        plain: None,
        proposed: Proposed::default(),
        ctrl: Seen::Unseen,
        shift: Seen::Unseen,
    };
    assert!(
        drop_into(&dropped, "/b", 1).is_ok(),
        "an absent marker is a foreign drag"
    );
}

#[test]
fn offer_is_one_action_so_a_receiver_takes_the_lift_verb() {
    assert_eq!(
        offer(false, false),
        (true, false, Verb::Copy),
        "a plain lift never offers move"
    );
    assert_eq!(offer(true, false), (true, false, Verb::Copy));
    assert_eq!(offer(false, true), (false, true, Verb::Move));
    assert_eq!(offer(true, true), (true, false, Verb::Copy));
}

#[test]
fn too_wide_reads_the_frozen_drag_line_and_reach_note() {
    let i18n = english();
    assert_eq!(
        too_wide(&i18n, 400, false, false),
        "Move 400 items to a folder · ctrl copies, shift moves · too wide to drag out"
    );
    assert_eq!(
        too_wide(&i18n, 1, true, false),
        "Copy 1 item to a folder · too wide to drag out"
    );
    assert_eq!(
        too_wide(&i18n, 2, true, true),
        "Link 2 items to a folder · too wide to drag out"
    );
}

#[test]
fn line_to_over_a_listed_folder_spells_the_keys_as_the_board_does() {
    let i18n = english();
    assert_eq!(
        line_to(&i18n, 1, "Docs", Verb::Move),
        "Move 1 item to Docs · ctrl copies, shift moves"
    );
}

#[test]
fn rail_destination_names_only_mounted_local_folders() {
    let urls = s(&["file:///home/u/x"]);
    let at = |kind, path: &str, mounted| rail_destination(kind, path, mounted, &urls, None, false);
    assert_eq!(
        at(RailKind::Folder, "/run/media/u/USB/", true),
        "/run/media/u/USB"
    );
    assert_eq!(at(RailKind::Folder, "/run/media/u/USB", false), "");
    assert_eq!(at(RailKind::Other, "/home/u", true), "");
    assert_eq!(
        at(RailKind::Folder, "/run/user/1000/gvfs/mtp:host=Phone", true),
        ""
    );
    assert_eq!(
        at(RailKind::Folder, "/home/u", true),
        "",
        "already in this folder"
    );
    assert_eq!(at(RailKind::Folder, "relative", true), "");
}

#[test]
fn shelf_reads_the_token_then_the_intent_fixed_at_the_lift() {
    assert_eq!(shelf("tok\ncopy"), Some(("tok", true)));
    assert_eq!(shelf("tok\nmove"), Some(("tok", false)));
    assert_eq!(shelf("tok"), Some(("tok", false)));
    assert_eq!(shelf(""), None, "no token is no shelf drag");
    assert_eq!(shelf("\ncopy"), None);
}
