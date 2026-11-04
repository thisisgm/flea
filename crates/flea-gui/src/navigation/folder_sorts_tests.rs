//! FolderSorts.js as pure logic: XDG newest folders, stored keys, the remember switch, peeks, the cap and forget.
use super::*;

fn folders(json: &str, newest: &[&str]) -> Folders {
    Folders::from_settings(
        &flea::uistate::from_file(json),
        newest.iter().map(|path| path.to_string()).collect(),
    )
}

const SIZE_DESC: Order = Order {
    key: "size",
    desc: true,
};
const NEWEST: Order = Order {
    key: "mtime",
    desc: true,
};

#[test]
fn noscene_newest_paths_read_user_dirs_and_fall_back_to_conventional_names() {
    assert_eq!(
        newest_paths("XDG_DOWNLOAD_DIR=\"$HOME/Inbox\"\n", "/home/u"),
        ["/home/u/Inbox", "/home/u/Pictures", "/home/u/Videos"],
        "the comment's sample: a configured Downloads, conventional Pictures and Videos"
    );
    assert_eq!(
        newest_paths(
            "XDG_PICTURES_DIR=\"$HOME/\"\nXDG_VIDEOS_DIR=\"/srv/media\"\n",
            "/home/u"
        ),
        ["/home/u/Downloads", "/srv/media"],
        "a role set to home itself is left out"
    );
    assert!(
        newest_paths("XDG_DOWNLOAD_DIR=\"$HOME/Inbox\"\n", "").is_empty(),
        "no home, no folders"
    );
}

#[test]
fn noscene_default_order_and_stored_keys() {
    let f = folders("{}", &["/home/u/Downloads"]);
    assert_eq!(
        f.default_order("/home/u/Downloads/"),
        Some(NEWEST),
        "a trailing slash still matches"
    );
    assert_eq!(f.default_order("/home/u/Work"), None);
    assert_eq!(
        (live_key("date"), live_key("mtime"), live_key("bogus")),
        (Some("mtime"), Some("mtime"), None)
    );
    assert_eq!(
        stored_key("mtime"),
        "date",
        "Settings stores Modified as date"
    );
}

#[test]
fn noscene_order_for_follows_saved_then_newest_then_global() {
    let on = folders(
        r#"{"sort":{"key":"size","reverse":true},"folderSorts":{"/a":{"key":"date","reverse":true},"/k":{"key":"kind","reverse":false},"/home/u/Pictures":{"key":"name","reverse":false}}}"#,
        &["/home/u/Downloads", "/home/u/Pictures"],
    );
    assert_eq!(on.order_for("/a"), NEWEST, "a saved date reads as mtime");
    assert_eq!(
        on.order_for("/home/u/Pictures"),
        Order::default(),
        "a saved order wins over the XDG newest default"
    );
    assert_eq!(
        on.order_for("/home/u/Downloads"),
        NEWEST,
        "an XDG newest folder without a saved order"
    );
    assert_eq!(
        on.order_for("/other"),
        SIZE_DESC,
        "everything else takes the global order"
    );
    assert!(on.has("/a") && !on.has("/other"));
    assert_eq!(on.sort_count(), 3);
    let off = folders(
        r#"{"rememberSort":false,"sort":{"key":"size","reverse":true},"folderSorts":{"/a":{"key":"name","reverse":true}}}"#,
        &["/home/u/Downloads"],
    );
    assert_eq!(
        off.order_for("/a"),
        SIZE_DESC,
        "remembering off ignores the saved order"
    );
    assert_eq!(
        off.order_for("/home/u/Downloads"),
        SIZE_DESC,
        "and the XDG default"
    );
    assert!(!off.has("/a"));
}

#[test]
fn noscene_peek_order_drops_kind_and_a_plain_name_order() {
    let on = folders(
        r#"{"folderSorts":{"/k":{"key":"kind","reverse":false},"/s":{"key":"size","reverse":true}}}"#,
        &["/d"],
    );
    assert_eq!(on.peek_order("/s"), Some(SIZE_DESC));
    assert_eq!(on.peek_order("/k"), None, "kind is not supported by peeks");
    assert_eq!(
        on.peek_order("/d"),
        Some(NEWEST),
        "an XDG newest folder peeks newest first"
    );
    // Each remembering-off case keeps a saved /s and a newest /d, so only the off branch can answer.
    let saved = r#""folderSorts":{"/s":{"key":"size","reverse":true}}"#;
    let off_name = folders(&format!(r#"{{"rememberSort":false,{saved}}}"#), &["/d"]);
    assert_eq!(
        (off_name.peek_order("/s"), off_name.peek_order("/d")),
        (None, None),
        "remembering off with name ascending sends no fields"
    );
    let off_date = folders(
        &format!(r#"{{"rememberSort":false,"sort":{{"key":"date","reverse":false}},{saved}}}"#),
        &["/d"],
    );
    let date_asc = Order {
        key: "mtime",
        desc: false,
    };
    assert_eq!(
        (off_date.peek_order("/s"), off_date.peek_order("/d")),
        (Some(date_asc), Some(date_asc)),
        "remembering off peeks the global order over a saved or newest one"
    );
    let off_kind = folders(
        &format!(r#"{{"rememberSort":false,"sort":{{"key":"kind","reverse":false}},{saved}}}"#),
        &["/d"],
    );
    assert_eq!(
        (off_kind.peek_order("/s"), off_kind.peek_order("/d")),
        (None, None)
    );
}

#[test]
fn noscene_remember_moves_to_the_newest_end_caps_and_forgets() {
    let mut f = folders("{}", &[]);
    for at in 0..MAX {
        f.remember(&format!("/f{at}"), SIZE_DESC).unwrap();
    }
    assert_eq!(f.sort_count(), MAX);
    f.remember("/f0", NEWEST).unwrap();
    f.remember("/new", SIZE_DESC).unwrap();
    assert_eq!(f.sort_count(), MAX, "past the cap the oldest entry goes");
    assert_eq!(
        f.get("/f0"),
        Some(NEWEST),
        "a re-sort moved its folder to the recent end, so it survives"
    );
    assert_eq!(
        f.get("/f1"),
        None,
        "the folder sorted longest ago is dropped"
    );
    assert_eq!(
        f.remember("/q\"x", NEWEST).as_deref(),
        Some(r#"{"folderSorts":{"/q\"x":{"key":"date","reverse":true}}}"#),
        "the patch stores date and escapes the path"
    );
    assert_eq!(
        f.forget("/f0").as_deref(),
        Some(r#"{"folderSorts":{"/f0":null}}"#)
    );
    assert_eq!(f.get("/f0"), None);
    assert_eq!(
        f.remember("", SIZE_DESC),
        None,
        "an empty path is never written"
    );
    let mut off = folders(r#"{"rememberSort":false}"#, &[]);
    assert_eq!(
        (off.remember("/a", SIZE_DESC), off.forget("/a")),
        (None, None),
        "remembering off writes nothing"
    );
}
