use super::*;

#[test]
fn trash_thumbnail_commands_are_recognized() {
    for line in [
        r#"{"c":"trashthumb","id":41,"cacheOnly":true}"#,
        r#"{"c":"trashthumbcancel","ids":[41,42]}"#,
    ] {
        assert!(
            !matches!(parse_request(line), Request::Unknown),
            "unrecognized Trash thumbnail command: {line}"
        );
    }
}

#[test]
fn locate_preserves_request_identity_and_reports_absence() {
    assert!(matches!(parse_request(r#"{"c":"locate","path":"/a/file"}"#), Request::Locate { path } if path == "/a/file"));
    assert_eq!(located_line("/a", "/a/file", Some(3)), r#"{"t":"located","directory":"/a","path":"/a/file","index":3}"#);
    assert_eq!(located_line("/a", "/a/\"\n", None), r#"{"t":"located","directory":"/a","path":"/a/\"\n","index":-1}"#);
    assert!(matches!(parse_request(r#"{"c":"locate","paths":["/a/file"],"id":2,"menuId":7}"#),
        Request::LocateMany { paths, id: 2, menu_id: 7, transfer_id: 0 } if paths == ["/a/file"]));
    assert!(matches!(parse_request(r#"{"c":"locate","paths":["/a/file"],"transferId":12}"#),
        Request::LocateMany { paths, id: 0, menu_id: 0, transfer_id: 12 } if paths == ["/a/file"]));
    assert_eq!(located_many_line("/a", 2, 0, &[("/a/\"\n", 3)], None),
        r#"{"t":"located","directory":"/a","id":2,"transferId":0,"matches":[{"path":"/a/\"\n","index":3}],"ok":true,"error":""}"#);
}

#[test]
fn convert_preserves_probe_and_caller_identity_without_changing_legacy_activation() {
    assert!(matches!(parse_request(r#"{"c":"convert","path":"/a.png","dest":"/a.jpg","strip":true,"menuId":7,"requestId":29,"check":true}"#),
        Request::Convert { path, dest, strip: true, menu_id: 7, request_id: 29, check: true } if path == "/a.png" && dest == "/a.jpg"));
    assert!(matches!(parse_request(r#"{"c":"convert","path":"/a.png","dest":"/a.jpg"}"#),
        Request::Convert { strip: false, menu_id: 0, request_id: 0, check: false, .. }));
}

#[test]
fn a_peek_carries_the_folder_order_the_column_remembers() {
    assert!(matches!(parse_request(r#"{"c":"peek","path":"/a","first":1,"by":"size","desc":true}"#),
        Request::Peek { by, desc: true, .. } if by.as_deref() == Some("size")));
    assert!(matches!(parse_request(r#"{"c":"peek","path":"/a","first":1}"#), Request::Peek { by: None, desc: false, .. }));
}

#[test]
fn a_watching_peek_names_the_column_directories_drawn() {
    assert!(matches!(parse_request(r#"{"c":"peek","path":"/a","first":1,"watch":true,"keep":["/a","/a/b, [odd]"]}"#),
        Request::Peek { watch: true, keep, .. } if keep == ["/a", "/a/b, [odd]"]));
    assert!(matches!(parse_request(r#"{"c":"peek","path":"/a","first":1}"#), Request::Peek { watch: false, keep, .. } if keep.is_empty()));
}

#[test]
fn parses_each_request_shape() {
    match parse_request(r#"{"c":"list","path":"/home/gm","first":350}"#) {
        Request::List { path, first, hidden, want_changed, keep } => {
            assert_eq!(path, "/home/gm");
            assert_eq!(first, 350);
            assert!(!hidden);
            assert!(!want_changed);
            assert!(keep.is_empty());
        }
        _ => panic!("expected List"),
    }
    match parse_request(r#"{"c":"list","path":"/a","keep":["/b","/c, [d]"]}"#) {
        Request::List { keep, .. } => {
            assert_eq!(keep, vec!["/b".to_string(), "/c, [d]".to_string()])
        }
        _ => panic!("expected List"),
    }
    match parse_request(r#"{"c":"window","start":1200,"count":350}"#) {
        Request::Window { start, count } => {
            assert_eq!(start, 1200);
            assert_eq!(count, 350);
        }
        _ => panic!("expected Window"),
    }
    match parse_request(r#"{"c":"sort","by":"size","desc":true}"#) {
        Request::Sort { by, desc, anchor } => {
            assert_eq!(by, "size");
            assert!(desc);
            assert!(anchor.is_none());
        }
        _ => panic!("expected Sort"),
    }
    match parse_request(r#"{"c":"search","path":"/home/gm","query":"bench","hidden":true}"#) {
        Request::Search { path, query, hidden } => {
            assert_eq!(path, "/home/gm");
            assert_eq!(query, "bench");
            assert!(hidden);
        }
        _ => panic!("expected Search"),
    }
    match parse_request(r#"{"c":"mkdir","path":"/home/gm","name":"New Folder"}"#) {
        Request::MkDir { path, name, .. } => assert_eq!((path.as_str(), name.as_str()), ("/home/gm", "New Folder")),
        _ => panic!("expected MkDir"),
    }
    assert!(matches!(parse_request(r#"{"c":"searchcancel"}"#), Request::SearchCancel));
    assert!(matches!(parse_request(r#"{"c":"quit"}"#), Request::Quit));
}

#[test]
fn a_paths_line_escapes_every_element_and_survives_an_empty_list() {
    assert_eq!(paths_line(&[]), r#"{"t":"paths","paths":[]}"#);
    assert_eq!(
        paths_line(&["/home/gm/a.txt".to_string(), "/home/gm/say \"hi\".txt".to_string()]),
        r#"{"t":"paths","paths":["/home/gm/a.txt","/home/gm/say \"hi\".txt"]}"#
    );
}

#[test]
fn a_shebang_request_names_its_path_and_caller_id() {
    assert!(matches!(parse_request(r#"{"c":"shebang","path":"/home/gm/run.sh","id":3}"#),
        Request::Shebang { path, id: 3 } if path == "/home/gm/run.sh"));
    assert_eq!(crate::backend::shebang::shebang_line("/home/gm/run.sh", true, 3),
        r#"{"t":"shebang","path":"/home/gm/run.sh","hasShebang":true,"id":3}"#);
}

#[test]
fn junk_is_unknown_rather_than_a_panic() {
    assert!(matches!(parse_request(""), Request::Unknown));
    assert!(matches!(parse_request("not json at all"), Request::Unknown));
    assert!(matches!(parse_request("{"), Request::Unknown));
    assert!(matches!(parse_request(r#"{"c":"nope"}"#), Request::Unknown));
}

#[test]
fn a_malformed_escape_never_panics() {
    // A bad escape only empties that one field; "c" alone decides the variant.
    assert!(matches!(parse_request(r#"{"c":"list","path":"/tmp/a"#), Request::List { .. }));
    assert!(matches!(parse_request(r#"{"c":"list","path":"\u00"#), Request::List { .. }));
    assert!(matches!(parse_request(r#"{"c":"list","path":"\ud800","first":1}"#), Request::List { .. }));
    assert!(matches!(parse_request(r#"{"c":"list","path":"trailing\"#), Request::List { .. }));
    assert!(matches!(parse_request(r#"{"c":"window","start":-5,"count":10}"#), Request::Window { .. }));
    assert!(matches!(parse_request(r#"{"c":"sort","by":"name","desc":truthy}"#), Request::Sort { .. }));
}

#[test]
fn a_list_request_without_first_defaults_to_zero() {
    match parse_request(r#"{"c":"list","path":"/tmp"}"#) {
        Request::List { first, .. } => assert_eq!(first, 0),
        _ => panic!("expected List"),
    }
}

#[test]
fn a_list_request_carries_its_hidden_flag() {
    match parse_request(r#"{"c":"list","path":"/tmp","first":0,"hidden":true}"#) {
        Request::List { hidden, .. } => assert!(hidden),
        _ => panic!("expected List"),
    }
    // Missing and explicitly false both mean dotfiles stay out of the scan.
    match parse_request(r#"{"c":"list","path":"/tmp","first":0}"#) {
        Request::List { hidden, .. } => assert!(!hidden),
        _ => panic!("expected List"),
    }
    match parse_request(r#"{"c":"list","path":"/tmp","first":0,"hidden":false}"#) {
        Request::List { hidden, .. } => assert!(!hidden),
        _ => panic!("expected List"),
    }
}

#[test]
fn a_list_request_names_the_reload_count_only_when_asked() {
    match parse_request(r#"{"c":"list","path":"/tmp","first":0,"wantChanged":true}"#) {
        Request::List { want_changed, .. } => assert!(want_changed),
        _ => panic!("expected List"),
    }
    // Absent is the silent re-read every navigation and watch already takes.
    match parse_request(r#"{"c":"list","path":"/tmp","first":0}"#) {
        Request::List { want_changed, .. } => assert!(!want_changed),
        _ => panic!("expected List"),
    }
}

#[test]
fn emits_a_listed_line_naming_the_directory_it_listed() {
    let s = say_listed(100000, 26.4, 2.5, 56, "/home/gm", true);
    assert_eq!(s, r#"{"t":"listed","n":100000,"read":26.400,"sort":2.500,"v":56,"w":true,"path":"/home/gm"}"#);
}

#[test]
fn a_sort_request_carries_its_cursor_anchor_and_a_bare_one_carries_none() {
    match parse_request(r#"{"c":"sort","by":"size","anchor":"/home/gm/amber"}"#) {
        Request::Sort { by, anchor, .. } => {
            assert_eq!(by, "size");
            assert_eq!(anchor.as_deref(), Some("/home/gm/amber"));
        }
        _ => panic!("expected Sort"),
    }
    // An empty anchor is absent, so no client can name "nothing" and change the reply shape.
    for line in [r#"{"c":"sort","by":"size"}"#, r#"{"c":"sort","by":"size","anchor":""}"#] {
        match parse_request(line) {
            Request::Sort { anchor, .. } => assert!(anchor.is_none(), "{}", line),
            _ => panic!("expected Sort for {}", line),
        }
    }
}

#[test]
fn an_anchored_listed_line_answers_the_anchor_and_a_bare_one_is_unchanged() {
    assert_eq!(
        with_anchor(&say_listed(3, 0.0, 2.5, 56, "/home/gm", true), "/home/gm/amber", 1),
        r#"{"t":"listed","n":3,"read":0.000,"sort":2.500,"v":56,"w":true,"path":"/home/gm","anchor":"/home/gm/amber","anchorIndex":1}"#
    );
    assert_eq!(
        with_anchor(&say_listed(3, 0.0, 2.5, 56, "/home/gm", true), "/home/gm/gone", -1),
        r#"{"t":"listed","n":3,"read":0.000,"sort":2.500,"v":56,"w":true,"path":"/home/gm","anchor":"/home/gm/gone","anchorIndex":-1}"#
    );
    // Escaped like every other string on this wire, so a quote in the path cannot break the line.
    let s = with_anchor(&say_listed(1, 0.0, 0.0, 0, "/home/gm", true), "/home/gm/say \"hi\".txt", 0);
    assert_eq!(s.lines().count(), 1);
    assert!(s.contains(r#""anchor":"/home/gm/say \"hi\".txt""#));
    // Without an anchor the reply is byte-for-byte today's line; see the listed test above.
    assert!(!say_listed(3, 0.0, 2.5, 56, "/home/gm", true).contains("anchor"));
}

#[test]
fn a_relist_names_added_plus_removed_rather_than_net_delta() {
    let base = say_listed(3, 0.0, 0.0, 1, "/d", true);
    assert_eq!(with_changed(&base, 2),
        r#"{"t":"listed","n":3,"read":0.000,"sort":0.000,"v":1,"w":true,"path":"/d","changed":2}"#);
    assert_eq!(with_changed(&base, 0).matches("changed").count(), 1);
}

#[test]
fn a_listed_line_says_when_the_directory_cannot_be_written() {
    use std::os::unix::fs::PermissionsExt;
    // Marked sandbox, so a failed assert cannot leave a 0o555 dir behind.
    let d = crate::backend::testdir::TestDir::new("listed-w");
    let dir = d.dir("w");
    let open = listed_line(1, 0.0, 0.0, 1, &dir.to_string_lossy(), crate::backend::ops::dir_writable(&dir));
    assert!(open.contains(r#""w":true"#), "{open}");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let locked = listed_line(1, 0.0, 0.0, 1, &dir.to_string_lossy(), crate::backend::ops::dir_writable(&dir));
    assert!(locked.contains(r#""w":false"#), "{locked}");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn a_thumb_request_carries_its_rows_in_order() {
    match parse_request(r#"{"c":"thumb","rows":[2,17,140]}"#) {
        Request::Thumb { rows, cache_only, .. } => {
            assert_eq!(rows, vec![2, 17, 140]);
            assert!(!cache_only, "absent is today's full path");
        }
        _ => panic!("expected Thumb"),
    }
    match parse_request(r#"{"c":"thumb","rows":[2],"cacheOnly":true}"#) {
        Request::Thumb { rows, cache_only, .. } => {
            assert_eq!(rows, vec![2]);
            assert!(cache_only, "an off class names it");
        }
        _ => panic!("expected Thumb"),
    }
    match parse_request(r#"{"c":"thumbcancel","rows":[17,140]}"#) {
        Request::ThumbCancel { rows } => assert_eq!(rows, vec![17, 140]),
        _ => panic!("expected ThumbCancel"),
    }
}

#[test]
fn a_dirsize_request_carries_its_rows_and_cancel_carries_none() {
    match parse_request(r#"{"c":"dirsize","rows":[4,9]}"#) {
        Request::DirSize { rows } => assert_eq!(rows, vec![4, 9]),
        _ => panic!("expected DirSize"),
    }
    // Unlike thumbcancel, dirsizecancel names nothing: it always cancels everything in flight.
    assert!(matches!(parse_request(r#"{"c":"dirsizecancel"}"#), Request::DirSizeCancel));
    assert!(matches!(
        parse_request(r#"{"c":"dirsizecancel","rows":[1,2]}"#),
        Request::DirSizeCancel
    ));
}

#[test]
fn a_thumb_request_with_no_usable_rows_is_still_a_thumb_request() {
    // The empty form of thumbcancel is a documented feature, so a missing or garbled rows must not change the variant.
    for line in [
        r#"{"c":"thumbcancel"}"#,
        r#"{"c":"thumbcancel","rows":[]}"#,
        r#"{"c":"thumbcancel","rows":[-1]}"#,
        r#"{"c":"thumbcancel","rows":nonsense}"#,
    ] {
        match parse_request(line) {
            Request::ThumbCancel { rows } => assert!(rows.is_empty(), "{} should carry no rows", line),
            _ => panic!("expected ThumbCancel for {}", line),
        }
    }
    match parse_request(r#"{"c":"thumb","rows":[1.5,"x",99999999999999999999]}"#) {
        Request::Thumb { rows, .. } => assert!(rows.is_empty()),
        _ => panic!("expected Thumb"),
    }
}

#[test]
fn emits_a_thumbed_line_for_a_generated_row_and_for_a_failed_one() {
    assert_eq!(
        thumbed_line(2, "/home/gm/.cache/thumbnails/large/b98fa408.png", 75.8234),
        r#"{"t":"thumbed","row":2,"file":"/home/gm/.cache/thumbnails/large/b98fa408.png","ms":75.823}"#
    );
    // The empty file is the whole failure form on this wire, so a client never waits forever.
    assert_eq!(thumbed_line(0, "", 0.0), r#"{"t":"thumbed","row":0,"file":"","ms":0.000}"#);
}

#[test]
fn emits_a_dirsized_line_complete_and_partial() {
    assert_eq!(
        dirsized_line(4, 1048576, false, 12.5),
        r#"{"t":"dirsized","row":4,"bytes":1048576,"partial":false,"ms":12.500}"#
    );
    // partial:true is a floor, not a wrong exact number; the cell renders it with a leading ">".
    assert_eq!(
        dirsized_line(9, 200, true, 2000.0),
        r#"{"t":"dirsized","row":9,"bytes":200,"partial":true,"ms":2000.000}"#
    );
}

#[test]
fn a_thumbed_path_is_escaped_like_every_other_string() {
    let s = thumbed_line(7, "/tmp/say \"hi\"\nand\ttab.png", 1.0);
    assert_eq!(s.lines().count(), 1);
    assert!(s.contains(r#""file":"/tmp/say \"hi\"\nand\ttab.png""#));
}

#[test]
fn emits_an_error_line_naming_operation_and_path() {
    let e = FleaError {
        where_: "scan".to_string(),
        path: "/root".to_string(),
        msg: "permission denied".to_string(),
    };
    assert_eq!(
        error_line(&e),
        r#"{"t":"error","where":"scan","path":"/root","msg":"permission denied"}"#
    );
    // The mode rides on the same line, after msg, so an old reader keeps parsing what it knows.
    assert_eq!(
        error_line_with_mode(&e, 0o40750),
        r#"{"t":"error","where":"scan","path":"/root","msg":"permission denied","mode":16872}"#
    );
    // Zero is "I could not stat it either", and that draws no mode string, so it sends no field.
    assert_eq!(
        error_line_with_mode(&e, 0),
        r#"{"t":"error","where":"scan","path":"/root","msg":"permission denied"}"#
    );
    // A typed cause rides after msg, the same way.
    assert_eq!(
        error_line_with_code(&e, "emptyclipboard"),
        r#"{"t":"error","where":"scan","path":"/root","msg":"permission denied","code":"emptyclipboard"}"#
    );
}

#[test]
fn a_devs_request_carries_the_paths_named_and_a_bare_one_carries_none() {
    assert!(matches!(parse_request(r#"{"c":"devs","paths":["/","/home/gm","/odd, [dir]"]}"#),
        Request::Devs { paths, id: 0 } if paths == ["/", "/home/gm", "/odd, [dir]"]));
    assert!(matches!(parse_request(r#"{"c":"devs"}"#), Request::Devs { paths, id: 0 } if paths.is_empty()));
    assert!(matches!(parse_request(r#"{"c":"devs","paths":["/usb"],"id":19}"#), Request::Devs { paths, id: 19 } if paths == ["/usb"]));
}

#[test]
fn a_jump_request_carries_both_client_sources_and_a_bare_one_carries_none() {
    assert!(matches!(parse_request(r#"{"c":"jump","id":4,"favourites":["/home/gm/Projects"],"recent":["/home/gm/a, b.txt"]}"#),
        Request::Jump { id: 4, ranking: 0, favourites, recent } if favourites == ["/home/gm/Projects"] && recent == ["/home/gm/a, b.txt"]));
    assert!(matches!(parse_request(r#"{"c":"jump","id":5,"ranking":4,"favourites":[],"recent":[]}"#),
        Request::Jump { id: 5, ranking: 4, favourites, recent } if favourites.is_empty() && recent.is_empty()));
    assert!(matches!(parse_request(r#"{"c":"jump"}"#),
        Request::Jump { id: 0, ranking: 0, favourites, recent } if favourites.is_empty() && recent.is_empty()));
}

#[test]
fn slow_names_its_op_path_and_sentence() {
    assert_eq!(
        slow_line("rename", "/hung/a.txt", "/hung is slow. The rename will finish on its own."),
        r#"{"t":"slow","op":"rename","path":"/hung/a.txt","msg":"/hung is slow. The rename will finish on its own."}"#
    );
}

#[test]
fn clip_requests_carry_their_operation_paths_and_token() {
    assert!(matches!(parse_request(r#"{"c":"clipSet","op":"copy","paths":["/home/gm/a.txt"]}"#),
        Request::ClipSet { op, paths } if op == "copy" && paths == ["/home/gm/a.txt"]));
    assert!(matches!(parse_request(r#"{"c":"clipGet"}"#), Request::ClipGet));
    assert!(matches!(parse_request(r#"{"c":"clipClear","token":"ab12"}"#),
        Request::ClipClear { token, cut } if token == "ab12" && cut.is_empty()));
    assert!(matches!(parse_request(r#"{"c":"clipClear"}"#), Request::ClipClear { token, cut } if token.is_empty() && cut.is_empty()));
    assert!(matches!(parse_request(r#"{"c":"clipClear","cut":["/a"]}"#),
        Request::ClipClear { cut, .. } if cut == ["/a"]));
    assert!(matches!(parse_request(r#"{"c":"clipWatch"}"#), Request::ClipWatch));
}
