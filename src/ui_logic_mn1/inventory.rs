//! The one row order every menu reads: F file, B background, T Trash, P Places, R rail rows (never hideable).
//! The HIG file menu ends with Move to Trash and Delete Permanently in their own destructive group.

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    // The stored id Settings > Menus hides by.
    pub id: &'static str,
    pub label: &'static str,
    pub glyph: &'static str,
    pub kinds: &'static str,
    pub group: &'static str,
    // The verb a chosen row fires; the stored id when it is the same.
    pub action: &'static str,
}

const fn spec(
    id: &'static str,
    label: &'static str,
    glyph: &'static str,
    kinds: &'static str,
    group: &'static str,
) -> Spec {
    Spec {
        id,
        label,
        glyph,
        kinds,
        group,
        action: id,
    }
}

const fn acting(
    id: &'static str,
    label: &'static str,
    glyph: &'static str,
    kinds: &'static str,
    group: &'static str,
    action: &'static str,
) -> Spec {
    Spec {
        id,
        label,
        glyph,
        kinds,
        group,
        action,
    }
}

pub const INVENTORY: &[Spec] = &[
    spec("open", "menu.row.open", "folder-open", "FTPR", "open"),
    // The search results' own way to the file, the reveal o and Enter make, for hands on the mouse.
    spec("goToLocation", "menu.row.go_to_location", "folder", "FTPR", "open"),
    acting(
        "showOriginal",
        "menu.row.show_original",
        "symlink",
        "F",
        "open",
        "showOriginal",
    ),
    spec("openTab", "menu.row.open_tab", "plus", "P", "open"),
    acting(
        "openwith",
        "menu.row.open_with",
        "app-window",
        "F",
        "open",
        "openWith",
    ),
    // Print joins the first group after Open With; its dialog and job belong to the print unit.
    spec("print", "menu.row.print", "printer", "F", "open"),
    spec("run", "menu.row.run", "play", "F", "open"),
    spec(
        "openAsText",
        "menu.row.open_as_text",
        "file-text",
        "F",
        "open",
    ),
    // Make Executable sits with Open, as v0.3.12's Run Script did (FINDER-BEHAVIOR, CTL2 2026-10-31).
    acting(
        "makeExecutable",
        "menu.row.make_executable",
        "play",
        "F",
        "open",
        "makeExecutable",
    ),
    spec("unlockVolume", "menu.row.unlock", "lock", "R", "open"),
    spec("mountVolume", "menu.row.mount", "drive", "R", "open"),
    spec("mountPhone", "menu.row.mount", "drive", "R", "open"),
    spec(
        "unmountVolume",
        "menu.row.unmount",
        "drive",
        "R",
        "rrelease",
    ),
    spec("unmountPhone", "menu.row.unmount", "drive", "R", "rrelease"),
    spec("unmount", "menu.row.unmount", "drive", "R", "rrelease"),
    spec("eject", "menu.row.eject", "eject", "R", "rrelease"),
    spec("flashImage", "menu.row.flash_image", "file", "R", "rflash"),
    spec(
        "newFolder",
        "menu.row.new_folder",
        "folder-plus",
        "B",
        "open",
    ),
    spec("newFile", "menu.row.new_file", "file-plus", "B", "open"),
    spec("cut", "menu.row.cut", "scissors", "F", "basic"),
    spec("copy", "menu.row.copy", "copy", "F", "basic"),
    spec("paste", "menu.row.paste", "clipboard", "FB", "basic"),
    acting(
        "pasteAs",
        "menu.row.paste_as",
        "symlink",
        "FB",
        "basic",
        "pasteAs",
    ),
    spec("duplicate", "menu.row.duplicate", "file-plus", "F", "basic"),
    spec("rename", "menu.row.rename", "rename", "FR", "basic"),
    spec("editPlace", "menu.row.edit_place", "sliders", "R", "basic"),
    spec("remove", "menu.row.remove_place", "minus", "R", "rremove"),
    spec("selectAll", "menu.row.select_all", "check", "B", "basic"),
    acting(
        "invertSelection",
        "menu.row.invert_selection",
        "contrast",
        "B",
        "basic",
        "invertSelection",
    ),
    spec("compress", "menu.row.compress", "archive", "F", "archive"),
    spec("extract", "menu.row.extract", "archive-out", "F", "archive"),
    spec("convert", "menu.row.convert", "sliders", "F", "archive"),
    // Combine Images sits under Convert with no key; its card belongs to the convert unit.
    spec(
        "combineImages",
        "menu.row.combine_images",
        "images",
        "F",
        "archive",
    ),
    spec(
        "flashDrive",
        "menu.row.flash_drive",
        "drive",
        "F",
        "archive",
    ),
    acting(
        "shelf",
        "menu.row.add_to_shelf",
        "shelf",
        "F",
        "share",
        "addToShelf",
    ),
    spec("taildrop", "menu.row.taildrop", "tailscale", "F", "share"),
    spec("airdrop", "menu.row.airdrop", "airdrop", "F", "share"),
    spec("localsend", "menu.row.localsend", "localsend", "F", "share"),
    spec(
        "sendRclone",
        "send.rclone.row",
        "cloud-upload",
        "F",
        "share",
    ),
    spec("dropbox", "menu.row.dropbox", "dropbox", "F", "share"),
    spec("sharelink", "menu.row.share_link", "network", "F", "share"),
    spec(
        "openTerminal",
        "menu.row.open_terminal",
        "terminal",
        "FBP",
        "inspect",
    ),
    acting(
        "moveto",
        "menu.row.move_to",
        "folder-plus",
        "F",
        "inspect",
        "moveTo",
    ),
    acting(
        "copyto",
        "menu.row.copy_to",
        "copy",
        "F",
        "inspect",
        "copyTo",
    ),
    spec("properties", "menu.row.properties", "info", "F", "inspect"),
    spec(
        "permissions",
        "menu.row.permissions",
        "lock",
        "F",
        "inspect",
    ),
    acting(
        "copyAs",
        "menu.row.copy_as",
        "file-text",
        "FP",
        "inspect",
        "copyAs",
    ),
    spec(
        "runScript",
        "menu.row.run_script",
        "terminal",
        "F",
        "inspect",
    ),
    spec("compare", "menu.row.compare", "columns", "F", "inspect"),
    spec(
        "addFavourite",
        "menu.row.add_favourite",
        "star",
        "FBP",
        "inspect",
    ),
    spec("tags", "menu.row.tags", "tag", "F", "inspect"),
    spec(
        "removeFavourite",
        "menu.row.remove_favourite",
        "minus",
        "PR",
        "inspect",
    ),
    spec("sort", "menu.row.sort", "sort", "B", "view"),
    spec("toggleHidden", "menu.row.show_hidden", "eye", "FB", "view"),
    spec(
        "toggleLangMarks",
        "menu.row.show_code_icons",
        "code",
        "FB",
        "view",
    ),
    // The HIG destructive group is last for files, folders and multi-selection; only Delete Permanently shares it.
    spec("trash", "menu.row.trash", "trash", "F", "trash"),
    acting(
        "delete",
        "menu.row.delete",
        "trash",
        "F",
        "trash",
        "deletePermanently",
    ),
    spec(
        "extThumbs",
        "menu.row.show_thumbnails",
        "image",
        "B",
        "view",
    ),
    spec("settings", "menu.row.settings", "sliders", "B", "settings"),
    spec(
        "updateFlea",
        "menu.row.update_flea",
        "download",
        "B",
        "settings",
    ),
    spec("restoreAll", "menu.row.restore_all", "undo", "T", "restore"),
    spec("emptyTrash", "menu.row.empty_trash", "trash", "T", "empty"),
];

// The ids Settings > Menus ships switched off, as src/uischema.rs writes; HIG keeps Actions on and Run Script off.
pub const SHIPPED_HIDDEN: &[&str] = &[
    "delete",
    "openTerminal",
    "placeMenu",
    "runScript",
    "moveto",
    "copyto",
    "copyAs",
    "pasteAs",
    "invertSelection",
    "extThumbs",
    "run",
    "openAsText",
    "openAgent",
    "toggleHidden",
    "toggleLangMarks",
];

/// The set a menu hides: the stored list, or the shipped one when none is stored, with Add to Shelf under the shelf's own switch.
pub fn effective_hidden(stored: Option<&[String]>, shelf_enabled: bool) -> Vec<String> {
    let base = stored.map_or_else(
        || SHIPPED_HIDDEN.iter().map(|id| (*id).to_owned()).collect(),
        <[String]>::to_vec,
    );
    let mut out: Vec<String> = base.into_iter().filter(|id| id != "shelf").collect();
    if !shelf_enabled {
        out.push("shelf".to_owned());
    }
    out
}

pub fn spec_of(id: &str) -> Option<&'static Spec> {
    INVENTORY.iter().find(|spec| spec.id == id)
}
