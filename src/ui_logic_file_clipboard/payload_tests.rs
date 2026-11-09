//! The payload shapes pinned byte for byte: the fork doc's cut example, the backend's GNOME, URI, plain, KDE and FLEA forms, and the foreign chooser.
use super::payload::*;

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|item| item.to_string()).collect()
}

#[test]
fn plugin_place_paste_frontend_payload_preserves_plugin_scheme_and_cut() {
    let paths = paths(&["plugin://example.virtual-place/renamed %.txt"]);
    assert_eq!(
        build_urilist(&paths),
        b"plugin://example.virtual-place/renamed%20%25.txt\r\n"
    );
    assert_eq!(parse_urilist(&build_urilist(&paths)), (paths.clone(), 0));
    assert_eq!(
        parse_gnome(&build_gnome("cut", &paths)),
        Some(("cut".into(), paths, 0))
    );
    assert!(parse_urilist(b"plugin://example.virtual-place/%2e%2e/x")
        .0
        .is_empty());
}

#[test]
fn the_fork_docs_cut_example_builds_exactly() {
    let mine = paths(&["/example/a b"]);
    assert_eq!(build_urilist(&mine), b"file:///example/a%20b\r\n");
    assert_eq!(build_gnome("cut", &mine), b"cut\nfile:///example/a%20b");
    // The fork doc writes a final newline; both shapes read as the same selection.
    assert_eq!(
        parse_gnome(b"cut\nfile:///example/a%20b\n").unwrap().1,
        mine
    );
}

#[test]
fn gnome_round_trips_and_refuses_a_foreign_first_line() {
    let mine = paths(&["/tmp/a.txt", "/tmp/b.txt"]);
    let built = build_gnome("copy", &mine);
    assert!(!built.ends_with(b"\n"));
    let (op, back, skipped) = parse_gnome(&built).unwrap();
    assert_eq!(op, "copy");
    assert_eq!(back, mine);
    assert_eq!(skipped, 0);
    assert!(parse_gnome(b"hello\nfile:///tmp/a.txt").is_none());
}

#[test]
fn urilist_reads_crlf_and_lf_and_skips_comments() {
    let mine = paths(&["/tmp/a.txt"]);
    let built = build_urilist(&mine);
    assert!(built.ends_with(b"\r\n"));
    let (back, skipped) = parse_urilist(&built);
    assert_eq!(back, mine);
    assert_eq!(skipped, 0);
    let (back, skipped) = parse_urilist(b"# comment\nfile:///tmp/a.txt\nrelative\n");
    assert_eq!(back, mine);
    assert_eq!(skipped, 1);
}

#[test]
fn decode_refuses_hosts_nuls_and_dot_parts() {
    let (back, _) = parse_urilist(b"file://other-host/tmp/a.txt\n");
    assert!(back.is_empty());
    let (back, _) = parse_urilist(b"file://localhost/tmp/a.txt\n");
    assert_eq!(back, paths(&["/tmp/a.txt"]));
    assert!(parse_gnome(b"copy\nfile:///a/../b\n").unwrap().1.is_empty());
}

#[test]
fn kde_and_flea_tokens_read_exactly() {
    assert!(parse_kde_cut(b"1"));
    assert!(parse_kde_cut(b"1\r\n"));
    assert!(!parse_kde_cut(b"0"));
    assert_eq!(
        parse_flea(b"cut ab12cd34ab12cd34ab12cd34ab12cd34"),
        Some(("cut".into(), "ab12cd34ab12cd34ab12cd34ab12cd34".into()))
    );
    assert!(parse_flea(b"copy short").is_none());
    assert!(parse_flea(b"move ab12cd34ab12cd34ab12cd34ab12cd34").is_none());
    assert!(is_token("ab12cd34ab12cd34ab12cd34ab12cd34"));
    assert!(!is_token("short"));
    assert!(is_op("copy") && is_op("cut") && !is_op("move"));
}

#[test]
fn the_chooser_prefers_gnome_then_uri_then_nothing() {
    let mine = paths(&["/example/a b"]);
    let gnome = build_gnome("cut", &mine);
    let uri = build_urilist(&mine);
    let flea = build_flea("cut", "ab12cd34ab12cd34ab12cd34ab12cd34");
    let foreign = choose(&[
        Offer {
            mime: URILIST,
            bytes: &uri,
        },
        Offer {
            mime: GNOME,
            bytes: &gnome,
        },
        Offer {
            mime: FLEA,
            bytes: &flea,
        },
    ]);
    assert_eq!(
        foreign,
        Some(ForeignSel {
            clip: "cut".into(),
            paths: mine.clone(),
            token: "ab12cd34ab12cd34ab12cd34ab12cd34".into()
        })
    );
    let uri_only = choose(&[Offer {
        mime: URILIST,
        bytes: &uri,
    }]);
    assert_eq!(uri_only.unwrap().clip, "copy");
    let kde = build_kde_cut();
    let uri_cut = choose(&[
        Offer {
            mime: URILIST,
            bytes: &uri,
        },
        Offer {
            mime: KDE_CUT,
            bytes: &kde,
        },
    ]);
    assert_eq!(uri_cut.unwrap().clip, "cut");
    let plain = build_plain(&mine);
    assert_eq!(
        choose(&[Offer {
            mime: PLAIN,
            bytes: &plain
        }]),
        None
    );
    assert_eq!(
        choose(&[Offer {
            mime: URILIST,
            bytes: b""
        }]),
        None
    );
}

#[test]
fn a_signed_escape_is_a_skip_like_any_bad_escape() {
    let (paths, skipped) = parse_urilist(b"file:///tmp/a%+1\r\nfile:///tmp/b%2B\r\n");
    assert_eq!(paths, vec!["/tmp/b+".to_string()]);
    assert_eq!(skipped, 1);
}

#[test]
fn a_gnome_offer_that_does_not_parse_falls_through_to_the_uri_list() {
    let gnome = b"copy\r\nfile:///tmp/a\r\n";
    let uri = b"file:///tmp/b\r\n";
    let foreign = choose(&[
        Offer {
            mime: GNOME,
            bytes: gnome,
        },
        Offer {
            mime: URILIST,
            bytes: uri,
        },
    ]);
    assert_eq!(
        foreign,
        Some(ForeignSel {
            clip: "copy".into(),
            paths: vec!["/tmp/b".into()],
            token: String::new()
        })
    );
    let empty = choose(&[
        Offer {
            mime: GNOME,
            bytes: b"cut\n",
        },
        Offer {
            mime: URILIST,
            bytes: uri,
        },
    ]);
    assert_eq!(
        empty.map(|sel| sel.paths),
        Some(vec!["/tmp/b".to_string()]),
        "a GNOME offer naming no files falls through too"
    );
}
