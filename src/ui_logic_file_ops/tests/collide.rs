// Ported checks from tests/js/collide.js and tests/js/collidemerge.js against the collide, lines and host modules.
use crate::ui_logic_file_ops::collide::{self, Choice, Fat, Key, Request};
use crate::ui_logic_file_ops::lines::{self, Outcome};
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::transfer::Transfer;
use crate::ui_logic_file_ops::Collision;

// Files named the way the oracle names() builds them.
fn names(list: &[&str]) -> Vec<Collision> {
    list.iter()
        .map(|n| Collision {
            name: (*n).into(),
            dir: false,
            icon: "image-x-generic".into(),
        })
        .collect()
}

// A folder collision as the merge walk lists it.
pub(super) fn folder(name: &str) -> Collision {
    Collision {
        name: name.into(),
        dir: true,
        icon: "folder".into(),
    }
}

// A key name as the handler reads it, without Shift held.
pub(super) fn key(name: &str) -> Key {
    collide::key(name, false)
}

// Shift+Tab reaches the handler as Tab with Shift held.
fn shift_tab() -> Key {
    collide::key("tab", true)
}

// A row drop's transfer request: move rows 2 and 3 into a folder.
pub(super) fn by_rows() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("move"))
        .with("rows", &collide::numbers(&[2, 3]))
        .with("dest", &collide::text("/d/omarchy"))
}

// A paste's transfer request: copy one path into /b.
pub(super) fn by_path() -> Request {
    transfer_copy_paths(&["/a/x.png"], "/b")
}

pub(super) fn transfer_copy_paths(paths: &[&str], dest: &str) -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("copy"))
        .with("paths", &collide::texts(paths))
        .with("dest", &collide::text(dest))
}

// A Copy to menu's transfer, which carries the menu's own selection.
pub(super) fn copy_to() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("copy"))
        .with("menuId", "31")
        .with("dest", &collide::text("/f"))
}

// A Move to Dropbox transfer, whose rows the backend never reads beside a menu.
pub(super) fn dropbox() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("move"))
        .with("rows", &collide::numbers(&[1, 2]))
        .with("dest", &collide::text("/dropbox"))
        .with("menuId", "32")
}

// A shelf drop: the token names what moves while the drag carries the paths.
pub(super) fn shelf() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text(""))
        .with("paths", &collide::texts(&[]))
        .with("dest", &collide::text("/e"))
        .with("shelf", &collide::text("tok"))
}

// A paste as links request for one path, optionally with row 2 named.
pub(super) fn link_paths(rows: bool) -> Request {
    let req = Request::new()
        .with("c", &collide::text("link"))
        .with("op", &collide::text("relative"))
        .with("paths", &collide::texts(&["/s/a.txt"]))
        .with("dest", &collide::text("/d"));
    if rows {
        req.with("rows", &collide::numbers(&[2]))
    } else {
        req
    }
}

// A row lifted in listing 7, dropped on the omarchy folder.
pub(super) fn lifted() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("move"))
        .with("rows", &collide::numbers(&[2]))
        .with("dest", &collide::text("/d/omarchy"))
        .with("listing", "7")
}

// A row drop of row 0 with no listing stamp yet.
pub(super) fn row_zero() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("move"))
        .with("rows", &collide::numbers(&[0]))
        .with("dest", &collide::text("/d/omarchy"))
}

// A move of one path, the cut a paste leaves on the clipboard.
pub(super) fn move_paths() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("move"))
        .with("paths", &collide::texts(&["/s/a.txt"]))
        .with("dest", &collide::text("/d"))
}

// A paste of a file into /dest, as the clipboard hands it over when cut.
pub(super) fn paste_move() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("move"))
        .with("paths", &collide::texts(&["/src/a.txt"]))
        .with("dest", &collide::text("/dest"))
}

// A paste of a file into /dest, as the clipboard hands it over when copied.
pub(super) fn paste_copy() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("copy"))
        .with("paths", &collide::texts(&["/src/a.txt"]))
        .with("dest", &collide::text("/dest"))
}

// A Merge's folder paste: copy the trip folder into Pictures.
pub(super) fn folder_paste() -> Request {
    transfer_copy_paths(&["/s/trip"], "/d/Pictures")
}

// A Merge of rows: move row 2 in listing 4 into Pictures.
pub(super) fn folder_rows() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("move"))
        .with("rows", &collide::numbers(&[2]))
        .with("dest", &collide::text("/d/Pictures"))
        .with("listing", "4")
}

// A move of the trip folder into Pictures, the cut the second merge answers.
pub(super) fn folder_move() -> Request {
    Request::new()
        .with("c", &collide::text("transfer"))
        .with("op", &collide::text("move"))
        .with("paths", &collide::texts(&["/s/trip"]))
        .with("dest", &collide::text("/d/Pictures"))
}

#[test]
fn collision_titles_and_folder_names_follow_the_board() {
    let cat = Catalog::english();
    let i18n = &cat.i18n;
    // oracle tests/js/collide.js:16
    assert_eq!(
        collide::title(i18n, &names(&["screenshot.png"]), 1, "/home/gm/Pictures"),
        "screenshot.png already exists in Pictures."
    );
    // oracle tests/js/collide.js:18
    assert_eq!(
        collide::title(i18n, &names(&["a", "b", "c"]), 3, "/home/gm/Downloads"),
        "3 items already exist in Downloads."
    );
    // oracle tests/js/collide.js:20
    assert_eq!(
        collide::title(i18n, &names(&["a", "b", "c"]), 12, "/home/gm/Downloads"),
        "12 items already exist in Downloads."
    );
    // oracle tests/js/collide.js:22
    assert_eq!(
        collide::title(i18n, &names(&["a", "b"]), 1204, "/home/gm/Downloads"),
        "1,204 items already exist in Downloads."
    );
    // oracle tests/js/collide.js:24
    assert_eq!(collide::folder_name("/"), "/");
    // oracle tests/js/collide.js:25
    assert_eq!(collide::folder_name("/home/gm/Pictures"), "Pictures");
}

#[test]
fn more_row_and_explanation_read_as_the_card_does() {
    let cat = Catalog::english();
    let i18n = &cat.i18n;
    // oracle tests/js/collide.js:28
    assert_eq!(collide::more(i18n, 3, 3), "");
    // oracle tests/js/collide.js:29
    assert_eq!(collide::more(i18n, 4, 3), "and 1 more");
    // oracle tests/js/collide.js:30
    assert_eq!(collide::more(i18n, 40, 3), "and 37 more");
    // oracle tests/js/collide.js:31
    assert_eq!(collide::more(i18n, 1204, 3), "and 1,201 more");
    // oracle tests/js/collide.js:32
    assert_eq!(
        collide::explain(i18n),
        "Replaced items go to Trash, and Undo restores them."
    );
}

#[test]
fn counts_group_in_thousands_on_the_operations_lines() {
    let cat = Catalog::english();
    let i18n = &cat.i18n;
    // oracle tests/js/collide.js:35
    assert_eq!(lines::items(i18n, 1204), "1,204 items");
    // oracle tests/js/collide.js:36
    assert_eq!(
        lines::transfer_done(
            i18n,
            "default",
            &Transfer::started(1, false, 1204, false),
            Outcome {
                ok: 1204,
                ..Default::default()
            }
        ),
        "Copied 1,204 items · z undoes"
    );
    // oracle tests/js/collide.js:39
    assert_eq!(
        lines::transfer_done(
            i18n,
            "default",
            &Transfer::started(1, false, 1204, false),
            Outcome {
                ok: 1000,
                failed: 204,
                ..Default::default()
            }
        ),
        "Copied 1,000 of 1,204 · 204 failed · z undoes"
    );
    // oracle tests/js/collide.js:42
    assert_eq!(
        lines::trashed(i18n, "default", 0, 1204, ""),
        "1,204 items could not be moved to Trash."
    );
    // oracle tests/js/collide.js:44
    assert_eq!(
        lines::trashed(i18n, "default", 1000, 204, ""),
        "Moved 1,000 items to Trash, 204 failed · z undoes"
    );
    // oracle tests/js/collide.js:46
    assert_eq!(
        lines::deleted_line(i18n, 3, 1204, 0, false),
        "Deleted 3 of 1,204 items"
    );
    // oracle tests/js/collide.js:49
    assert_eq!(
        lines::deleted_line(i18n, 3, 1204, 204, true),
        "Deleted 3 of 1,204 items · 204 failed · cancelled"
    );
}

#[test]
fn focus_walks_the_buttons_and_stops_at_the_ends() {
    let cat = Catalog::english();
    let i18n = &cat.i18n;
    // oracle tests/js/collide.js:54
    assert_eq!(collide::START, Choice::Keep);
    // oracle tests/js/collide.js:55
    let labels: Vec<String> = collide::BUTTONS
        .iter()
        .map(|b| i18n.tr(b.label_key()))
        .collect();
    assert_eq!(labels.join("|"), "Cancel|Skip|Merge|Replace|Keep Both");
    // oracle tests/js/collide.js:57
    assert_eq!(collide::moved(Choice::Skip, key("l"), true), Choice::Merge);
    // oracle tests/js/collide.js:58
    assert_eq!(
        collide::moved(Choice::Merge, key("l"), true),
        Choice::Replace
    );
    // oracle tests/js/collide.js:59
    assert_eq!(
        collide::moved(Choice::Skip, key("right"), true),
        Choice::Merge
    );
    // oracle tests/js/collide.js:60
    assert_eq!(collide::moved(Choice::Keep, key("l"), true), Choice::Keep);
    // oracle tests/js/collide.js:61
    assert_eq!(collide::moved(Choice::Merge, key("h"), true), Choice::Skip);
    // oracle tests/js/collide.js:62
    assert_eq!(
        collide::moved(Choice::Cancel, key("h"), true),
        Choice::Cancel
    );
    // oracle tests/js/collide.js:63
    assert_eq!(
        collide::moved(Choice::Skip, key("left"), true),
        Choice::Cancel
    );
    // oracle tests/js/collide.js:64
    assert_eq!(
        collide::moved(Choice::Keep, key("tab"), true),
        Choice::Cancel
    );
    // oracle tests/js/collide.js:65
    assert_eq!(
        collide::moved(Choice::Cancel, key("backtab"), true),
        Choice::Keep
    );
    // oracle tests/js/collide.js:66
    assert_eq!(
        collide::moved(Choice::Cancel, shift_tab(), true),
        Choice::Keep
    );
    // oracle tests/js/collide.js:68
    let fat_walk: Vec<&str> = [
        (Fat::Cancel, key("backtab")),
        (Fat::Cancel, shift_tab()),
        (Fat::Rename, key("tab")),
        (Fat::Cancel, key("h")),
        (Fat::Rename, key("l")),
    ]
    .into_iter()
    .map(|(focus, k)| collide::walk(&collide::FAT_BUTTONS, focus, k, collide::FAT_START).wire())
    .collect();
    assert_eq!(fat_walk.join("|"), "rename|rename|cancel|cancel|rename");
    // oracle tests/js/collide.js:71
    let tab: Vec<&str> = [
        Choice::Cancel,
        Choice::Skip,
        Choice::Merge,
        Choice::Replace,
        Choice::Keep,
    ]
    .iter()
    .map(|b| collide::moved(*b, key("tab"), true).wire())
    .collect();
    assert_eq!(tab.join("|"), "skip|merge|replace|keep|cancel");
    // oracle tests/js/collide.js:73
    let back: Vec<&str> = [Choice::Skip, Choice::Merge, Choice::Replace, Choice::Keep]
        .iter()
        .map(|b| collide::moved(*b, key("backtab"), true).wire())
        .collect();
    assert_eq!(back.join("|"), "cancel|skip|merge|replace");
    // oracle tests/js/collide.js:75
    assert_eq!(collide::moved(Choice::Skip, key("j"), false), Choice::Skip);
    // oracle tests/js/collide.js:76
    assert!(collide::activates(key("return")) && collide::activates(key("enter")));
    // oracle tests/js/collide.js:77
    assert!(collide::activates(key("space")));
    // oracle tests/js/collide.js:78
    assert!(!collide::activates(key("escape")));
}
