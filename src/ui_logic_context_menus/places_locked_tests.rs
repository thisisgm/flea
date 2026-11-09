//! The Locked tile cases of tests/js/menu.js.
use super::places::*;
use super::testkit::*;

const SHIPPED_HIDDEN: [&str; 10] = [
    "delete",
    "openTerminal",
    "placeMenu",
    "runScript",
    "moveto",
    "copyto",
    "properties",
    "permissions",
    "copyAs",
    "extThumbs",
];

#[test]
fn locked_tile() {
    let i18n = english();
    let locked = locked_entries(0o040000, &[]);
    check(
        "the Locked tile offers only rows that act without listing the folder",
        actions(&locked),
        "openTerminal,permissions,copypath".into(),
    );
    check(
        "its Open in Terminal row carries the terminal dispatch",
        entry(&locked, "openTerminal").is_some(),
        true,
    );
    check(
        "its Permissions row carries the permissions dispatch",
        entry(&locked, "permissions").is_some(),
        true,
    );
    check(
        "its Copy path row carries the copy dispatch",
        entry(&locked, "copypath").is_some(),
        true,
    );
    check(
        "and no row that would create in, paste into or sort the parent",
        [
            "newFolder",
            "newFile",
            "paste",
            "selectAll",
            "sort",
            "toggleHidden",
            "settings",
        ]
        .iter()
        .all(|a| entry(&locked, a).is_none()),
        true,
    );
    let refusal = |rows: &[super::entry::Entry]| {
        let e = entry(rows, "permissions").unwrap();
        format!("{}|{}|{}", e.disabled, e.errored, hint(&i18n, Some(e)))
    };
    check(
        "Permissions stays plain while its owner may still be the operator, the way back in",
        refusal(&locked),
        "false|false|undefined".into(),
    );
    check(
        "Permissions for a folder owned by somebody else reads red and says not yours",
        refusal(&locked_entries(0o040755, &[])),
        "true|true|not yours".into(),
    );
    check(
        "Permissions with no mode to read reads red and names no word",
        refusal(&locked_entries(0, &[])),
        "true|true|undefined".into(),
    );
    check(
        "hiding Open in terminal takes it off the Locked tile as well",
        entry(
            &locked_entries(0o040000, &strings(&["openTerminal"])),
            "openTerminal",
        )
        .is_none(),
        true,
    );
    check(
        "the shipped hidden set leaves the Locked tile with no row",
        locked_entries(0o040000, &strings(&SHIPPED_HIDDEN)).len(),
        0,
    );
    check(
        "and it refuses with the Menus switch sentence",
        locked_refusal(0o040000, &strings(&SHIPPED_HIDDEN)).map(|k| i18n.tr(k)),
        Some("Open in Terminal, Permissions and Copy As are hidden in Settings > Menus.".into()),
    );
    check(
        "one row shown yields that row alone",
        actions(&locked_entries(
            0o040000,
            &strings(&["openTerminal", "permissions"]),
        )),
        "copypath".into(),
    );
    check(
        "and it refuses nothing then",
        locked_refusal(0o040000, &strings(&["openTerminal", "permissions"])),
        None,
    );
    check(
        "nor names a hidden pair, since Permissions is not the row left",
        locked_notice(&strings(&["openTerminal", "permissions"])),
        None,
    );
    let shipped: Vec<String> = strings(&SHIPPED_HIDDEN)
        .into_iter()
        .filter(|id| id != "properties" && id != "permissions")
        .collect();
    check(
        "Properties and Permissions shown leave the tile Permissions alone",
        actions(&locked_entries(0o040000, &shipped)),
        "permissions".into(),
    );
    check(
        "so the menu opens without refusing",
        locked_refusal(0o040000, &shipped),
        None,
    );
    check(
        "and the status line names the two still hidden",
        locked_notice(&shipped).map(|k| i18n.tr(k)),
        Some("Open in Terminal and Copy As are hidden in Settings > Menus.".into()),
    );
    check(
        "every row hidden keeps the refusal and no second sentence",
        locked_notice(&strings(&SHIPPED_HIDDEN)),
        None,
    );
    check("every row shown names nothing", locked_notice(&[]), None);
    check(
        "its Copy Path row reads Copy Path",
        label(&i18n, entry(&locked, "copypath")),
        "Copy Path".into(),
    );
}
