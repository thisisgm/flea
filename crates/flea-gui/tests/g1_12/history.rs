//! The jump's window-side sources: the recent history as recent.js reads it and the ui state's favourites.
use super::js;
use super::HOME;
use flea_gui::path::history::{self, favourites, history_path, path_of, LIMIT};

#[test]
fn the_recent_history_reads_as_recent_js_reads_it() {
    js(
        "recent.js",
        8,
        history_path(Some("/x/data"), HOME),
        "/x/data/recently-used.xbel".into(),
    );
    js(
        "recent.js",
        10,
        history_path(None, HOME),
        "/home/gm/.local/share/recently-used.xbel".into(),
    );
    js(
        "recent.js",
        12,
        history_path(Some("data"), HOME),
        "/home/gm/.local/share/recently-used.xbel".into(),
    );
    js(
        "recent.js",
        14,
        history_path(Some("/x/data/"), HOME),
        "/x/data/recently-used.xbel".into(),
    );
    // Recent.js re-adds the slash after trimming, so a data home of "/" stays absolute.
    assert_eq!(
        history_path(Some("/"), HOME),
        std::path::PathBuf::from("/recently-used.xbel")
    );
    assert_eq!(
        history_path(Some("//"), HOME),
        std::path::PathBuf::from("/recently-used.xbel")
    );
    let local = [
        ("file:///home/gm/a.png", "/home/gm/a.png"),
        ("file:///home/gm/a%20b%23c.png", "/home/gm/a b#c.png"),
        ("file:///home/gm/caf%C3%A9.txt", "/home/gm/café.txt"),
        ("file://localhost/etc/hostname", "/etc/hostname"),
        ("FILE:///etc/hostname", "/etc/hostname"),
        ("file:///home/gm/a%C2%85b", "/home/gm/a\u{85}b"),
    ];
    for (href, want) in local {
        js("recent.js", 18, path_of(href), Some(want.into()));
    }
    for href in [
        "file://nas.local/share/x.txt",
        "smb://nas/share/x.txt",
        "trash:///x.txt",
        "/home/gm/a.png",
        "",
        "file://",
        "file:///",
    ] {
        js("recent.js", 24, path_of(href), None);
    }
    for href in [
        "file:///home/gm/a%00b",
        "file:///home/gm/a%0Ab",
        "file:///home/gm/a%7Fb",
        "file:///home/gm/%zz",
    ] {
        js("recent.js", 33, path_of(href), None);
    }
    let mark = |href: &str, stamp: &str| {
        format!("<bookmark href=\"{href}\" added=\"2026-01-01T00:00:00Z\" visited=\"{stamp}\">\n  <info/>\n</bookmark>")
    };
    let file = [
        mark("file:///home/gm/old.txt", "2026-08-01T09:00:00Z"),
        mark("smb://nas/x", "2026-09-01T09:00:00Z"),
        mark("file:///home/gm/new.png", "2026-08-30T11:32:04Z"),
        mark("file:///home/gm/mid.md", "2026-08-15T09:00:00Z"),
        mark("file:///home/gm/new.png", "2026-07-01T09:00:00Z"),
        "<bookmark href='file:///home/gm/q&amp;a.txt'/>".into(),
    ];
    let text = format!(
        "<?xml version=\"1.0\"?>\n<xbel version=\"1.0\">\n{}\n</xbel>\n",
        file.join("\n")
    );
    js(
        "recent.js",
        44,
        history::read(&text),
        vec![
            "/home/gm/new.png".into(),
            "/home/gm/mid.md".into(),
            "/home/gm/old.txt".into(),
            "/home/gm/q&a.txt".into(),
        ],
    );
    let tied = [
        mark("file:///home/gm/first.txt", "2026-08-01T00:00:00Z"),
        mark("file:///home/gm/second.txt", "2026-08-01T00:00:00Z"),
    ];
    js(
        "recent.js",
        61,
        history::read(&tied.join("")),
        vec!["/home/gm/first.txt".into(), "/home/gm/second.txt".into()],
    );
    let many: String = (0..550)
        .map(|at| {
            mark(
                &format!("file:///home/gm/g{at}.txt"),
                &format!("2026-08-01T00:{:02}:{:02}Z", at / 60, at % 60),
            )
        })
        .collect();
    let newest = history::read(&many);
    js("recent.js", 92, newest.len(), LIMIT);
    js(
        "recent.js",
        93,
        (newest[0].as_str(), newest[LIMIT - 1].as_str()),
        ("/home/gm/g549.txt", "/home/gm/g50.txt"),
    );
    js("recent.js", 72, history::read("").len(), 0);
}

#[test]
fn the_favourites_are_the_places_paths_the_ui_state_keeps() {
    let state = flea::jsondoc::parse(r#"{"places":{"favourites":[{"label":"P","path":"/fx/j/Projects"},{"label":"x"},{"path":"rel"},{"path":"/fx/j/flea"}]}}"#).unwrap();
    assert_eq!(favourites(&state), ["/fx/j/Projects", "/fx/j/flea"]);
    assert!(favourites(&flea::jsondoc::parse("{}").unwrap()).is_empty());
}
