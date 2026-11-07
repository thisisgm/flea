use crate::backend::{events, iomount, mime::Db, testdir::TestDir};
use std::sync::Arc;

#[test]
fn tree_outline_5000_children_uses_one_bounded_first_window() {
    let dir = TestDir::new("tree-outline-5000");
    std::fs::create_dir(dir.join("A")).unwrap();
    for n in 0..5000 {
        std::fs::write(dir.join(&format!("A/f{n}.txt")), "x").unwrap();
    }
    let base = dir.join("");
    let line = format!(
        r#"{{"c":"list","outlinePaths":["{}"]}}"#,
        crate::json::escape(&base.join("A").to_string_lossy())
    );
    let (tx, _) = events::channel();
    let done = iomount::list_dir(
        base.to_string_lossy().into_owned(),
        false,
        32,
        line,
        Arc::new(Db::load()),
        -1,
        tx,
        &iomount::mount_body(),
    )
    .unwrap();
    assert_eq!(done.listing.len(), 5001);
    assert_eq!(
        done.first_metas.len(),
        32,
        "only the requested window builds row facts"
    );
    assert_eq!(done.listing.name(1), "A/f0.txt");
    assert_eq!(done.listing.name(5000), "A/f4999.txt");
}

fn listed(dir: &TestDir, paths: &[&str], extra: &str) -> iomount::ListOut {
    let base = dir.path();
    let paths = paths
        .iter()
        .map(|name| {
            format!(
                "\"{}\"",
                crate::json::escape(&base.join(name).to_string_lossy())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let line = format!("{{\"c\":\"list\",\"outlinePaths\":[{paths}]{extra}}}");
    let (tx, _) = events::channel();
    iomount::list_dir(
        base.to_string_lossy().into_owned(),
        false,
        32,
        line,
        Arc::new(Db::load()),
        -1,
        tx,
        &iomount::mount_body(),
    )
    .unwrap()
}

#[test]
fn tree_outline_nested_siblings_sort_and_rows_keep_their_real_paths() {
    let dir = TestDir::new("tree-outline-nested");
    dir.dir("A");
    dir.dir("A/full");
    dir.dir("A/empty");
    dir.file("A/z.txt", "z");
    dir.file("A/a.txt", "a");
    dir.file("A/full/f10.txt", "10");
    dir.file("A/full/f2.txt", "2");
    dir.file("end.txt", "end");
    let mut done = listed(&dir, &["A", "A/full"], "");
    let names = |l: &crate::backend::listing::Listing| {
        (0..l.len())
            .map(|i| l.name(i).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&done.listing),
        [
            "A",
            "A/empty",
            "A/full",
            "A/full/f2.txt",
            "A/full/f10.txt",
            "A/a.txt",
            "A/z.txt",
            "end.txt"
        ]
    );
    let resolved =
        crate::backend::opsdispatch::resolve_rows(Vec::new(), &[3, 7], dir.path(), &done.listing);
    assert_eq!(
        resolved,
        [
            dir.join("A/full/f2.txt").to_string_lossy(),
            dir.join("end.txt").to_string_lossy()
        ]
    );
    crate::backend::ordering::request(
        &mut done.listing,
        dir.path(),
        &Db::load(),
        r#"{"c":"sort","by":"name","desc":true}"#,
    )
    .unwrap();
    assert_eq!(
        names(&done.listing),
        [
            "A",
            "A/full",
            "A/full/f10.txt",
            "A/full/f2.txt",
            "A/empty",
            "A/z.txt",
            "A/a.txt",
            "end.txt"
        ]
    );
    assert!(crate::backend::outline::row_fields(&done.listing, 2).contains("\"depth\":2"));
    assert!(crate::backend::outline::watched(
        &done.listing,
        dir.path(),
        &dir.join("A/full")
    ));
    assert!(!crate::backend::outline::watched(
        &done.listing,
        dir.path(),
        &dir.join("A/full/closed")
    ));
}

#[test]
fn tree_outline_one_request_opens_every_named_folder_and_keeps_the_anchor_row() {
    use crate::backend::{dirsizeworker::Worker, outline, proto::{parse_request, Request}, state::State};
    let dir = TestDir::new("tree-outline-many");
    dir.dir("A");
    dir.dir("B");
    dir.file("c.txt", "c");
    let (tx, _) = events::channel();
    let mut state = State::new(Worker::new(tx));
    state.base = dir.path().to_owned();
    state.listing = listed(&dir, &[], "").listing;
    let base = dir.path().to_string_lossy().into_owned();
    state.outline.last = format!(r#"{{"c":"list","path":"{}"}}"#, crate::json::escape(&base));
    let line = r#"{"c":"outline","rows":[0,1],"open":true,"anchor":2,"listing":1,"first":32}"#;
    let Request::Outline { rows, anchor, open, .. } = parse_request(line) else { panic!("not an outline request") };
    assert_eq!((rows.as_slice(), anchor, open), (&[0, 1][..], Some(2), true));
    let opened = outline::request(&mut state, &rows, anchor, open, None, 32).unwrap();
    assert!(state.outline.open.contains(&dir.join("A")) && state.outline.open.contains(&dir.join("B")));
    assert_eq!(crate::json::field_str(&opened, "select"), Some(dir.join("c.txt").to_string_lossy().into_owned()));
    outline::request(&mut state, &[0, 1], None, false, None, 32).unwrap();
    assert!(state.outline.open.is_empty(), "one close request closes every named folder");
    assert_eq!(
        outline::request(&mut state, &[2, 9], None, true, None, 32),
        Err("row 2 is not a folder in this listing".into())
    );
    assert!(state.outline.open.is_empty(), "a request naming no folder opens nothing");
    outline::request(&mut state, &[2, 0, 9], None, true, None, 32).unwrap();
    assert_eq!(state.outline.open.len(), 1, "a marked file or a row past the listing is skipped");
    assert!(state.outline.open.contains(&dir.join("A")));
    outline::request(&mut state, &[2, 9], Some(1), true, None, 32).unwrap();
    assert!(state.outline.open.contains(&dir.join("B")), "with no folder named, the anchor's folder opens");
    state.outline.open.remove(&dir.join("B"));
    // With A open, closing it from a cursor on A's child selects A, the row the child folds into.
    dir.file("A/in.txt", "i");
    state.listing = listed(&dir, &["A"], "").listing;
    let child = (0..state.listing.len()).find(|&row| state.listing.name(row) == "A/in.txt").expect("A/in.txt is listed under A");
    let closed = outline::request(&mut state, &[0], Some(child), false, None, 32).unwrap();
    assert_eq!(crate::json::field_str(&closed, "select"), Some(dir.join("A").to_string_lossy().into_owned()));
    let kept = outline::request(&mut state, &[0], Some(child), true, None, 32).unwrap();
    assert_eq!(crate::json::field_str(&kept, "select"), Some(dir.join("A/in.txt").to_string_lossy().into_owned()));
    let Request::Outline { rows, .. } = parse_request(r#"{"c":"outline","row":1,"open":true}"#) else { panic!("not an outline request") };
    assert_eq!(rows, [1], "a lone row still names one folder");
    std::os::unix::fs::symlink(dir.join("B"), dir.join("L")).unwrap();
    state.listing = listed(&dir, &[], "").listing;
    let link = (0..state.listing.len()).find(|&row| state.listing.name(row) == "L").expect("L is listed");
    assert_eq!(outline::request(&mut state, &[link], None, true, None, 32), Err(format!("row {link} is not a folder in this listing")));
    assert!(!state.outline.open.contains(&dir.join("L")), "a symlinked folder never opens in place");
}

#[test]
fn tree_outline_session_retains_nested_expansions_and_rejects_stale_rows() {
    use crate::backend::{dirsizeworker::Worker, outline, rowguard, state::State};
    let dir = TestDir::new("tree-outline-session");
    dir.dir("A");
    dir.dir("A/full");
    dir.file("A/full/f.txt", "f");
    let (tx, _) = events::channel();
    let mut state = State::new(Worker::new(tx));
    state.base = dir.path().to_owned();
    state.listing = listed(&dir, &[], "").listing;
    state.outline.last = format!(
        "{{\"c\":\"list\",\"path\":\"{}\",\"by\":\"name\"}}",
        crate::json::escape(&dir.path().to_string_lossy())
    );
    let first = outline::request(&mut state, &[0], None, true, None, 32).unwrap();
    let first = state.outline.list(&first);
    assert!(first.contains("outlinePaths"));
    state.listing = listed(&dir, &["A"], "").listing;
    outline::request(&mut state, &[1], None, true, None, 32).unwrap();
    outline::request(&mut state, &[0], None, false, None, 32).unwrap();
    assert!(!state.outline.open.contains(&dir.join("A")), "a collapse closes the folder");
    assert!(state.outline.open.contains(&dir.join("A/full")));
    outline::request(&mut state, &[0], None, true, None, 32).unwrap();
    outline::request(&mut state, &[], None, false, Some(false), 32).unwrap();
    let last = state.outline.last.clone();
    assert!(crate::json::field_str_array(&state.outline.list(&last), "outlinePaths").is_empty());
    outline::request(&mut state, &[], None, false, Some(true), 32).unwrap();
    assert!(state.outline.list(&last).contains("A/full"));
    assert!(rowguard::refusal(r#"{"c":"outline","row":0,"listing":1}"#, 2).is_some());
    assert!(rowguard::refusal(r#"{"c":"outline","rows":[0,1],"open":true,"anchor":0,"listing":1}"#, 2).is_some());
}

#[test]
fn tree_outline_children_follow_hidden_filter_and_symlinks_never_expand() {
    let dir = TestDir::new("tree-outline-hidden");
    dir.dir("A");
    dir.file("A/.hidden", "hidden");
    dir.file("A/visible", "visible");
    std::os::unix::fs::symlink(dir.join("A"), dir.join("link")).unwrap();
    let done = listed(&dir, &["A", "link"], "");
    assert_eq!(done.listing.len(), 3);
    assert_eq!(done.listing.name(1), "A/visible");
    assert!(!done.listing.outline_open.contains("link"));
}

#[test]
fn tree_outline_unreadable_child_stays_closed_and_the_listing_still_lands() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TestDir::new("tree-outline-locked");
    dir.dir("A");
    dir.file("A/inside", "x");
    dir.dir("B");
    dir.file("B/visible", "x");
    let locked = dir.join("A");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable = std::fs::read_dir(&locked).is_ok();
    let done = listed(&dir, &["A", "B"], "");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    if readable {
        return;
    }
    let names: Vec<_> = (0..done.listing.len()).map(|i| done.listing.name(i).to_owned()).collect();
    assert_eq!(names, ["A", "B", "B/visible"]);
    assert!(!done.listing.outline_open.contains("A"));
    assert!(done.listing.outline_open.contains("B"));
}

#[test]
fn tree_outline_watch_notices_a_child_and_collapse_keeps_the_root_watch() {
    use crate::backend::{events::Event, watch::Watch};
    let dir = TestDir::new("tree-outline-watch");
    let child = dir.dir("A");
    let (tx, rx) = events::channel();
    let mut watch = Watch::start(tx);
    watch.begin(dir.path());
    watch.commit(Vec::new());
    let wd = Watch::add_raw(watch.raw_fd(), &child);
    assert!(wd >= 0);
    watch.begin(dir.path());
    watch.commit(vec![wd]);
    dir.file("A/new.txt", "new");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut changed = false;
    while std::time::Instant::now() < deadline {
        if let Ok(Event::Changed(seen)) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
            if seen == wd {
                changed = true;
                break;
            }
        }
    }
    assert!(changed && watch.is_current(wd));
    watch.begin(dir.path());
    watch.commit(Vec::new());
    assert!(!watch.is_current(wd));
    dir.file("root.txt", "root");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut root_changed = false;
    while std::time::Instant::now() < deadline {
        if let Ok(Event::Changed(wd)) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
            if watch.is_current(wd) {
                root_changed = true;
                break;
            }
        }
    }
    assert!(root_changed);
    watch.stop();
}

#[test]
fn tree_outline_remote_child_uses_the_existing_scan_worker_even_under_a_local_root() {
    use crate::backend::listwork::threaded;
    let body = "1 0 8:1 / / rw - ext4 /dev/a rw\n2 1 0:45 / /files/NAS rw - nfs server:/share rw\n";
    assert!(!threaded(
        std::path::Path::new("/files"),
        r#"{"outlinePaths":["/files/local"]}"#,
        body
    ));
    assert!(threaded(
        std::path::Path::new("/files"),
        r#"{"outlinePaths":["/files/NAS"]}"#,
        body
    ));
    assert!(!threaded(
        std::path::Path::new("/files-elsewhere"),
        r#"{"outlinePaths":["/files/NAS"]}"#,
        body
    ));
}

#[test]
fn tree_outline_refused_child_watch_is_detected_and_the_folder_still_opens() {
    use std::os::fd::AsRawFd;
    let dir = TestDir::new("tree-outline-refused");
    dir.dir("A");
    dir.file("A/inside", "x");
    let plain = std::fs::File::open(dir.join("A/inside")).unwrap();
    let base = dir.path();
    let line = format!(
        r#"{{"c":"list","outlinePaths":["{}"]}}"#,
        crate::json::escape(&base.join("A").to_string_lossy())
    );
    let (tx, _) = events::channel();
    // A descriptor that is not inotify refuses every watch, as a box out of inotify watches does.
    let done = iomount::list_dir(
        base.to_string_lossy().into_owned(),
        false,
        32,
        line,
        Arc::new(Db::load()),
        plain.as_raw_fd(),
        tx,
        &iomount::mount_body(),
    )
    .unwrap();
    assert_eq!(done.listing.outline_watches, [-1]);
    assert!(done.listing.outline_open.contains("A"));
    assert_eq!(done.listing.name(1), "A/inside");
    assert!(crate::backend::outline::watch_refused(plain.as_raw_fd(), -1));
    assert!(!crate::backend::outline::watch_refused(-1, -1));
    assert!(!crate::backend::outline::watch_refused(plain.as_raw_fd(), 3));
}

#[test]
fn tree_outline_size_sort_seeds_each_folder_size_at_its_outline_row() {
    let dir = TestDir::new("tree-outline-sized");
    dir.dir("A");
    dir.dir("A/B");
    dir.file("A/B/big", &"x".repeat(65536));
    dir.file("A/own", &"x".repeat(32768));
    dir.dir("C");
    dir.file("C/small", "x");
    let mut listing = listed(&dir, &["A"], "").listing;
    let line = r#"{"c":"sort","by":"size","desc":true}"#;
    let (_, _, sized) = crate::backend::ordering::request(&mut listing, dir.path(), &Db::load(), line).unwrap();
    assert_eq!(sized.len(), listing.len(), "one seed per row of the outline numbering");
    let row = |name: &str| (0..listing.len()).find(|&r| listing.name(r) == name).unwrap();
    let size = |name: &str| sized[row(name)].unwrap().bytes;
    assert!(size("A") > size("A/B") && size("A/B") > size("C"), "each folder's own walked size sits at its own row");
}

#[test]
fn tree_outline_requests_reach_the_list_parser_and_refusals_name_their_cause() {
    use crate::backend::{dirsizeworker::Worker, outline, proto::{parse_request, Request}, state::State};
    let dir = TestDir::new("tree-outline-wire");
    dir.dir("A \"quoted\"");
    dir.file("f.txt", "f");
    let (tx, _) = events::channel();
    let mut state = State::new(Worker::new(tx));
    state.base = dir.path().to_owned();
    state.listing = listed(&dir, &[], "").listing;
    let base = dir.path().to_string_lossy().into_owned();
    state.outline.last = format!(r#"{{"c":"list","path":"{}"}}"#, crate::json::escape(&base));
    let folder = dir.join("A \"quoted\"").to_string_lossy().into_owned();
    let opened = outline::request(&mut state, &[0], None, true, None, 32).unwrap();
    let line = state.outline.list(&opened);
    let Request::List { path, first, .. } = parse_request(&line) else { panic!("not a list request: {line}") };
    assert_eq!((path.as_str(), first), (base.as_str(), 32));
    assert_eq!(crate::json::field_str_array(&line, "outlinePaths"), std::slice::from_ref(&folder));
    assert_eq!(crate::json::field_str(&line, "select"), Some(folder));
    let toggled = outline::request(&mut state, &[], None, false, Some(false), 32).unwrap();
    assert_eq!(crate::json::field_str(&toggled, "select"), None, "a view toggle carries no select from the last listing");
    assert_eq!(outline::request(&mut state, &[1], None, true, None, 32), Err("row 1 is not a folder in this listing".into()));
    assert_eq!(outline::request(&mut state, &[], None, false, None, 32), Err("an outline request names a row or enabled".into()));
}

#[test]
fn tree_outline_a_link_inside_an_open_folder_stays_a_link_through_a_sort() {
    let dir = TestDir::new("tree-outline-link");
    dir.dir("A");
    dir.dir("A/target");
    dir.file("A/plain", "x");
    std::os::unix::fs::symlink(dir.join("A/plain"), dir.join("A/link")).unwrap();
    let mut listing = listed(&dir, &["A"], "").listing;
    let line = r#"{"c":"sort","by":"name","desc":true}"#;
    crate::backend::ordering::request(&mut listing, dir.path(), &Db::load(), line).unwrap();
    let names: Vec<&str> = (0..listing.len()).map(|row| listing.name(row)).collect();
    let mut every = names.clone();
    every.sort();
    assert_eq!((names[0], every), ("A", vec!["A", "A/link", "A/plain", "A/target"]), "rows keep their full names under their parent");
    let link = names.iter().position(|&name| name == "A/link").unwrap();
    assert!(listing.spans[link].is_symlink, "the copied row keeps the link bit through expand and the group sort");
    assert!(!listing.spans[names.iter().position(|&name| name == "A/plain").unwrap()].is_symlink);
}
