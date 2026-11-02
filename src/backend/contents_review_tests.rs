use super::*;

fn raw_fixture() -> Fixture {
    let mut f = Fixture::new();
    for (name, bytes) in [
        (
            std::ffi::OsString::from_vec(b"raw-\xff.txt".to_vec()),
            "needle raw\n",
        ),
        (std::ffi::OsString::from("raw-�.txt"), "needle neighbor\n"),
        (std::ffi::OsString::from("safe.txt"), "needle safe\n"),
    ] {
        std::fs::write(f.root.join(name), bytes).unwrap();
    }
    f.search(false, 1, 10);
    f.finish();
    f.out.clear();
    f
}

#[test]
fn m9_g2rows_01_review_malformed_identity_fails_closed() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = raw_fixture();
    let row =
        f.st.contents
            .held
            .as_ref()
            .unwrap()
            .matches
            .iter()
            .position(|hit| !hit.operable)
            .unwrap();
    let generation = f.st.generation;
    let alias = escape(&f.root.join("raw-�.txt").to_string_lossy());
    for line in [
        format!(r#"{{"c":"paths","rows":[{row}],"listing":{generation}}} trailing"#),
        format!(r#"{{"c":"paths","rows":[{row}],"listing":{generation},}}"#),
        format!(r#"{{"c":"locate","path":"{alias}","path":"safe.txt"}}"#),
        format!(r#"{{"c":"locate","path":"{alias}","p\u0061th":"safe.txt"}}"#),
        format!(r#"{{"c":"open","payload":{{"path":"{alias}","path":"safe.txt"}}}}"#),
    ] {
        f.out.clear();
        f.request(&line);
        assert!(
            f.text().contains(r#""where":"contentsidentity""#),
            "unsafe request escaped: {line}: {}",
            f.text()
        );
        assert!(!f.text().contains(r#""t":"paths""#));
    }
    let safe =
        f.st.contents
            .held
            .as_ref()
            .unwrap()
            .matches
            .iter()
            .position(|hit| hit.operable)
            .unwrap();
    f.out.clear();
    f.request(&format!(
        r#"{{"c":"paths","rows":[{safe}],"listing":{generation}}}"#
    ));
    assert!(f.text().contains(r#""t":"paths""#));
    assert_eq!(
        std::fs::read_to_string(f.root.join("raw-�.txt")).unwrap(),
        "needle neighbor\n"
    );
}

#[test]
fn m9_g2rows_01_review_failed_and_cancelled_search_preserves_watch() {
    for cancel in [false, true] {
        let mut f = Fixture::new();
        let mut watch = Watch::start(f.tx.clone());
        watch.begin(&f.root);
        watch.commit(Vec::new());
        f.st.base = f.root.clone();
        f.st.held = Held::List;
        f.st.contents.program = Some(f.root.join("missing-rg"));
        let line = format!(
            r#"{{"c":"searchcontents","path":"{}","query":"needle","gen":1,"count":10}}"#,
            escape(&f.root.to_string_lossy())
        );
        f.event_watched(Event::Request(line), &mut watch);
        if cancel {
            f.event_watched(Event::Request(r#"{"c":"searchcancel"}"#.into()), &mut watch);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while f.st.contents.busy() {
            let event =
                f.rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .unwrap();
            f.event_watched(event, &mut watch);
        }
        std::fs::write(f.root.join("after-search.txt"), "created\n").unwrap();
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut fired = false;
        while let Ok(event) =
            f.rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            if let Event::Changed(wd) = event {
                fired |= watch.is_current(wd);
                if fired {
                    break;
                }
            }
        }
        assert!(
            fired,
            "preserved directory lost its real inotify watch after cancel={cancel}"
        );
    }
}

#[test]
fn m9_g2rows_01_review_completed_cancel_and_close_are_idempotent() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = raw_fixture();
    f.request(r#"{"c":"searchcancel"}"#);
    assert!(
        f.out.is_empty(),
        "finished worker cannot be cancelled twice: {}",
        f.text()
    );
    f.request(r#"{"c":"searchclose"}"#);
    assert!(f.text().contains("searchclosed"));
    f.out.clear();
    f.request(r#"{"c":"searchclose"}"#);
    f.request(r#"{"c":"searchcancel"}"#);
    assert!(
        f.out.is_empty(),
        "closed search emitted another terminal receipt: {}",
        f.text()
    );
    let alias = escape(&f.root.join("raw-�.txt").to_string_lossy());
    f.request(&format!(r#"{{"c":"locate","path":"{alias}"}}"#));
    assert!(
        f.text().contains("contentsidentity"),
        "close must retain unsafe listing identity guard"
    );
}

#[test]
fn m9_g2rows_01_review_sort_and_filter_preserve_match_identity() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = raw_fixture();
    let before = f.st.contents.held.as_ref().unwrap().matches.clone();
    f.request(r#"{"c":"filter","query":"safe","id":1}"#);
    assert_eq!(f.st.listing.len(), before.len());
    for (row, hit) in before.iter().enumerate() {
        assert_eq!(f.st.listing.name(row), hit.path);
    }
    f.out.clear();
    let generation = f.st.generation;
    f.request(r#"{"c":"sort","by":"name","desc":true}"#);
    assert!(
        !f.text().contains(r#""t":"error""#),
        "valid name sort refused: {}",
        f.text()
    );
    assert!(f.st.generation > generation);
    assert_eq!(f.st.listing.name(0), "safe.txt");
    let held = f.st.contents.held.as_ref().unwrap();
    for (row, hit) in held.matches.iter().enumerate() {
        assert_eq!(row, hit.row);
        assert_eq!(f.st.listing.name(row), hit.path);
        assert_eq!(hit.operable, hit.path == "safe.txt");
    }
    let mut sorted_bytes: Vec<_> = held
        .matches
        .iter()
        .map(|hit| hit.path_bytes.clone())
        .collect();
    let mut before_bytes: Vec<_> = before.iter().map(|hit| hit.path_bytes.clone()).collect();
    sorted_bytes.sort();
    before_bytes.sort();
    assert_eq!(sorted_bytes, before_bytes);
    f.out.clear();
    f.request(r#"{"c":"window","start":1,"count":2}"#);
    assert_eq!(f.text().matches(r#""operable":false"#).count(), 2);

    let mut f = Fixture::new();
    std::fs::write(f.root.join("a.txt"), "needle\n").unwrap();
    std::fs::write(f.root.join("z.txt"), "needle longer\n").unwrap();
    f.search(false, 1, 10);
    f.finish();
    f.out.clear();
    f.request(r#"{"c":"sort","by":"size","desc":true}"#);
    assert!(
        !f.text().contains(r#""t":"error""#),
        "UTF-8 size sort refused: {}",
        f.text()
    );
    assert_eq!(f.st.listing.name(0), "z.txt");
    assert_eq!(
        f.st.contents.held.as_ref().unwrap().matches[0].line,
        "needle longer"
    );
}

#[test]
fn m9_g2rows_01_retained_filter_and_delete_receipt_keep_rows() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = raw_fixture();
    let generation = f.st.generation;
    let before = f.st.contents.held.as_ref().unwrap().matches.clone();
    f.request(r#"{"c":"filter","query":"safe","id":2}"#);
    f.event(Event::Op(
        super::super::super::opsreq::OpMsg::MenuDeleteDone {
            line: r#"{"t":"deleted","ok":1}"#.into(),
        },
    ));
    assert_eq!(f.st.generation, generation);
    assert_eq!(f.st.listing.len(), before.len());
    assert_eq!(f.st.contents.held.as_ref().unwrap().matches, before);
    assert!(f.st.search_session.is_none());
    for (row, hit) in before.iter().enumerate() {
        assert_eq!(f.st.listing.name(row), hit.path);
    }
}

#[test]
fn m9_g2rows_01_r2_close_stops_previous_names_session() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = Fixture::new();
    std::fs::write(f.root.join("one.txt"), "needle\n").unwrap();
    f.request(&format!(
        r#"{{"c":"searchopen","path":"{}","hidden":false}}"#,
        escape(&f.root.to_string_lossy())
    ));
    assert!(f.st.search_session.is_some());
    let probe = f.st.contents.probe.clone();
    probe.hold.store(true, std::sync::atomic::Ordering::Release);
    probe
        .before_spawn
        .store(true, std::sync::atomic::Ordering::Release);
    f.search(false, 2, 10);
    let until = Instant::now() + Duration::from_secs(5);
    while !probe.reached.load(std::sync::atomic::Ordering::Acquire) {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    f.out.clear();
    f.request(r#"{"c":"searchclose"}"#);
    f.until("searchclosed");
    assert!(
        f.st.search_session.is_none(),
        "close receipt left previous names session live"
    );
    assert!(!f.st.contents.busy());
    f.out.clear();
    f.event(Event::SearchWalked);
    f.event(Event::SearchRefresh);
    assert!(
        f.out.is_empty(),
        "late names events published after close: {}",
        f.text()
    );
}

#[test]
fn m9_g2rows_01_r3_failed_contents_does_not_swallow_names_cancel() {
    let mut f = Fixture::new();
    std::fs::write(f.root.join("one.txt"), "needle\n").unwrap();
    let root = f.root.to_str().unwrap();
    let probes = std::sync::Arc::new(super::super::super::searchwalk::WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_at(1);
    let mut session =
        Session::open_async_with_probes(root, root, false, f.tx.clone(), probes.clone());
    parked.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(session.query(9, "needle", 10).unwrap().walking);
    f.st.search_session = Some(session);
    f.st.contents.program = Some(f.root.join("missing-rg"));
    f.search(false, 10, 1);
    f.until("error");
    assert!(!probes.cancel.load(std::sync::atomic::Ordering::Acquire));
    f.out.clear();
    f.request(r#"{"c":"searchcancel"}"#);
    assert!(
        probes.cancel.load(std::sync::atomic::Ordering::Acquire),
        "idle contents scope swallowed real names-walk cancellation"
    );
    assert!(f.text().contains(r#""cancelled":true"#));
    assert!(f.text().contains(r#""gen":9"#));
    assert!(f.st.search_session.is_some());
    let _ = release.send(());
}

#[test]
fn m9_g2rows_01_r2_large_matching_line_uses_request_budget() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = Fixture::new();
    let line = format!("needle{}", "x".repeat(2 * 1024 * 1024));
    std::fs::write(f.root.join("bundle.js"), format!("{line}\n")).unwrap();
    std::fs::write(f.root.join("other.txt"), "needle other\n").unwrap();
    f.search(false, 1, 0);
    f.finish();
    assert_eq!(f.st.listing.len(), 2);
    assert_eq!(f.st.contents.held.as_ref().unwrap().matches[0].line, line);
}

#[test]
fn m9_g2rows_01_review_minified_line_does_not_discard_other_files() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = Fixture::new();
    let line = format!("needle{}", "x".repeat(200 * 1024));
    std::fs::write(f.root.join("bundle.js"), format!("{line}\n")).unwrap();
    std::fs::write(f.root.join("other.txt"), "needle other\n").unwrap();
    f.search(false, 1, 1);
    f.finish();
    assert_eq!(f.st.listing.len(), 2);
    assert_eq!(f.st.contents.held.as_ref().unwrap().matches[0].line, line);
}

// An unreadable folder under the root is skipped like the names walk skips it, never a failed search.
#[test]
fn ey333_unreadable_folder_keeps_the_other_matches() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let mut f = Fixture::new();
    std::fs::write(f.root.join("notes.txt"), "plain needle\n").unwrap();
    std::fs::create_dir(f.root.join("Docs")).unwrap();
    std::fs::write(f.root.join("Docs/notes.txt"), "line\nnested needle\n").unwrap();
    f.search(false, 1, 10);
    f.finish();
    let searched = |f: &Fixture| {
        let text = f.text();
        let line = text
            .lines()
            .find(|line| field_str(line, "t").as_deref() == Some("searched"));
        line.unwrap().to_owned()
    };
    let complete = searched(&f);
    let locked = f.root.join("Locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&locked).is_ok() {
        // Root bypasses the mode, so this fixture cannot deny anything here; the scripted exit 2 below covers it.
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    f.out.clear();
    f.search(false, 2, 10);
    f.finish();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(f.st.listing.len(), 2);
    assert_eq!(f.st.listing.name(0), "notes.txt");
    assert_eq!(f.st.listing.name(1), "Docs/notes.txt");
    assert!(f.text().contains(r#""lineNo":2,"line":"nested needle""#));
    let line = searched(&f);
    assert!(
        line.contains(r#""n":2"#) && line.contains(r#""gen":2"#),
        "{line}"
    );
    assert!(
        line.contains(r#""partial":true,"partial_reason":"unreadable""#),
        "{line}"
    );
    assert!(complete.contains(r#""partial":false"#), "{complete}");
}

// Exit 2 after the summary is a partial walk only when every stderr line names a skipped path or an ignore glob rg passed over.
#[test]
fn ey333_exit_two_is_partial_only_for_skipped_paths() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let summary = r#"{"type":"summary","data":{"stats":{"searches":1}}}"#;
    let denied = "rg: ./Locked: Permission denied (os error 13)\n";
    // rg prefixes only the first line of an ignore file's errors: "rg: ./.ignore: line 1: ..." then "./.ignore: line 3: ...".
    let glob = "rg: ./.ignore: line 1: error parsing glob 'a{b': unclosed alternate group; missing '}' (maybe escape '{' with '[{]'?)\n";
    for (gen, stderr, partial) in [
        (
            1,
            format!("{denied}rg: ./Gone: No such file or directory (os error 2)\n"),
            true,
        ),
        (
            2,
            format!("{glob}{}{}", &glob[4..], denied.trim_end()),
            true,
        ),
        (3, format!("{denied}rg: ./x: some other failure\n"), false),
        (4, format!("{denied}rg: unfinished (os error 13"), false),
        (5, String::new(), false),
        (
            6,
            format!("{denied}rg: No space left on device (os error 28)\n"),
            false,
        ),
        (7, format!("{denied}{}", glob.trim_end()), true),
        (8, format!("{denied}rg: ./x: line 1: other\n"), false),
        (
            9,
            format!("{denied}{}", glob.replace("line 1", "line one")),
            false,
        ),
    ] {
        let mut f = Fixture::new();
        let tool = f.root.join("exit-two-rg");
        let mut writer = std::process::Command::new("/bin/sh")
            .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
            .arg(&tool)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let lines = stderr.replace('\n', "\\n").replace('\'', "'\\''");
        let script =
            format!("#!/bin/sh\nprintf '%s\\n' '{summary}'\nprintf '{lines}' >&2\nexit 2\n");
        writer
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        assert!(writer.wait().unwrap().success());
        f.st.contents.program = Some(tool);
        f.search(false, gen, 10);
        if partial {
            f.finish();
        } else {
            f.until("error");
        }
        let text = f.text();
        assert!(
            text.contains(&format!(r#""gen":{gen}"#)),
            "case {gen}: {text}"
        );
        assert_eq!(
            text.contains(r#""t":"searched""#),
            partial,
            "case {gen}: {text}"
        );
        let skipped = text.contains(r#""partial":true,"partial_reason":"unreadable""#);
        assert_eq!(skipped, partial, "case {gen}: {text}");
    }
}

// EY-366: rg exits 0 and warns on stderr about a malformed .ignore glob; the matches still publish.
#[test]
fn ey366_malformed_ignore_glob_keeps_the_matches() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let mut f = Fixture::new();
    std::fs::write(f.root.join(".ignore"), "a{b\n").unwrap();
    std::fs::write(f.root.join("notes.txt"), "plain needle\n").unwrap();
    f.search(false, 1, 10);
    f.finish();
    assert_eq!(f.st.listing.len(), 1);
    assert_eq!(f.st.listing.name(0), "notes.txt");
    let text = f.text();
    let line = text
        .lines()
        .find(|line| field_str(line, "t").as_deref() == Some("searched"))
        .unwrap();
    assert!(line.contains(r#""partial":false"#), "{line}");
}
