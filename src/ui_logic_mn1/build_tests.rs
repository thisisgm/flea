//! tests/js/menu.js inventory cases, with the 0.4.0 board order, title case and hidden-not-dimmed rows.
use super::build::{listing_entries, rail_entries, trash_entries, Context, RailEntry, S_IFDIR};
use super::inventory::{effective_hidden, spec_of, INVENTORY, SHIPPED_HIDDEN};
use super::rows::is_hidden;
use super::testkit::*;
use std::collections::HashSet;

fn st(change: impl FnOnce(&mut Context)) -> Context {
    let mut p = state();
    change(&mut p);
    p
}

#[test]
fn sol_menu_order_inventory_order_and_default_menus() {
    let i18n = english();
    let file = listing_entries(&state());
    // 0.4.0 adds Print, Combine Images and Send with rclone to the 57 rows v0.3.12 shipped; 0.4.1 adds Send with AirDrop.
    check(
        "Menus and Places inventory has 61 actions",
        INVENTORY.len(),
        61,
    );
    check(
        "inventory storage ids are unique",
        INVENTORY.iter().map(|s| s.id).collect::<HashSet<_>>().len(),
        61,
    );
    check("default image menu matches Menus specimen", actions(&file),
        "open,openWith,run,openAsText,cut,copy,paste,duplicate,rename,compress,convert,flashDrive,addToShelf,taildrop,dropbox,tags,toggleHidden,toggleLangMarks,trash".into());
    check(
        "code icons ship enabled with the existing code glyph",
        format!(
            "{}|{}",
            label(&i18n, entry(&file, "toggleLangMarks")),
            entry(&file, "toggleLangMarks").unwrap().glyph.unwrap()
        ),
        "Hide Code Icons|code".into(),
    );
    check(
        "code icons flip their verb live",
        label(
            &i18n,
            entry(
                &listing_entries(&st(|p| p.lang_marks = false)),
                "toggleLangMarks",
            ),
        ),
        "Show Code Icons".into(),
    );
    check(
        "Settings can hide the code-icons row",
        entry(
            &listing_entries(&st(|p| p.hidden = strings(&["toggleLangMarks"]))),
            "toggleLangMarks",
        )
        .is_none(),
        true,
    );
    let at = |a: &str| file.iter().position(|r| r.action == a).unwrap();
    check(
        "the code-icons row follows Show hidden files",
        at("toggleLangMarks"),
        at("toggleHidden") + 1,
    );
    check(
        "empty clipboard keeps Paste disabled",
        entry(&file, "paste").unwrap().disabled,
        true,
    );
    check(
        "populated clipboard enables Paste",
        entry(
            &listing_entries(&st(|p| p.clipboard_available = true)),
            "paste",
        )
        .unwrap()
        .disabled,
        false,
    );
    let folder = listing_entries(&st(|p| {
        p.row_mode = 0o040755;
        p.row_is_image = false;
        p.row_name = "Photos".into()
    }));
    check("folder omits run and open as text; Pane gives a folder no image or archive flag, so no conversion or extraction", actions(&folder),
        "open,openWith,cut,copy,paste,duplicate,rename,compress,addToShelf,taildrop,dropbox,addFavourite,tags,toggleHidden,toggleLangMarks,trash".into());
    check("background menu includes real creation actions in order", actions(&listing_entries(&st(|p| p.has_row = false))),
        "newFolder,newFile,paste,selectAll,openTerminal,addFavourite,sort,toggleHidden,toggleLangMarks,settings".into());
    // The HIG file-menu board closes with Move to Trash alone in the destructive group.
    let last_group: Vec<_> = file
        .split(|r| r.separator)
        .next_back()
        .unwrap()
        .iter()
        .map(|r| r.action.to_string())
        .collect();
    check(
        "Move to Trash is last, alone in the destructive group",
        last_group,
        strings(&["trash"]),
    );
    let all = listing_entries(&st(|p| {
        p.hidden.clear();
        p.clipboard_available = true;
        p.row_is_symlink = true;
        p.has_shebang = true;
        p.cursor_is_target = true
    }));
    let tail: Vec<_> = all
        .split(|r| r.separator)
        .next_back()
        .unwrap()
        .iter()
        .map(|r| r.action.to_string())
        .collect();
    check(
        "Move to Trash and Delete Permanently share the final group",
        tail,
        strings(&["trash", "deletePermanently"]),
    );
}

#[test]
fn update_and_favourite_rows() {
    let i18n = english();
    let background = listing_entries(&st(|p| {
        p.has_row = false;
        p.update_version = "0.3.4".into()
    }));
    let tail: Vec<_> = background[background.len() - 2..]
        .iter()
        .map(|r| format!("{}:{}", r.action, r.glyph.unwrap_or("")))
        .collect();
    check("a known newer build adds Update Flea under Settings, in the same group, with the download mark", tail.join(","), "settings:sliders,updateFlea:download".into());
    let update = entry(&background, "updateFlea").unwrap();
    check(
        "and its hint slot carries that version beside the status square",
        format!("{}|{}", hint(&i18n, Some(update)), update.hint_square),
        "0.3.4|true".into(),
    );
    check(
        "the Menus switch takes it away like any other row",
        entry(
            &listing_entries(&st(|p| {
                p.has_row = false;
                p.update_version = "0.3.4".into();
                p.hidden = strings(&["updateFlea"])
            })),
            "updateFlea",
        )
        .is_none(),
        true,
    );
    check(
        "a file row's menu never offers it",
        entry(
            &listing_entries(&st(|p| p.update_version = "0.3.4".into())),
            "updateFlea",
        )
        .is_none(),
        true,
    );
    check(
        "and neither does an empty version, which is what every state but available gives",
        entry(&listing_entries(&st(|p| p.has_row = false)), "updateFlea").is_none(),
        true,
    );
    // 0.4.0 hides Add to Favorites on a file instead of dimming it, since only a folder can be a favourite.
    check(
        "selected file cannot be pinned as a folder",
        entry(&listing_entries(&state()), "addFavourite").is_none(),
        true,
    );
    let dir = listing_entries(&st(|p| p.row_mode = 0o040755));
    check(
        "Favorites menu uses the displayed spelling",
        label(&i18n, entry(&dir, "addFavourite")),
        "Add to Favorites".into(),
    );
    check(
        "selected directory can be pinned",
        entry(&dir, "addFavourite").unwrap().disabled,
        false,
    );
    check(
        "multi-selection cannot pin a cursor sibling",
        entry(
            &listing_entries(&st(|p| {
                p.row_mode = 0o040755;
                p.selection_count = 2
            })),
            "addFavourite",
        )
        .is_none(),
        true,
    );
    check(
        "symlink metadata cannot pretend to be a directory",
        entry(
            &listing_entries(&st(|p| p.row_mode = 0o120777)),
            "addFavourite",
        )
        .is_none(),
        true,
    );
    check(
        "missing metadata cannot be pinned",
        entry(&listing_entries(&st(|p| p.row_mode = 0)), "addFavourite").is_none(),
        true,
    );
    check(
        "background pins the current folder without selected rows",
        entry(
            &listing_entries(&st(|p| {
                p.has_row = false;
                p.selection_count = 0
            })),
            "addFavourite",
        )
        .unwrap()
        .disabled,
        false,
    );
    check(
        "direct background builder ignores row eligibility",
        entry(
            &super::build::build_entries(super::build::Kind::Background, &state()),
            "addFavourite",
        )
        .unwrap()
        .disabled,
        false,
    );
    check(
        "a folder row keeps the directory type bits",
        0o040755 & super::build::S_IFMT,
        S_IFDIR,
    );
}

#[test]
fn trash_and_rail_rows() {
    let i18n = english();
    check(
        "empty Trash retains both disabled actions",
        actions(&trash_entries(0, false)),
        "open,restoreAll,emptyTrash".into(),
    );
    check(
        "empty Trash disables restore",
        entry(&trash_entries(0, false), "restoreAll")
            .unwrap()
            .disabled,
        true,
    );
    check(
        "empty Trash disables empty",
        entry(&trash_entries(0, false), "emptyTrash")
            .unwrap()
            .disabled,
        true,
    );
    check(
        "busy Trash disables destructive reactivation",
        entry(&trash_entries(4, true), "emptyTrash")
            .unwrap()
            .disabled,
        true,
    );
    check(
        "full idle Trash enables restore",
        entry(&trash_entries(4, false), "restoreAll")
            .unwrap()
            .disabled,
        false,
    );
    check(
        "Empty Trash gains the ellipsis and keeps the destructive ink at the end",
        format!(
            "{}|{}",
            label(&i18n, entry(&trash_entries(4, false), "emptyTrash")),
            entry(&trash_entries(4, false), "emptyTrash")
                .unwrap()
                .danger
        ),
        "Empty Trash\u{2026}|true".into(),
    );
    let glyphs = |rows: &[super::entry::Entry]| {
        rows.iter()
            .filter(|r| !r.separator)
            .map(|r| r.glyph.unwrap_or(""))
            .collect::<Vec<_>>()
            .join("|")
    };
    let share = RailEntry {
        group: "network".into(),
        kind: "share".into(),
        mounted: true,
        ..RailEntry::default()
    };
    check(
        "a mounted saved place opens like every other Flea menu",
        labels(&i18n, &rail_entries(&share)),
        "Open|Unmount|Rename|Edit Address\u{2026}|Remove from Network".into(),
    );
    check(
        "and its marks are the main menu's own",
        glyphs(&rail_entries(&share)),
        "folder-open|drive|rename|sliders|minus".into(),
    );
    check(
        "a group change draws a separator",
        rail_entries(&share).iter().filter(|r| r.separator).count(),
        3,
    );
    let stick = RailEntry {
        group: "device".into(),
        kind: "volume".into(),
        device: "/dev/sda1".into(),
        mounted: true,
        removable: true,
        ..RailEntry::default()
    };
    check(
        "a mounted volume offers Open, Unmount, Eject and Flash image",
        labels(&i18n, &rail_entries(&stick)),
        "Open|Unmount|Eject|Flash Image".into(),
    );
    check(
        "Mount and Unmount share the drive mark",
        glyphs(&rail_entries(&stick)),
        "folder-open|drive|eject|file".into(),
    );
    let idle = RailEntry {
        mounted: false,
        removable: false,
        ..stick.clone()
    };
    check(
        "an unmounted volume offers the mount its activation does",
        labels(&i18n, &rail_entries(&idle)),
        "Mount".into(),
    );
    let saved = RailEntry {
        mounted: false,
        ..share.clone()
    };
    check(
        "an unmounted saved place keeps Rename, Edit address and Remove from Network",
        labels(&i18n, &rail_entries(&saved)),
        "Rename|Edit Address\u{2026}|Remove from Network".into(),
    );
    let fav = RailEntry {
        group: "favourite".into(),
        kind: "favourite".into(),
        ..RailEntry::default()
    };
    check(
        "a favourite with the Places menu off is INVENTORY's own row, never a bare Remove",
        labels(&i18n, &rail_entries(&fav)),
        "Remove from Favorites".into(),
    );
    check(
        "New tab keeps its own label",
        i18n.tr(spec_of("openTab").unwrap().label),
        "Open in New Tab".into(),
    );
    check(
        "Add to shelf keeps its label and the shelf's own glyph",
        format!(
            "{}|{}",
            i18n.tr(spec_of("shelf").unwrap().label),
            spec_of("shelf").unwrap().glyph
        ),
        "Add to Shelf|shelf".into(),
    );
    check(
        "the rail menu's Rename keeps its one word",
        label(&i18n, rail_entries(&saved).first()),
        "Rename".into(),
    );
    let crypt = RailEntry {
        device: "/dev/mapper/luks-1".into(),
        ..stick.clone()
    };
    check(
        "Flash image never shows on an open crypt",
        labels(&i18n, &rail_entries(&crypt)),
        "Open|Unmount|Eject".into(),
    );
    check(
        "no rail entry offers nothing",
        super::build::build_entries(super::build::Kind::Rail, &Context::default()).len(),
        0,
    );
}

#[test]
fn optional_rows_and_counts() {
    let i18n = english();
    let all = listing_entries(&st(|p| {
        p.hidden.clear();
        p.clipboard_available = true;
        p.row_is_symlink = true;
        p.has_shebang = true;
        p.cursor_is_target = true
    }));
    check(
        "stored delete id reaches permanent deletion action",
        entry(&all, "deletePermanently").unwrap().id.as_ref(),
        "delete",
    );
    check(
        "all optional file controls exist",
        [
            "openWith",
            "moveTo",
            "copyTo",
            "properties",
            "permissions",
            "makeExecutable",
            "copyAs",
            "showOriginal",
            "pasteAs",
            "openTerminal",
        ]
        .iter()
        .all(|a| entry(&all, a).is_some()),
        true,
    );
    check(
        "while Invert selection lives on the background menu",
        entry(
            &listing_entries(&st(|p| {
                p.has_row = false;
                p.hidden.clear()
            })),
            "invertSelection",
        )
        .is_some(),
        true,
    );
    check(
        "Show original draws the link mark in the open group",
        entry(
            &listing_entries(&st(|p| {
                p.hidden.clear();
                p.row_is_symlink = true
            })),
            "showOriginal",
        )
        .unwrap()
        .glyph,
        Some("symlink"),
    );
    check(
        "permanent deletion carries danger role",
        entry(&all, "deletePermanently").unwrap().danger,
        true,
    );
    let share = listing_entries(&st(|p| {
        p.hidden.clear();
        p.can_trash = false
    }));
    check(
        "a file on an SMB share is not offered Move to Trash",
        entry(&share, "trash").is_none(),
        true,
    );
    check(
        "while Delete permanently, which the share can do, stays and stays enabled",
        entry(&share, "deletePermanently").unwrap().disabled,
        false,
    );
    check(
        "and a local file keeps Move to Trash",
        entry(&listing_entries(&state()), "trash").is_some(),
        true,
    );
    let many = listing_entries(&st(|p| {
        p.hidden.clear();
        p.selection_count = 2
    }));
    check(
        "single-item actions stay present but disabled on multi-selection",
        ["openWith", "properties"]
            .iter()
            .all(|a| entry(&many, a).unwrap().disabled),
        true,
    );
    check(
        "Duplicate, which acts on one item, is hidden instead of dimmed",
        entry(&many, "duplicate").is_none(),
        true,
    );
    check(
        "multi-selection keeps Rename enabled for the bulk card",
        entry(&many, "rename").unwrap().disabled,
        false,
    );
    let rename = |count| {
        label(
            &i18n,
            entry(
                &listing_entries(&st(|p| {
                    p.hidden.clear();
                    p.selection_count = count
                })),
                "rename",
            ),
        )
    };
    check(
        "the file menu's rename row reads Rename for one selected row",
        rename(1),
        "Rename".into(),
    );
    check(
        "and Rename 2 items\u{2026} for two",
        rename(2),
        "Rename 2 Items\u{2026}".into(),
    );
    check(
        "and Rename 12 items\u{2026} for twelve",
        rename(12),
        "Rename 12 Items\u{2026}".into(),
    );
    check(
        "multi-selection keeps Permissions enabled on files",
        entry(
            &listing_entries(&st(|p| {
                p.hidden.clear();
                p.selection_count = 2;
                p.selection_modes = vec![0o100644, 0o100644]
            })),
            "permissions",
        )
        .unwrap()
        .disabled,
        false,
    );
    let copy = entry(&all, "copyAs").unwrap().submenu.clone().unwrap();
    check(
        "Copy as holds Path, Relative Path, Name, Stem, Folder path, URI and Shell-quoted",
        copy.iter()
            .map(|r| r.id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        "copyPath,copyRelative,copyName,copyStem,copydirpath,copyUri,copyQuoted".into(),
    );
    check(
        "and each leaf wears the board's mark",
        copy.iter()
            .map(|r| r.glyph.unwrap())
            .collect::<Vec<_>>()
            .join(","),
        "file-text,file,type,type,folder,globe,terminal".into(),
    );
    check(
        "Copy As rows read Path, Relative Path, Name, Name Without Extension, Folder Path, File URI, Shell-Quoted",
        labels(&i18n, &copy),
        "Path|Relative Path|Name|Name Without Extension|Folder Path|File URI|Shell-Quoted".into(),
    );
    let paste = entry(&all, "pasteAs").unwrap().submenu.clone().unwrap();
    check(
        "Paste as holds Link, Absolute link and Hard link",
        paste
            .iter()
            .map(|r| r.id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        "pasteLink,pasteAbsoluteLink,pasteHardLink".into(),
    );
    check(
        "and Link shares Paste's mark rather than repeating it",
        paste
            .iter()
            .map(|r| r.glyph.unwrap())
            .collect::<Vec<_>>()
            .join(","),
        "symlink,symlink,copy".into(),
    );
    check(
        "Paste As rows read Link, Absolute Link, Hard Link",
        labels(&i18n, &paste),
        "Link|Absolute Link|Hard Link".into(),
    );
    check(
        "a linkless filesystem offers no Paste as rows at all",
        entry(
            &listing_entries(&st(|p| {
                p.hidden.clear();
                p.clipboard_available = true;
                p.can_link = false
            })),
            "pasteAs",
        )
        .is_none(),
        true,
    );
    check(
        "an empty clipboard's Paste As is hidden instead of dimmed",
        entry(&listing_entries(&st(|p| p.hidden.clear())), "pasteAs").is_none(),
        true,
    );
    let ro_bg = listing_entries(&st(|p| {
        p.has_row = false;
        p.hidden.clear();
        p.clipboard_available = true;
        p.dir_writable = false
    }));
    check(
        "a read-only folder turns its background write rows off",
        ["newFolder", "newFile", "paste"]
            .iter()
            .map(|a| entry(&ro_bg, a).unwrap().disabled)
            .collect::<Vec<_>>(),
        vec![true; 3],
    );
    let ro = listing_entries(&st(|p| {
        p.hidden.clear();
        p.dir_writable = false
    }));
    check(
        "and its row write rows too",
        ["duplicate", "rename", "trash"]
            .iter()
            .map(|a| entry(&ro, a).unwrap().disabled)
            .collect::<Vec<_>>(),
        vec![true; 3],
    );
    let rw = listing_entries(&st(|p| p.hidden.clear()));
    check(
        "while a writable folder leaves them enabled",
        ["duplicate", "trash"]
            .iter()
            .map(|a| entry(&rw, a).unwrap().disabled)
            .collect::<Vec<_>>(),
        vec![false; 2],
    );
    check(
        "Show original is absent except on a symlink",
        entry(&rw, "showOriginal").is_none(),
        true,
    );
    check(
        "and present on one",
        entry(
            &listing_entries(&st(|p| {
                p.hidden.clear();
                p.row_is_symlink = true
            })),
            "showOriginal",
        )
        .is_some(),
        true,
    );
    check(
        "and present once something is",
        entry(
            &listing_entries(&st(|p| {
                p.has_row = false;
                p.hidden.clear()
            })),
            "invertSelection",
        )
        .is_some(),
        true,
    );
    check(
        "Invert selection remains present with nothing selected",
        entry(
            &listing_entries(&st(|p| {
                p.has_row = false;
                p.selection_count = 0;
                p.hidden.clear()
            })),
            "invertSelection",
        )
        .is_some(),
        true,
    );
    check(
        "the background shows Open in terminal while it stays hidden",
        entry(&listing_entries(&st(|p| p.has_row = false)), "openTerminal").is_some(),
        true,
    );
    check(
        "and the file menu hides it behind that same switch",
        entry(&listing_entries(&state()), "openTerminal").is_none(),
        true,
    );
    check(
        "with the switch off the file menu shows it too",
        entry(
            &listing_entries(&st(|p| p.hidden = strings(&["delete"]))),
            "openTerminal",
        )
        .is_some(),
        true,
    );
    check(
        "Open cannot be hidden",
        is_hidden(&strings(&["open"]), "open"),
        false,
    );
    let view = strings(&["toggleHidden", "toggleLangMarks", "openTerminal"]);
    check(
        "Settings > Menus takes the view rows off the file menu",
        actions(&listing_entries(&st(|p| p.hidden = view.clone()))).contains("toggle"),
        false,
    );
    check(
        "and the background keeps them with Open in Terminal",
        actions(&listing_entries(&st(|p| {
            p.hidden = view.clone();
            p.has_row = false;
        }))),
        "newFolder,newFile,paste,selectAll,invertSelection,openTerminal,addFavourite,sort,toggleHidden,toggleLangMarks,settings".into(),
    );
    check(
        "stored lower-case id hides corresponding action",
        entry(
            &listing_entries(&st(|p| p.hidden = strings(&["openwith"]))),
            "openWith",
        )
        .is_none(),
        true,
    );
    check(
        "hidden capability rows leave no doubled separators",
        separated(&listing_entries(&st(|p| {
            p.hidden = strings(&["compress", "convert", "taildrop", "dropbox"])
        }))),
        true,
    );
    let source =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/uischema.rs")).unwrap();
    let quoted: Vec<String> = SHIPPED_HIDDEN
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect();
    check(
        "the shipped hidden set is the one src/uischema.rs writes",
        quoted.iter().all(|q| source.contains(q.as_str())),
        true,
    );
    check(
        "with nothing stored the shipped set hides, and Add to Shelf waits for the shelf's switch",
        (
            effective_hidden(None, false).len(),
            effective_hidden(None, false).contains(&"shelf".to_owned()),
        ),
        (SHIPPED_HIDDEN.len() + 1, true),
    );
    check(
        "a stored shelf id never outranks the shelf's own switch",
        effective_hidden(Some(&strings(&["shelf"])), true),
        Vec::<String>::new(),
    );
}

#[test]
fn print_convert_and_combine() {
    let i18n = english();
    let print = listing_entries(&st(|p| p.printable = true));
    check(
        "Print joins the first group after Open With",
        actions(&print).starts_with("open,openWith,print,"),
        true,
    );
    check(
        "Print ends in an ellipsis and wears the printer mark",
        format!(
            "{}|{}",
            label(&i18n, entry(&print, "print")),
            entry(&print, "print").unwrap().glyph.unwrap()
        ),
        "Print\u{2026}|printer".into(),
    );
    check(
        "It shows only for kinds that print faithfully",
        entry(&listing_entries(&state()), "print").is_none(),
        true,
    );
    let three = listing_entries(&st(|p| {
        p.printable = true;
        p.selection_count = 3;
        p.selection_images = 3
    }));
    check(
        "Print 3 Items and Rename 3 Items name the selection",
        format!(
            "{}|{}",
            label(&i18n, entry(&three, "print")),
            label(&i18n, entry(&three, "rename"))
        ),
        "Print 3 Items\u{2026}|Rename 3 Items\u{2026}".into(),
    );
    check(
        "Combine Images sits under Convert in the group Compress leads",
        actions(&three).contains("compress,convert,combineImages"),
        true,
    );
    check(
        "Convert stays for three images",
        label(&i18n, entry(&three, "convert")),
        "Convert\u{2026}".into(),
    );
    check(
        "Combine Images shows only while two or more images are selected",
        entry(&listing_entries(&state()), "combineImages").is_none(),
        true,
    );
    let mixed = listing_entries(&st(|p| {
        p.selection_count = 3;
        p.selection_images = 2
    }));
    check(
        "a mixed selection offers neither Convert nor Combine Images",
        (
            entry(&mixed, "convert").is_none(),
            entry(&mixed, "combineImages").is_none(),
        ),
        (true, true),
    );
}

#[test]
fn a_folder_row_offers_no_taildrop_peers() {
    // tests/js/menu.js "cold folder listing suppresses file-only peers": a folder takes no Taildrop peer.
    let rows = listing_entries(&st(|p| p.row_is_file = false));
    let taildrop = rows
        .iter()
        .find(|e| e.action == "taildrop")
        .expect("Taildrop installed draws its row");
    check(
        "cold folder listing suppresses file-only peers",
        taildrop.submenu.as_ref().map_or(0, Vec::len),
        0,
    );
    check("folder Taildrop row is dimmed", taildrop.disabled, true);
    // GM 2026-10-29: it reads red with no state word, so the menu keeps its size.
    check(
        "folder Taildrop row is red and says no word",
        (taildrop.errored, taildrop.hint.is_none()),
        (true, true),
    );
}

// An empty application list takes Open With away only once the catalogue answered; one past the deadline keeps it.
#[test]
fn open_with_leaves_only_on_an_answered_empty_catalogue() {
    let answered = st(|p| p.open_with_loaded = true);
    check(
        "an answered empty catalogue takes Open With away",
        entry(&listing_entries(&answered), "openWith").is_none(),
        true,
    );
    let late = listing_entries(&st(|p| {
        p.open_with_loaded = true;
        p.open_with_late = true;
    }));
    check(
        "a catalogue past the deadline keeps it, with Another Application alone",
        entry(&late, "openWith")
            .and_then(|e| e.submenu.as_deref())
            .map(|leaves| {
                leaves
                    .iter()
                    .map(|leaf| leaf.id.to_string())
                    .collect::<Vec<_>>()
            }),
        Some(vec![super::rows::OPEN_WITH_OTHER.to_string()]),
    );
}
