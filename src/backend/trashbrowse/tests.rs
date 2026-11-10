use super::*;
// gio 2.90.1's gio list shape: the line feed line as minipc printed it, then a tab, a backslash and a two-byte UTF-8 name.
#[test]
fn an_escaped_original_reads_back_as_its_own_bytes() {
    let text = "trash:///a%0Ab.txt\t4\t(regular)\ttrash::orig-path=/tmp/a\\x0ab.txt\ntrash:///c\t4\t(regular)\ttrash::orig-path=/tmp/a\\x09b \\x5cx41.txt\ntrash:///e\t4\t(regular)\ttrash::orig-path=/tmp/caf\\xc3\\xa9.txt\n";
    let originals: Vec<_> = parse_list(text)
        .into_iter()
        .map(|item| item.original)
        .collect();
    assert_eq!(
        originals,
        ["/tmp/a\nb.txt", "/tmp/a\tb \\x41.txt", "/tmp/caf\u{e9}.txt"]
    );
    assert_eq!(
        unescape("/tmp/good one.txt").as_deref(),
        Some("/tmp/good one.txt")
    );
    for broken in [
        "/tmp/a\\x0",
        "/tmp/a\\",
        "/tmp/a\\y41",
        "/tmp/a\\xzz",
        "/tmp/a\\x00b",
        // A NUL would let a record spell the exact tail of another name, here /tmp/x<0xff>.
        "/tmp/x\\xef\\xbf\\xbd\\x0078ff",
    ] {
        assert_eq!(unescape(broken), None, "{broken}");
    }
}
// gio 2.90.1 on minipc printed Path=/tmp/caf%E9 %5Cx41.txt as this line; a folder that is not UTF-8 is carried the same way.
#[test]
fn an_original_that_is_not_utf8_lists_as_its_exact_text_and_names_its_raw_bytes() {
    let text = "trash:///bad.txt\t4\t(regular)\ttrash::orig-path=/tmp/caf\\xe9 \\x5cx41.txt\ntrash:///d\t4\t(regular)\ttrash::orig-path=/tmp/d\\xff/a.txt\n";
    let items = parse_list(text);
    let originals: Vec<_> = items.iter().map(|item| item.original.as_str()).collect();
    assert_eq!(
        originals,
        [
            "/tmp/caf\u{fffd} \\x41.txt\u{0}636166e9205c7834312e747874",
            "/tmp/d\u{fffd}\u{0}64ff/a.txt"
        ]
    );
    let raw: Vec<_> = items
        .iter()
        .map(|item| {
            flea::bytename::exact(&item.original)
                .into_os_string()
                .into_encoded_bytes()
        })
        .collect();
    assert_eq!(
        raw,
        [
            b"/tmp/caf\xe9 \\x41.txt".to_vec(),
            b"/tmp/d\xff/a.txt".to_vec()
        ]
    );
    assert_eq!(
        flea::bytename::display(&items[0].original),
        "/tmp/caf\u{fffd} \\x41.txt"
    );
}
#[test]
fn listing_is_newest_deleted_first_and_ties_break_by_uri() {
    let item = |name: &str| Item { uri: format!("trash:///{}", name), original: format!("/home/u/{}", name) };
    let mut items = vec![item("b.txt"), item("a.txt"), item("c.txt")];
    let date = |item: &Item| match item.uri.as_str() {
        "trash:///a.txt" => "2026-10-05T09:00:00".to_string(),
        _ => "2026-10-02T10:00:00".to_string(),
    };
    newest_first(&mut items, date);
    let order: Vec<_> = items.iter().map(|item| item.uri.as_str()).collect();
    assert_eq!(order, ["trash:///a.txt", "trash:///b.txt", "trash:///c.txt"]);
    // A listing in the other uri order lands on the same sequence, so two reads of one Trash agree.
    let mut reversed = vec![item("c.txt"), item("b.txt"), item("a.txt")];
    newest_first(&mut reversed, date);
    assert_eq!(reversed, items);
}
#[test]
fn deletion_dates_come_from_one_gio_list() {
    let text = "trash:///a%20b.txt\t0\t(regular)\ttrash::deletion-date=2026-09-08T10:00:00\ntrash:///c\t4096\t(directory)\ntrash:///d\t1\t(regular)\ttrash::deletion-date=2026-09-09T11:30:00\n";
    let dates = parse_dates(text);
    assert_eq!(dates.get("trash:///a%20b.txt").map(String::as_str), Some("2026-09-08T10:00:00"));
    assert_eq!(dates.get("trash:///d").map(String::as_str), Some("2026-09-09T11:30:00"));
    assert_eq!(dates.get("trash:///c"), None);
    assert!(parse_dates("").is_empty());
}
#[test]
fn an_item_without_a_date_sorts_last() {
    let item = |name: &str| Item { uri: format!("trash:///{}", name), original: format!("/home/u/{}", name) };
    let mut items = vec![item("a"), item("b"), item("c")];
    newest_first(&mut items, |item| if item.uri.ends_with('b') { "2026-10-01T00:00:00".into() } else { String::new() });
    let order: Vec<_> = items.iter().map(|item| item.uri.as_str()).collect();
    assert_eq!(order, ["trash:///b", "trash:///a", "trash:///c"]);
}
// gio's two outputs for one Trash, one escaped name in both: the listing in uri order, the dates by uri.
const TRASH_LIST: &str = "trash:///a%20b.txt\t0\t(regular)\ttrash::orig-path=/home/u/a b.txt\ntrash:///c.txt\t0\t(regular)\ttrash::orig-path=/home/u/c.txt\ntrash:///z.txt\t0\t(regular)\ttrash::orig-path=/home/u/z.txt\n";
const TRASH_DATES: &str = "trash:///a%20b.txt\t0\t(regular)\ttrash::deletion-date=2026-10-05T09:00:00\ntrash:///c.txt\t0\t(regular)\ttrash::deletion-date=2026-10-01T09:00:00\ntrash:///z.txt\t0\t(regular)\ttrash::deletion-date=2026-10-03T09:00:00\n";
fn read_gio(dates: Result<&'static str, &'static str>) -> impl Fn(&[&str]) -> Result<String, Refusal> {
    // Both calls list dot names too, so a trashed .hidden is listed and dated.
    move |args: &[&str]| match args {
        ["list", "-u", "-h", "-a", "trash::orig-path,standard::target-uri", "trash:///"] => {
            Ok(TRASH_LIST.to_string())
        }
        ["list", "-u", "-h", "-a", "trash::deletion-date", "trash:///"] => {
            dates.map(String::from).map_err(unreadable)
        }
        _ => panic!("unexpected gio call {args:?}"),
    }
}
fn uris(items: &[Item]) -> Vec<&str> {
    items.iter().map(|item| item.uri.as_str()).collect()
}
#[test]
fn the_real_listing_joins_dates_to_uris_newest_first() {
    let items = list_with(read_gio(Ok(TRASH_DATES))).unwrap();
    assert_eq!(uris(&items), ["trash:///a%20b.txt", "trash:///z.txt", "trash:///c.txt"]);
    assert_eq!(items[0].original, "/home/u/a b.txt");
}
#[test]
fn a_failed_dates_call_keeps_the_uri_order() {
    let items = list_with(read_gio(Err("gio failed"))).unwrap();
    assert_eq!(uris(&items), ["trash:///a%20b.txt", "trash:///c.txt", "trash:///z.txt"]);
}
#[test]
fn a_failed_trash_list_is_an_error() {
    assert!(list_with(|_: &[&str]| Err(unreadable("gio failed"))).is_err());
}
#[test]
fn two_reads_of_one_trash_are_unchanged_whatever_their_order() {
    let dated = list_with(read_gio(Ok(TRASH_DATES))).unwrap();
    let by_uri = list_with(read_gio(Err("timed out"))).unwrap();
    assert_ne!(dated, by_uri);
    assert!(same_items(&dated, &by_uri));
    assert!(same_items(&by_uri, &dated));
    // A different item or a different original is a change.
    let mut other = by_uri.clone();
    other[1].original = "/home/u/elsewhere".into();
    assert!(!same_items(&dated, &other));
    assert!(!same_items(&dated, &by_uri[1..]));
}
#[test]
fn membership_finds_every_item_of_a_date_ordered_listing() {
    let items = list_with(read_gio(Ok(TRASH_DATES))).unwrap();
    assert_ne!(uris(&items), { let mut sorted = uris(&items); sorted.sort(); sorted });
    let listed = listed_uris(&items);
    for item in &items { assert!(listed.contains(item.uri.as_str()), "{} missed", item.uri); }
    assert!(!listed.contains("trash:///absent"));
}
#[test]
fn selected_targets_require_known_paths_and_original_identities() {
    let d = crate::backend::testdir::TestDir::new("trash-selected-targets");
    let session = Session {
        items: listed("trash:///a\t/tmp/a\n"),
        scratch: d.path().to_path_buf(),
        ..Session::default()
    };
    for request in [r#"{"uris":["/tmp/a"],"identities":["saved"]}"#,
        r#"{"uris":["trash:///a"],"identities":[]}"#,
        r#"{"uris":["trash:///a"],"identities":[""]}"#,
        r#"{"selectionToken":1}"#] {
        assert!(session.targets(request).is_err());
    }
    let targets = session.targets(r#"{"uris":["trash:///a","trash:///a"],"identities":["saved","saved"]}"#).unwrap();
    let mut offset = 0;
    assert!(targets.records().next(&mut offset).unwrap().is_some());
    assert!(targets.records().next(&mut offset).unwrap().is_none());
}
#[test]
fn bulk_targets_keep_the_saved_set_and_identity_when_listing_changes() {
    let d = crate::backend::testdir::TestDir::new("trash-bulk-targets");
    let mut selected = Manifest::new(d.path()).unwrap();
    selected.append(br#"{"uri":"trash:///a","original":"/original/a","identity":"original-inode"}"#).unwrap();
    let session = Session {
        items: listed("trash:///a\t/replacement/a\ntrash:///new\t/new\n"),
        selection: Some(Selection { token: 7, items: selected }),
        scratch: d.path().to_path_buf(),
        ..Session::default()
    };
    let targets = session.targets(r#"{"selectionToken":7}"#).unwrap();
    let mut offset = 0;
    let record = record_text(targets.records().next(&mut offset).unwrap().unwrap()).unwrap();
    assert_eq!(field_str(&record, "identity").unwrap(), "original-inode");
    assert_eq!(record_item(&record).unwrap().original, "/original/a");
    assert!(targets.records().next(&mut offset).unwrap().is_none());
    assert!(session.targets(r#"{"selectionToken":7,"exclude":["trash:///a"]}"#).is_err());
}
#[test]
fn refreshed_confirmation_keeps_its_uri_set_without_rewriting_selection() {
    let d = crate::backend::testdir::TestDir::new("trash-confirm-refresh");
    let mut targets = Manifest::new(d.path()).unwrap();
    targets.append(br#"{"uri":"trash:///a","original":"/old/a","identity":"old-inode"}"#).unwrap();
    targets.append(br#"{"uri":"trash:///gone","original":"/old/gone","identity":"gone-inode"}"#).unwrap();
    let confirmation = Confirmation { token: 3, items: Manifest::new(d.path()).unwrap(),
        trees: Manifest::new(d.path()).unwrap(), listing: Vec::new(), targets, count: 0, nodes: 0, all: false, sweep: Default::default() };
    let session = Session { items: listed("trash:///a\t/new/a\ntrash:///new\t/new\n"),
        scratch: d.path().to_path_buf(), ..Session::default() };
    let refreshed = session.refreshed_targets(&confirmation).unwrap();
    let mut offset = 0;
    let text = record_text(refreshed.records().next(&mut offset).unwrap().unwrap()).unwrap();
    assert_eq!(record_item(&text).unwrap().original, "/new/a");
    assert_eq!(field_str(&text, "identity").unwrap(), "");
    assert!(refreshed.records().next(&mut offset).unwrap().is_none());
    let mut offset = 0;
    let held = record_text(confirmation.targets.records().next(&mut offset).unwrap().unwrap()).unwrap();
    assert_eq!(field_str(&held, "identity").unwrap(), "old-inode");
}
#[test]
fn named_failure_keeps_its_retry_identity() {
    let item = Item { uri: "trash:///a".into(), original: "/original/a".into() };
    let message = failure(&item, "held-identity", "Permission denied");
    assert_eq!(field_str(&message, "identity").unwrap(), "held-identity");
    assert_eq!(field_str(&message, "name").unwrap(), "a");
    assert_eq!(field_str(&message, "error").unwrap(), "Permission denied");
}
#[test]
fn expired_confirmation_never_reaches_provider() {
    assert!(!Session::default().valid(1).unwrap());
}
// The flag comes from the typed failure, so a sentence that reads the same on another op never sets it.
#[test]
fn a_stale_delete_sets_the_flag_and_says_confirm_again_with_its_detail_logged() {
    let seen = crate::backend::trashmanifest::logged().len();
    let stale = Session::default().handle(r#"{"c":"trashbrowse","op":"delete","id":4,"token":9}"#);
    assert!(stale.contains(r#""ok":false,"stale":true,"error":"The Trash changed, so nothing was done; confirm again.""#), "{}", stale);
    assert!(crate::backend::trashmanifest::logged().split_off(seen).iter().any(|line| line.contains("Trash changed; confirm again.")));
    let unknown = Session::default().handle(r#"{"c":"trashbrowse","op":"nothing","id":4}"#);
    assert!(unknown.contains(r#""stale":false,"error":"Unknown Trash request.""#), "{}", unknown);
}
#[test]
fn a_review_record_that_does_not_read_back_says_the_trash_changed_and_logs_which() {
    let seen = crate::backend::trashmanifest::logged().len();
    let refused = record_item(r#"{"original":"/o"}"#).unwrap_err();
    assert_eq!(refused.shown(), "The Trash changed, so nothing was done. Try again.");
    assert!(crate::backend::trashmanifest::logged().split_off(seen).iter().any(|line| line.contains("Missing reviewed Trash URI.")));
    assert_eq!(String::from(record_text(vec![0xff]).unwrap_err()), "The Trash changed, so nothing was done. Try again.");
}
#[test]
fn attributes_require_exact_names() {
    let text = "  standard::size: 12\n  id::file: abc\n";
    assert_eq!(attribute(text, "standard::size"), Some("12"));
    assert_eq!(attribute(text, "standard::siz"), None);
}
#[test]
fn restore_selection_rejects_a_reused_uri_or_changed_metadata() {
    let original = Detail {
        item: Item { uri: "trash:///same.txt".into(), original: "/original/same.txt".into() },
        size: 8, deleted: "2026-09-08T10:00:00".into(), identity: "filesystem:inode-one".into(),
        directory: false, icon: "text-plain".into(), backing: None,
    };
    let observed = selection_identity(&original);
    assert!(require_selected_identity(&original, &observed).is_ok());
    assert!(require_selected_identity(&original, "").is_err());
    let mut replacement = original.clone();
    replacement.identity = "filesystem:inode-two".into();
    assert!(require_selected_identity(&replacement, &observed).is_err());
    replacement = original.clone();
    replacement.item.original = "/other/same.txt".into();
    assert!(require_selected_identity(&replacement, &observed).is_err());
    replacement = original.clone();
    replacement.size += 1;
    assert!(require_selected_identity(&replacement, &observed).is_err());
}
#[test]
fn restore_selection_rejects_a_replaced_backing_inode() {
    use crate::backend::testdir::TestDir;
    use std::os::unix::fs::MetadataExt;
    let d = TestDir::new("trash-restore-identity");
    d.dir("files");
    d.dir("info");
    let path = d.file("files/same.txt", "original");
    d.file("info/same.txt.trashinfo", "[Trash Info]\nPath=/original/same.txt\n");
    let backing = || {
        let meta = path.symlink_metadata().unwrap();
        Reviewed::inspect(path.clone(), &format!("l{}:{}", meta.dev(), meta.ino())).unwrap()
    };
    let mut item = Detail {
        item: Item { uri: "trash:///same.txt".into(), original: "/original/same.txt".into() },
        size: 8, deleted: "2026-09-08T10:00:00".into(), identity: "provider-identity".into(),
        directory: false, icon: "text-plain".into(), backing: Some(backing()),
    };
    let observed = selection_identity(&item);
    std::fs::rename(&path, d.join("files/old.txt")).unwrap();
    std::fs::write(&path, "replaced").unwrap();
    item.backing = Some(backing());
    assert!(require_selected_identity(&item, &observed).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "replaced");
}

// Sample body: root ext4 beside one hung nfs mount, the same shape the slow tests map a sandbox onto.
fn stub_body(dir: &std::path::Path) -> String {
    format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n", dir.to_string_lossy())
}

// One sweep over restore and delete: each waits on a mount held slow and runs elsewhere.
#[test]
fn restore_and_delete_wait_on_a_pending_mount_and_run_elsewhere() {
    let remote = crate::backend::testdir::TestDir::new("trashgate");
    let body = stub_body(remote.path());
    let listing = format!("trash:///a.txt\t{}/a.txt\n", remote.path().to_string_lossy());
    let line = r#"{"c":"trashbrowse","op":"restore","id":9,"uris":["trash:///a.txt"],"identities":["held"]}"#;
    // A restore whose original sits on the held mount waits before gio runs.
    let mut held = Session {
        items: listed(&listing),
        scratch: remote.path().to_path_buf(),
        pending: vec![remote.path().to_path_buf()],
        body: body.clone(),
        ..Session::default()
    };
    let busy = held.handle(line);
    assert!(busy.contains(r#""ok":false"#) && busy.contains("already running"), "a restore onto a pending mount waits: {}", busy);
    // The same restore elsewhere runs past the gate to gio's own answer.
    let mut free = Session {
        items: listed(&listing),
        scratch: remote.path().to_path_buf(),
        ..Session::default()
    };
    let past = free.handle(line);
    assert!(!past.contains("already running"), "a restore elsewhere runs past the gate: {}", past);
    // A delete whose original sits on the held mount waits, and keeps its confirmation for the retry.
    let mut targets = Manifest::new(remote.path()).unwrap();
    targets.append(format!(r#"{{"uri":"trash:///a.txt","original":"{}/a.txt","identity":""}}"#,
        remote.path().to_string_lossy()).as_bytes()).unwrap();
    let confirmation = Confirmation { token: 7, items: Manifest::new(remote.path()).unwrap(),
        trees: Manifest::new(remote.path()).unwrap(), listing: Vec::new(), targets, count: 0, nodes: 0, all: false, sweep: Default::default() };
    let delete = r#"{"c":"trashbrowse","op":"delete","id":9,"token":7}"#;
    let mut held = Session { confirmation: Some(confirmation), scratch: remote.path().to_path_buf(),
        pending: vec![remote.path().to_path_buf()], body, ..Session::default() };
    let busy = held.handle(delete);
    assert!(busy.contains(r#""ok":false"#) && busy.contains("already running"), "a delete onto a pending mount waits: {}", busy);
    assert!(held.confirmation.is_some(), "the refused delete keeps its confirmation for the retry");
    // The same delete elsewhere runs past the gate to the listing's own answer.
    let mut targets = Manifest::new(remote.path()).unwrap();
    targets.append(format!(r#"{{"uri":"trash:///a.txt","original":"{}/a.txt","identity":""}}"#,
        remote.path().to_string_lossy()).as_bytes()).unwrap();
    let confirmation = Confirmation { token: 7, items: Manifest::new(remote.path()).unwrap(),
        trees: Manifest::new(remote.path()).unwrap(), listing: Vec::new(), targets, count: 0, nodes: 0, all: false, sweep: Default::default() };
    let mut free = Session { confirmation: Some(confirmation), scratch: remote.path().to_path_buf(), ..Session::default() };
    let past = free.handle(delete);
    assert!(!past.contains("already running"), "a delete elsewhere runs past the gate: {}", past);
}
