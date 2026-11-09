use super::*;

#[test]
fn plugin_place_paste_clipboard_round_trips_plugin_paths() {
    let paths = vec!["plugin://example.virtual-place/renamed %.txt".into()];
    for op in ["copy", "cut"] {
        assert_eq!(check_selection(op, &paths), Ok(()));
        assert_eq!(parse_urilist(&build_urilist(&paths)), (paths.clone(), 0));
        assert_eq!(
            parse_gnome(&build_gnome(op, &paths)),
            Some((op.into(), paths.clone(), 0))
        );
        assert!(String::from_utf8(build_urilist(&paths))
            .unwrap()
            .starts_with("plugin://"));
    }
}

#[test]
fn plugin_place_paste_clipboard_refuses_invalid_plugin_paths() {
    for path in [
        "plugin://invalid/x",
        "plugin://example.virtual-place/../x",
        "plugin://example.virtual-place/./x",
        "plugin://example.virtual-place/x\n",
        "plugin://example.virtual-place",
    ] {
        assert!(check_selection("copy", &[path.into()]).is_err(), "{path:?}");
    }
    for uri in [
        b"plugin://example.virtual-place/%2e%2e/x".as_slice(),
        b"plugin://example.virtual-place/%00",
        b"plugin://invalid/x",
    ] {
        assert!(parse_urilist(uri).0.is_empty());
    }
}

// Every format round trips a name holding a newline, %, #, ?, spaces and non-ASCII UTF-8.
fn awkward() -> Vec<String> {
    vec![
        "/tmp/line\nbreak.txt".to_string(),
        "/tmp/100%.txt".to_string(),
        "/tmp/#hash.txt".to_string(),
        "/tmp/what?.txt".to_string(),
        "/tmp/with spaces.txt".to_string(),
        "/tmp/caf\u{e9} \u{2615}.txt".to_string(),
    ]
}

#[test]
fn gnome_round_trips_without_a_trailing_newline() {
    for op in ["copy", "cut"] {
        let paths = awkward();
        let bytes = build_gnome(op, &paths);
        assert!(!bytes.ends_with(b"\n"));
        let (got_op, got_paths, skipped) = parse_gnome(&bytes).expect("a file selection");
        assert_eq!(got_op, op);
        assert_eq!(got_paths, paths);
        assert_eq!(skipped, 0);
    }
}

#[test]
fn urilist_round_trips_with_crlf_and_a_final_crlf() {
    let paths = awkward();
    let bytes = build_urilist(&paths);
    assert!(bytes.ends_with(b"\r\n"));
    let (got, skipped) = parse_urilist(&bytes);
    assert_eq!(got, paths);
    assert_eq!(skipped, 0);
    // LF alone reads too, the shape Drag.js never writes but others do.
    let lf = bytes.windows(2).fold(Vec::new(), |mut acc, w| {
        if w == b"\r\n" {
            acc.push(b'\n');
        } else if acc.last() != Some(&b'\r') {
            acc.push(w[0]);
        }
        acc
    });
    let (got, _) = parse_urilist(&lf);
    assert_eq!(got, paths);
}

#[test]
fn comments_localhost_and_plain_text_behave() {
    let bytes = b"# a comment\nfile://localhost/tmp/a.txt\n/text/plain\n";
    let (got, skipped) = parse_urilist(bytes);
    assert_eq!(got, vec!["/tmp/a.txt".to_string()]);
    assert_eq!(
        skipped, 1,
        "the bare path is refused, the comment is not counted"
    );
    assert!(parse_gnome(b"hello\nfile:///tmp/a.txt").is_none());
    assert_eq!(build_plain(&["/a".to_string()]), b"/a");
}

#[test]
fn every_refusal_is_a_skip_and_never_a_failed_read() {
    // Bad escapes, NUL, a relative path, dot components, duplicates, other schemes; a non-UTF-8 name is carried exactly.
    let lines: Vec<Vec<u8>> = vec![
        b"copy".to_vec(),
        b"file:///tmp/ok.txt".to_vec(),
        b"file:///tmp/%zz.txt".to_vec(),
        b"file:///tmp/%00.txt".to_vec(),
        b"file://other/tmp/a.txt".to_vec(),
        b"file:///tmp/../etc.txt".to_vec(),
        b"file:///tmp/./a.txt".to_vec(),
        b"file:///tmp/%ff.txt".to_vec(),
        b"file:///tmp/%+1.txt".to_vec(),
        b"file:///tmp/ok.txt".to_vec(),
        b"https://example.com/x".to_vec(),
        b"file:///tmp/second.txt".to_vec(),
    ];
    let bytes = lines.join(&b'\n');
    let (op, paths, skipped) = parse_gnome(&bytes).expect("a file selection");
    assert_eq!(op, "copy");
    assert_eq!(
        paths,
        vec![
            "/tmp/ok.txt".to_string(),
            "/tmp/\u{fffd}.txt\0ff2e747874".to_string(),
            "/tmp/second.txt".to_string()
        ]
    );
    assert_eq!(skipped, 8);
}

#[test]
fn kde_and_flea_tokens_decide_cut_and_identity() {
    assert!(parse_kde_cut(b"1"));
    assert!(parse_kde_cut(b"1\r\n"));
    assert!(!parse_kde_cut(b"0"));
    assert_eq!(
        parse_flea(b"cut ab12cd34ab12cd34ab12cd34ab12cd34"),
        Some((
            "cut".into(),
            "ab12cd34ab12cd34ab12cd34ab12cd34".into(),
            None
        ))
    );
    assert!(parse_flea(b"copy short").is_none());
    assert!(parse_flea(b"copy ab12cd34ab12cd34ab12cd34ab12cd3!").is_none());
    assert!(parse_flea(b"move ab12cd34ab12cd34ab12cd34ab12cd34").is_none());
}

#[test]
fn flea_owner_pids_are_optional_and_do_not_change_the_token() {
    let token = "ab12cd34ab12cd34ab12cd34ab12cd34";
    let old = format!("cut {}", token);
    assert_eq!(pid_of(old.as_bytes()), None);
    let bytes = build_flea("cut", token);
    assert_eq!(parsed_pair(&bytes), parsed_pair(old.as_bytes()));
    assert_eq!(pid_of(&bytes), Some(std::process::id()));
    for pid in ["0", "-1", "2147483648", "no-pid", "23 24"] {
        let bytes = format!("cut {} {}", token, pid).into_bytes();
        assert_eq!(parsed_pair(&bytes), parsed_pair(old.as_bytes()));
        assert_eq!(pid_of(&bytes), None);
    }
}

// The operation and token a Flea payload parses to, without its optional owner pid.
fn parsed_pair(bytes: &[u8]) -> Option<(String, String)> {
    parse_flea(bytes).map(|(op, token, _)| (op, token))
}

fn pid_of(bytes: &[u8]) -> Option<u32> {
    parse_flea(bytes).and_then(|(_, _, pid)| pid)
}

#[test]
fn clip_paths_are_absolute_without_nul_or_parent_climbs() {
    assert!(validate_clip_paths(&["/a/b".to_string()]).is_ok());
    assert!(validate_clip_paths(&["relative".to_string()]).is_err());
    assert!(validate_clip_paths(&["/a\0b".to_string()]).is_err());
    assert_eq!(
        validate_clip_paths(&["/a/../b".to_string()]),
        Err("/a/../b climbs out of its directory".to_string())
    );
    assert_eq!(
        validate_clip_paths(&["/tmp/./a".to_string()]),
        Err("/tmp/./a holds a . component".to_string())
    );
}

#[test]
fn a_hundred_thousand_paths_is_the_most_any_payload_names() {
    let ok: Vec<String> = (0..100000).map(|i| format!("/tmp/f{}.txt", i)).collect();
    assert!(validate_clip_paths(&ok).is_ok());
    let mut over = ok;
    over.push("/tmp/one-more.txt".to_string());
    assert!(validate_clip_paths(&over).is_err());
}

#[test]
fn a_selection_past_the_cap_names_its_size_and_the_limit_in_plain_words() {
    let over: Vec<String> = (0..=MAX_CLIP_PATHS)
        .map(|i| format!("/tmp/f{}", i))
        .collect();
    assert_eq!(
        check_path_cap(&over),
        Err("100001 items are selected, and the clipboard takes at most 100000".to_string())
    );
}

#[test]
fn a_made_token_is_32_hex_chars() {
    // /dev/urandom never reaches EOF, so a read to the end must fail here instead of hanging.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(make_token());
    });
    let token = rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("a token within 2 s")
        .expect("a token made");
    assert_eq!(token.len(), 32);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
}

#[test]
fn only_copy_and_cut_own() {
    assert!(is_op("copy") && is_op("cut"));
    assert!(!is_op("move"));
}

#[test]
fn a_foreign_gnome_trailing_newline_is_not_a_refused_file() {
    let (op, paths, skipped) = parse_gnome(b"copy\nfile:///tmp/a\n").unwrap();
    assert_eq!(op, "copy");
    assert_eq!(paths, vec!["/tmp/a"]);
    assert_eq!(skipped, 0, "a formatting line is not a refused URI");
}

#[test]
fn tui17_text_payload_offers_no_file_identity_or_uri_list() {
    assert!(check_selection("text", &["é\nfilename".into()]).is_ok());
    let payload = payloads("text", &["é\nfilename".into()], "unused");
    assert_eq!(payload.len(), 3);
    for (mime, bytes) in payload {
        assert!(matches!(mime, PLAIN_UTF8 | PLAIN | UTF8_STRING));
        assert_eq!(bytes, "é\nfilename".as_bytes());
    }
    assert!(check_selection("text", &["nul\0byte".into()]).is_err());
    assert!(check_selection("text", &["a".into(), "b".into()]).is_err());
}

#[test]
fn a_raw_name_round_trips_by_its_own_bytes_and_reads_lossy_as_plain_text() {
    let paths = vec![format!("/tmp/{}", flea::bytename::name_text(b"r\xff.txt"))];
    assert_eq!(build_urilist(&paths), b"file:///tmp/r%FF.txt\r\n");
    assert_eq!(parse_urilist(&build_urilist(&paths)), (paths.clone(), 0));
    assert_eq!(build_plain(&paths), "/tmp/r\u{fffd}.txt".as_bytes());
    assert!(
        validate_clip_paths(&paths).is_ok(),
        "the exact text is a path, not a stray NUL"
    );
}
