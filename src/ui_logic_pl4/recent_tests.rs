// Ported from tests/js/recent.js and the date half of tests/js/recentdates.js.
use super::recent::*;
use super::today::*;

fn paths(marks: &[(&str, &str)]) -> Vec<String> {
    entries(marks.iter().copied())
        .into_iter()
        .map(|v| v.path)
        .collect()
}

#[test]
fn the_history_lives_in_the_data_home() {
    assert_eq!(
        history_path("/run/user/1000/data", "/home/gm"),
        "/run/user/1000/data/recently-used.xbel"
    );
    assert_eq!(
        history_path("", "/home/gm"),
        "/home/gm/.local/share/recently-used.xbel",
        "an unset data home falls back"
    );
    assert_eq!(
        history_path("relative/share", "/home/gm"),
        "/home/gm/.local/share/recently-used.xbel",
        "a relative data home is none"
    );
    assert_eq!(
        history_path("/home/gm/.local/share/", "/home/gm"),
        "/home/gm/.local/share/recently-used.xbel",
        "no doubled slash"
    );
}

#[test]
fn every_bookmark_is_untrusted_until_it_reads_as_a_local_file() {
    assert_eq!(
        path_of("file:///home/gm/a.png").as_deref(),
        Some("/home/gm/a.png")
    );
    assert_eq!(
        path_of("file:///home/gm/a%20b%23c.png").as_deref(),
        Some("/home/gm/a b#c.png")
    );
    assert_eq!(
        path_of("file:///home/gm/caf%C3%A9.txt").as_deref(),
        Some("/home/gm/caf\u{e9}.txt")
    );
    assert_eq!(
        path_of("file://localhost/etc/hostname").as_deref(),
        Some("/etc/hostname")
    );
    assert_eq!(
        path_of("FILE:///etc/hostname").as_deref(),
        Some("/etc/hostname"),
        "the scheme is case-insensitive"
    );
    for refused in [
        "file://nas.local/share/x.txt",
        "smb://nas/share/x.txt",
        "sftp://host/home/gm/x.txt",
        "trash:///x.txt",
        "/home/gm/a.png",
        "rm -rf /home/gm",
        "",
        "file://",
        "file:///",
        "file:///home/gm/a%00b",
        "file:///home/gm/a%0Ab",
        "file:///home/gm/%zz",
        "file:///home/gm/%C3",
    ] {
        assert_eq!(path_of(refused), None, "{refused:?} is refused");
    }
}

#[test]
fn entries_list_newest_first_once_each_up_to_the_cap() {
    let read = paths(&[
        ("file:///home/gm/old.txt", "2026-08-01T09:00:00Z"),
        ("file:///home/gm/new.png", "2026-08-30T11:32:04Z"),
        ("smb://nas/share/skip.txt", "2026-08-31T11:32:04Z"),
        ("file:///home/gm/mid.md", "2026-08-15T10:00:00Z"),
    ]);
    assert_eq!(
        read,
        ["/home/gm/new.png", "/home/gm/mid.md", "/home/gm/old.txt"],
        "only local files, newest first"
    );
    let twice = paths(&[
        ("file:///home/gm/a.png", "2026-08-30T11:32:04Z"),
        ("file:///home/gm/a.png", "2026-08-01T09:00:00Z"),
    ]);
    assert_eq!(
        twice,
        ["/home/gm/a.png"],
        "a file listed twice keeps one row"
    );
    let tied = paths(&[
        ("file:///home/gm/first.txt", "2026-08-30T11:32:04Z"),
        ("file:///home/gm/second.txt", "2026-08-30T11:32:04Z"),
    ]);
    assert_eq!(
        tied,
        ["/home/gm/first.txt", "/home/gm/second.txt"],
        "a tie keeps the file's own order"
    );
    let stampless = paths(&[
        ("file:///home/gm/none.txt", ""),
        ("file:///home/gm/dated.txt", "2026-08-30T11:32:04Z"),
    ]);
    assert_eq!(
        stampless,
        ["/home/gm/dated.txt", "/home/gm/none.txt"],
        "a stampless bookmark lists last"
    );
    assert!(paths(&[]).is_empty(), "an empty history is an empty rail");
    assert!(
        paths(&[("smb://nas/x", "2026-08-30T11:32:04Z")]).is_empty(),
        "remote bookmarks only list nothing"
    );
    let many: Vec<(String, String)> = (0..LIMIT + 50)
        .map(|i| {
            (
                format!("file:///home/gm/f{i}.txt"),
                "2026-08-30T11:32:04Z".into(),
            )
        })
        .collect();
    assert_eq!(
        entries(many.iter().map(|(h, s)| (h.as_str(), s.as_str()))).len(),
        LIMIT,
        "an enormous history stops at the cap"
    );
    let oldest_first: Vec<(String, String)> = (0..LIMIT + 50)
        .map(|k| {
            (
                format!("file:///home/gm/g{k}.txt"),
                format!("2026-08-30T11:{}:{:02}Z", 32 + k / 60, k % 60),
            )
        })
        .collect();
    let newest: Vec<String> = entries(oldest_first.iter().map(|(h, s)| (h.as_str(), s.as_str())))
        .into_iter()
        .map(|v| v.path)
        .collect();
    assert_eq!(newest.len(), LIMIT);
    assert_eq!(
        newest[0], "/home/gm/g549.txt",
        "the top row is the newest bookmark in the file"
    );
    assert_eq!(
        newest[LIMIT - 1],
        "/home/gm/g50.txt",
        "the last kept is the oldest of the newest LIMIT"
    );
    let kept = entries([
        ("file:///home/gm/a.png", "2026-08-01T09:00:00Z"),
        ("file:///home/gm/a.png", "2026-08-30T11:32:04Z"),
    ]);
    assert_eq!(
        kept[0].stamp, "2026-08-30T11:32:04Z",
        "a file seen twice keeps its newest stamp"
    );
    assert!(entries([
        ("", "2026-08-30T11:32:04Z"),
        ("not a uri", "2026-08-30T11:32:04Z")
    ])
    .is_empty());
}

#[test]
fn names_and_locations_read_off_the_path() {
    assert_eq!(name_of("/home/gm/a.txt"), "a.txt");
    assert_eq!(location_of("/home/gm/a.txt"), "/home/gm");
    assert_eq!(
        name_of("home/gm/a.txt"),
        "a.txt",
        "a listpaths row names the same leaf"
    );
    assert_eq!(
        location_of("home/gm/a.txt"),
        "/home/gm",
        "and the same absolute parent"
    );
    assert_eq!(location_of("a.txt"), "/");
    assert_eq!(location_of("/a.txt"), "/");
    let gm = "/home/gm";
    assert_eq!(
        location_under("/home/gm/Documents/claude/a.md", gm),
        "Documents/claude"
    );
    assert_eq!(
        location_under("/home/gm/Downloads/receipt.pdf", gm),
        "Downloads"
    );
    assert_eq!(
        location_under("/home/gm/a.txt", gm),
        "~",
        "a file directly in home reads as home"
    );
    assert_eq!(location_under("/srv/data/a.txt", gm), "/srv/data");
    assert_eq!(
        location_under("/home/gmx/a.txt", gm),
        "/home/gmx",
        "a sibling named like home is outside it"
    );
    assert_eq!(location_under("/a.txt", gm), "/");
    assert_eq!(
        location_under("/home/gm/Downloads/a.txt", ""),
        "/home/gm/Downloads",
        "no home keeps the absolute parent"
    );
    assert_eq!(location_under("", gm), "");
}

#[test]
fn visits_read_the_stamp_and_askers_join_once() {
    assert_eq!(stamp_seconds("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(stamp_seconds("2026-08-30T11:32:04Z"), Some(1_788_089_524));
    assert_eq!(
        stamp_seconds("2026-08-30T11:32:04.123456Z"),
        Some(1_788_089_524),
        "a fraction is dropped"
    );
    assert_eq!(
        stamp_seconds("2026-08-30T13:32:04+02:00"),
        Some(1_788_089_524),
        "an offset is applied"
    );
    assert_eq!(stamp_seconds(""), None);
    assert_eq!(stamp_seconds("2026-13-30T11:32:04Z"), None);
    let v = visits(&entries([
        ("file:///a", "2026-08-30T11:32:04Z"),
        ("file:///b", ""),
    ]));
    assert_eq!((v["/a"], v["/b"]), (Some(1_788_089_524), None));
    let askers = join_requesters(&[], Some("paneA"));
    let askers = join_requesters(&askers, Some("paneB"));
    assert_eq!(
        askers,
        [Some("paneA"), Some("paneB")],
        "two panes both wait, in order"
    );
    assert_eq!(
        join_requesters(&askers, Some("paneA")).len(),
        2,
        "a repeat asks nothing twice"
    );
    let rail = join_requesters::<&str>(&[], None);
    assert_eq!(rail, [None], "the rail pane itself joins as none");
    assert_eq!(
        join_requesters(&rail, None).len(),
        1,
        "and a repeat joins once"
    );
    assert_eq!(
        join_requesters(&rail, Some("paneA")).len(),
        2,
        "a pane joins beside the rail pane"
    );
}

#[test]
fn today_is_local_midnight_and_one_compare() {
    let noon = day_start(1_790_164_800_000) + 12 * 3_600_000;
    let start = day_start(noon);
    assert_eq!(
        day_start(start + 86_399_000),
        start,
        "the start is the same all day"
    );
    assert!(start % 1000 == 0 && start <= noon && noon - start < 86_400_000);
    assert!(
        !is_recent(Some((start / 1000 - 1) as f64), start),
        "the second before midnight is yesterday"
    );
    assert!(
        is_recent(Some((start / 1000) as f64), start),
        "midnight is today"
    );
    assert!(
        is_recent(Some((noon / 1000 + 86_400) as f64), start),
        "a future mtime counts as recent"
    );
    assert!(!is_recent(None, start), "a missing mtime is never recent");
    assert!(!is_recent(Some(0.0), start), "the epoch is not recent");
    assert!(
        !is_recent(Some(f64::NAN), start),
        "a non-number is never recent"
    );
    assert_eq!(
        ms_until_midnight(start),
        day_start(start + 30 * 3_600_000) - start,
        "midnight arms until the next day starts"
    );
    let next = day_start(start + 30 * 3_600_000);
    assert_eq!(
        ms_until_midnight(next - 1000),
        1000,
        "one second to midnight arms one second"
    );
    assert_eq!(
        day_start(next + 60_000),
        next,
        "a minute past midnight the start is the new day"
    );
    assert_eq!(MINUTE_MS, 60_000);
}

#[test]
fn the_history_file_reads_each_bookmark_and_its_visit() {
    let xbel = r#"<?xml version="1.0"?><xbel version="1.0">
<bookmark href="file:///home/gm/a%20b.txt" added="2026-08-01T09:00:00Z" modified="2026-08-02T09:00:00Z" visited="2026-08-30T11:32:04Z"><info/></bookmark>
<bookmark href='file:///home/gm/R&amp;D.md' added="2026-08-01T09:00:00Z" modified="2026-08-15T10:00:00Z"/>
<bookmark   href="file:///home/gm/x&#233;.txt" visited=""/>
<bookmarks href="file:///not/a/bookmark"/>
</xbel>"#;
    let read = bookmarks(xbel);
    assert_eq!(
        read[0],
        (
            "file:///home/gm/a%20b.txt".into(),
            "2026-08-30T11:32:04Z".into()
        ),
        "visited leads"
    );
    assert_eq!(
        read[1],
        (
            "file:///home/gm/R&D.md".into(),
            "2026-08-15T10:00:00Z".into()
        ),
        "modified stands in, entities decode"
    );
    assert_eq!(
        read[2],
        ("file:///home/gm/x\u{e9}.txt".into(), String::new()),
        "an empty stamp lists last"
    );
    assert_eq!(read.len(), 3, "a lookalike element is skipped");
    let rows = entries(read.iter().map(|(h, s)| (h.as_str(), s.as_str())));
    assert_eq!(rows[0].path, "/home/gm/a b.txt");
}
