use super::user_dirs;
use crate::ui_logic_pl4::places::{favorites, home_entries};

#[test]
fn only_standard_xdg_role_names_use_catalog_keys() {
    let dirs = concat!(
        "XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\n",
        "XDG_DOCUMENTS_DIR=\"$HOME/Office records\"\n",
        "XDG_MUSIC_DIR=\"$HOME/Documents\"\n",
        "XDG_UNKNOWN_DIR=\"$HOME/Desktop\"\n",
        "XDG_DESKTOP_DIR=\"$HOME\"\n",
    );
    let rows = user_dirs(dirs, "/fixture");
    assert_eq!(rows.len(), 4, "a disabled desktop adds no place");
    assert_eq!(rows[0].label_key, "places.row.downloads");
    for row in &rows[1..] {
        assert_eq!(row.label_key, "", "custom names are real names");
    }
    let home = home_entries("/fixture", dirs, |_| "folder");
    assert_eq!(home[1].label_key, "places.row.downloads");
    assert_eq!(home[2].label, "Office records");
    let merged = favorites(
        "/fixture",
        dirs,
        "file:///saved/Downloads Downloads\nfile:///saved/Desktop Desktop",
        |_| "folder",
    );
    assert_eq!(merged[1].label_key, "places.row.downloads");
    for row in &merged[2..] {
        assert_eq!(
            row.label_key, "",
            "bookmark names never become product copy"
        );
    }
}

#[test]
fn every_xdg_user_dirs_default_role_uses_a_catalog_key() {
    // xdg-user-dirs 0.20's user-dirs.defaults: DESKTOP to VIDEOS, then PROJECTS.
    let dirs = concat!(
        "XDG_DESKTOP_DIR=\"$HOME/Desktop\"\n",
        "XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\n",
        "XDG_TEMPLATES_DIR=\"$HOME/Templates\"\n",
        "XDG_PUBLICSHARE_DIR=\"$HOME/Public\"\n",
        "XDG_DOCUMENTS_DIR=\"$HOME/Documents\"\n",
        "XDG_MUSIC_DIR=\"$HOME/Music\"\n",
        "XDG_PICTURES_DIR=\"$HOME/Pictures\"\n",
        "XDG_VIDEOS_DIR=\"$HOME/Videos\"\n",
        "XDG_PROJECTS_DIR=\"$HOME/Projects\"\n",
        "XDG_UNKNOWN_DIR=\"$HOME/Projects 2\"\n",
    );
    let keys: Vec<&str> = user_dirs(dirs, "/fixture")
        .iter()
        .map(|row| row.label_key)
        .collect();
    assert_eq!(
        keys,
        [
            "places.row.desktop",
            "places.row.downloads",
            "places.row.templates",
            "places.row.public",
            "places.row.documents",
            "places.row.music",
            "places.row.pictures",
            "places.row.videos",
            "places.row.projects",
            "",
        ]
    );
    let renamed = user_dirs("XDG_PROJECTS_DIR=\"$HOME/Work\"\n", "/fixture");
    assert_eq!(
        (renamed[0].label.as_str(), renamed[0].label_key),
        ("Work", ""),
        "a renamed Projects folder keeps its name"
    );
}
