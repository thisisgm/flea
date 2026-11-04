use flea::jsondoc::parse;
use flea_gui::navigation::folder_state::{newest_paths, Folders, Order};

fn order(key: &'static str, desc: bool) -> Order {
    Order { key, desc }
}

#[test]
fn saved_orders_win_then_newest_defaults_then_the_global_order_and_remember_off_reads_only_global()
{
    let data = parse(r#"{"sort":{"key":"date","reverse":true},"folderSorts":{"/w":{"key":"size","reverse":true},"rel":{"key":"size"},"/m":{"key":"mode"}}}"#).unwrap();
    let folders = Folders::from_settings(&data, vec!["/h/Downloads".into()]);
    assert_eq!(folders.order_for("/w"), order("size", true));
    assert_eq!(folders.order_for("/h/Downloads/"), order("mtime", true));
    assert_eq!(
        folders.order_for("/m"),
        order("mtime", true),
        "an unknown key reads as the default"
    );
    assert_eq!(folders.sort_count(), 2);
    assert!(folders.has("/w") && !folders.has("/h/Downloads"));
    let off = parse(r#"{"rememberSort":false,"folderSorts":{"/w":{"key":"size","reverse":true}}}"#)
        .unwrap();
    let off = Folders::from_settings(&off, vec!["/w".into()]);
    assert_eq!(off.order_for("/w"), Order::default());
    assert_eq!(
        off.peek_order("/w"),
        None,
        "name ascending sends no peek fields"
    );
    let mut kind = Folders::from_settings(&parse("{}").unwrap(), Vec::new());
    kind.remember("/k", order("kind", false));
    assert_eq!(kind.peek_order("/k"), None, "peeks do not support kind");
}

#[test]
fn a_resort_moves_its_folder_to_the_recent_end_and_the_cap_drops_the_oldest() {
    let mut folders = Folders::from_settings(&parse("{}").unwrap(), Vec::new());
    for at in 0..501 {
        folders.remember(&format!("/d{at}"), order("size", false));
    }
    assert_eq!(folders.sort_count(), 500);
    assert_eq!(folders.get("/d0"), None);
    folders.remember("/d1", order("name", true));
    for at in 2..501 {
        folders.remember(&format!("/n{at}"), order("size", false));
    }
    assert_eq!(
        folders.get("/d1"),
        Some(order("name", true)),
        "re-sorted folder survived as most recent"
    );
    let mut off = Folders::from_settings(&parse(r#"{"rememberSort":false}"#).unwrap(), Vec::new());
    assert_eq!(
        off.remember("/a", order("size", false)),
        None,
        "remembering off writes nothing"
    );
}

#[test]
fn folder_views_are_opt_in_and_hidden_is_a_saved_toggle() {
    let data = parse(
        r#"{"rememberViewPerFolder":true,"folderViews":{"/a":"grid","/b":"dual"},"hidden":true}"#,
    )
    .unwrap();
    let mut folders = Folders::from_settings(&data, Vec::new());
    assert_eq!(folders.view_for("/a", "columns"), "grid");
    assert_eq!(
        folders.view_for("/b", "columns"),
        "columns",
        "dual is a window shape, not a folder view"
    );
    assert_eq!(folders.view_for("/c", "bogus"), "list");
    assert_eq!(
        folders.remember_view("/c", "list").as_deref(),
        Some(r#"{"folderViews":{"/c":"list"}}"#)
    );
    assert!(folders.hidden);
    let off = Folders::from_settings(
        &parse(r#"{"folderViews":{"/a":"grid"}}"#).unwrap(),
        Vec::new(),
    );
    assert_eq!(off.view_for("/a", "columns"), "columns");
}

#[test]
fn user_dirs_name_the_newest_first_folders_with_conventional_fallbacks() {
    let body = "# comment\nXDG_DOWNLOAD_DIR=\"$HOME/Old\"\nXDG_DOWNLOAD_DIR=\"$HOME/In\\\"box/\"\nXDG_VIDEOS_DIR=\"$HOME\"\nXDG_DESKTOP_DIR=\"relative\"\n";
    assert_eq!(newest_paths(body, "/h"), ["/h/In\"box", "/h/Pictures"]);
    assert_eq!(
        newest_paths("", "/h"),
        ["/h/Downloads", "/h/Pictures", "/h/Videos"]
    );
    assert!(newest_paths("", "").is_empty());
}
