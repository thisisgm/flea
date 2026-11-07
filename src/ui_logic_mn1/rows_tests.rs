//! tests/js/menu.js row rules and menuhunt.js eligibility: providers, state words, permissions, flyouts and steps.
use super::build::{listing_entries, Context};
use super::entry::{Entry, Text};
use super::refresh::{
    provider_view, refreshed_cursor, settle, validate_choice, Live, Settled, EMPTY_CLIPBOARD,
};
use super::rows::*;
use super::testkit::*;

fn st(change: impl FnOnce(&mut Context)) -> Context {
    let mut p = state();
    change(&mut p);
    p
}

fn refusal(i18n: &crate::i18n::I18n, r: &Refusal) -> String {
    let hint = r
        .hint
        .as_ref()
        .map_or_else(|| "undefined".into(), |t| t.resolve(i18n));
    format!("{}|{}|{hint}", r.disabled, r.errored)
}

fn row_refusal(i18n: &crate::i18n::I18n, e: Option<&Entry>) -> String {
    let e = e.unwrap();
    refusal(
        i18n,
        &Refusal {
            disabled: e.disabled,
            errored: e.errored,
            hint: e.hint.clone(),
        },
    )
}

fn word(change: impl FnOnce(&mut Context), action: &str) -> String {
    hint(&english(), entry(&listing_entries(&st(change)), action))
}

#[test]
fn make_executable_and_capabilities() {
    let exe = |change: &dyn Fn(&mut Context)| {
        listing_entries(&st(|p| {
            p.hidden.clear();
            p.row_mode = 0o100644;
            p.has_shebang = true;
            p.cursor_is_target = true;
            change(p)
        }))
    };
    check(
        "Make executable shows on a script missing its bit",
        entry(&exe(&|_| {}), "makeExecutable").is_some(),
        true,
    );
    check(
        "and wears the play mark no neighbour wears",
        entry(&exe(&|_| {}), "makeExecutable").unwrap().glyph,
        Some("play"),
    );
    check(
        "without a shebang it is absent",
        entry(&exe(&|p| p.has_shebang = false), "makeExecutable").is_none(),
        true,
    );
    check(
        "with the execute bit already set it is absent too",
        entry(&exe(&|p| p.row_mode = 0o100744), "makeExecutable").is_none(),
        true,
    );
    check(
        "hunt: Make executable is absent with group execute already set",
        entry(&exe(&|p| p.row_mode = 0o100654), "makeExecutable").is_none(),
        true,
    );
    check(
        "hunt: Make executable is absent with everyone execute already set",
        entry(&exe(&|p| p.row_mode = 0o100645), "makeExecutable").is_none(),
        true,
    );
    check(
        "on a directory it is absent",
        entry(&exe(&|p| p.row_mode = 0o040755), "makeExecutable").is_none(),
        true,
    );
    check(
        "on a multi-selection it is absent",
        entry(&exe(&|p| p.selection_count = 2), "makeExecutable").is_none(),
        true,
    );
    check(
        "on a single selection that is not the cursor row it is absent",
        entry(&exe(&|p| p.cursor_is_target = false), "makeExecutable").is_none(),
        true,
    );
    check(
        "and the Menus switch takes it away like any other row",
        entry(
            &exe(&|p| p.hidden = strings(&["makeExecutable"])),
            "makeExecutable",
        )
        .is_none(),
        true,
    );
    let rows = exe(&|p| p.selection_modes = vec![0o100644]);
    let at = rows
        .iter()
        .position(|r| !r.separator && r.action == "makeExecutable")
        .unwrap();
    check(
        "it sits with Open, in the first group before Move to Trash's",
        rows[0].action == "open" && rows[..at].iter().all(|r| !r.separator),
        true,
    );
    check(
        "missing converter removes Convert",
        entry(&listing_entries(&st(|p| p.can_convert = false)), "convert").is_none(),
        true,
    );
    check(
        "missing archiver removes Compress",
        entry(
            &listing_entries(&st(|p| p.archive_formats.clear())),
            "compress",
        )
        .is_none(),
        true,
    );
    let i18n = english();
    let no_reader = listing_entries(&st(|p| {
        p.row_is_archive = true;
        p.can_extract = false;
        p.archive_formats = strings(&["zip", "tar"])
    }));
    let extract = entry(&no_reader, "extract");
    check(
        "a recognized archive with no reader keeps its Extract row",
        extract.is_some(),
        true,
    );
    check(
        "and the disabled row carries the missing reader as its reason",
        format!(
            "{}|{}|{}",
            extract.unwrap().disabled,
            hint(&i18n, extract),
            extract.unwrap().hint_wrap
        ),
        "true|7-Zip is not installed|true".into(),
    );
    check(
        "supported archive offers Extract",
        entry(
            &listing_entries(&st(|p| p.row_is_archive = true)),
            "extract",
        )
        .is_some(),
        true,
    );
    check(
        "missing Dropbox removes its row",
        entry(
            &listing_entries(&st(|p| p.dropbox_installed = false)),
            "dropbox",
        )
        .is_none(),
        true,
    );
    check(
        "offline Dropbox remains disabled",
        entry(&listing_entries(&st(|p| p.dropbox_path.clear())), "dropbox")
            .unwrap()
            .disabled,
        true,
    );
    let inside = listing_entries(&st(|p| p.row_in_dropbox = true));
    check(
        "inside Dropbox replaces move with share link",
        (
            entry(&inside, "dropbox").is_none(),
            entry(&inside, "sharelink").is_some(),
        ),
        (true, true),
    );
}

#[test]
fn provider_rows_and_state_words() {
    check(
        "absent Taildrop provider removes its row",
        entry(
            &listing_entries(&st(|p| p.taildrop_installed = false)),
            "taildrop",
        )
        .is_none(),
        true,
    );
    let rows = listing_entries(&st(|p| p.taildrop_peers.clear()));
    let offline = entry(&rows, "taildrop").unwrap();
    check(
        "offline installed Taildrop remains disabled",
        offline.disabled,
        true,
    );
    // GM 2026-10-29: an app row that cannot answer reads red and says no state word, so the menu keeps its size.
    check(
        "offline Taildrop reads as an error with no state word beside it",
        format!("{}|{}", offline.errored, hint(&english(), Some(offline))),
        "true|undefined".into(),
    );
    check(
        "offline Taildrop offers no stale peers",
        offline.submenu.as_ref().unwrap().len(),
        0,
    );
    let reading = listing_entries(&st(|p| p.taildrop_refreshing = true));
    check(
        "fresh status disables a cached Taildrop target",
        entry(&reading, "taildrop").unwrap().disabled,
        true,
    );
    check(
        "a provider still being read is dimmed, not errored",
        entry(&reading, "taildrop").unwrap().errored,
        false,
    );
    check(
        "fresh status disables a cached Dropbox directory",
        entry(
            &listing_entries(&st(|p| p.dropbox_refreshing = true)),
            "dropbox",
        )
        .unwrap()
        .disabled,
        true,
    );
    check(
        "a Taildrop still being read leaves Dropbox as it was answered",
        entry(&reading, "dropbox").unwrap().disabled,
        false,
    );
    check(
        "installed signed-out provider reads as an error",
        entry(&rows, "taildrop").unwrap().errored,
        true,
    );
    let red = |change: &dyn Fn(&mut Context), action: &str| {
        let rows = listing_entries(&st(change));
        let row = entry(&rows, action).unwrap();
        format!(
            "{}|{}|{}",
            row.disabled,
            row.errored,
            hint(&english(), Some(row))
        )
    };
    for code in [
        "signedOut",
        "noPeers",
        "stopped",
        "",
        "invalid",
        "statusFailed",
    ] {
        check(
            "an unanswering Taildrop is red and wordless under every code",
            red(
                &|p| {
                    p.taildrop_peers.clear();
                    p.taildrop_code = code.into()
                },
                "taildrop",
            ),
            "true|true|undefined".into(),
        );
    }
    check(
        "Taildrop on a folder takes no peer, so it is red and wordless with a device answering",
        red(
            &|p| {
                p.row_is_file = false;
                p.row_is_image = false;
                p.row_mode = 0o040755
            },
            "taildrop",
        ),
        "true|true|undefined".into(),
    );
    check(
        "LocalSend with nobody answering is red and wordless",
        red(&|p| p.localsend_installed = true, "localsend"),
        "true|true|undefined".into(),
    );
    check(
        "LocalSend on a folder with a device answering stays live, as v0.3.12 MenuContext.js hands it every peer",
        red(
            &|p| {
                p.localsend_installed = true;
                p.localsend_peers = vec![peer("box", "Box")];
                p.row_is_file = false;
                p.row_is_image = false
            },
            "localsend",
        ),
        "false|false|undefined".into(),
    );
    check(
        "LocalSend still being read is dimmed and says nothing",
        red(
            &|p| {
                p.localsend_installed = true;
                p.localsend_checking = true
            },
            "localsend",
        ),
        "true|false|undefined".into(),
    );
    for code in ["signedOut", "notRunning", "", "readFailed", "statusFailed"] {
        check(
            "an unanswering Dropbox is red and wordless under every code",
            red(
                &|p| {
                    p.dropbox_path.clear();
                    p.dropbox_code = code.into()
                },
                "dropbox",
            ),
            "true|true|undefined".into(),
        );
    }
    check(
        "Share link reads as Dropbox does",
        red(
            &|p| {
                p.dropbox_path.clear();
                p.row_in_dropbox = true;
                p.dropbox_code = "signedOut".into()
            },
            "sharelink",
        ),
        "true|true|undefined".into(),
    );
    check(
        "Dropbox still being read says nothing",
        word(
            |p| {
                p.dropbox_path.clear();
                p.dropbox_refreshing = true
            },
            "dropbox",
        ),
        "undefined".into(),
    );
    let i18n = english();
    let words = ["symlink", "special_file", "not_yours"];
    check(
        "every state word is one short lowercase run with no period",
        words.iter().all(|w| {
            let said = i18n.tr(&format!("menu.state.{w}"));
            let parts: Vec<&str> = said.split(' ').collect();
            parts.len() <= 2
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_lowercase()))
        }),
        true,
    );
}

#[test]
fn permissions_refusals() {
    let i18n = english();
    let r = |mode, count| refusal(&i18n, &permissions_entry(mode, count, &[]));
    check(
        "permissions accepts ordinary file",
        permissions_entry(0o100644, 1, &[]).disabled,
        false,
    );
    check(
        "permissions accepts directory",
        permissions_entry(0o040755, 1, &[]).disabled,
        false,
    );
    check(
        "permissions on a symlink reads as an error with one word",
        r(0o120777, 1),
        "true|true|symlink".into(),
    );
    check(
        "permissions on a row whose mode could not be read names no word",
        r(0, 1),
        "true|true|undefined".into(),
    );
    check(
        "permissions with nothing selected is refused with no kind to name",
        r(0o100644, 0),
        "true|true|undefined".into(),
    );
    check(
        "permissions with nothing selected names no kind even for a link",
        r(0o120777, 0),
        "true|true|undefined".into(),
    );
    check(
        "permissions on a fifo says special file",
        r(0o010644, 1),
        "true|true|special file".into(),
    );
    check(
        "permissions on a socket says special file",
        r(0o140644, 1),
        "true|true|special file".into(),
    );
    check(
        "and a file's row can act, so it is not red",
        r(0o100644, 1),
        "false|false|undefined".into(),
    );
    let perms = |change: &dyn Fn(&mut Context)| {
        row_refusal(
            &i18n,
            entry(
                &listing_entries(&st(|p| {
                    p.hidden.clear();
                    change(p)
                })),
                "permissions",
            ),
        )
    };
    check(
        "the menu row under a parent without execute reads red with no word",
        perms(&|p| p.row_mode = 0),
        "true|true|undefined".into(),
    );
    check(
        "the menu row for a multi-selection reads plain on files",
        perms(&|p| {
            p.selection_count = 2;
            p.selection_modes = vec![0o100644, 0o100644]
        }),
        "false|false|undefined".into(),
    );
    check(
        "and red when one of them is a link",
        perms(&|p| {
            p.selection_count = 2;
            p.selection_modes = vec![0o100644, 0o120777]
        }),
        "true|true|symlink".into(),
    );
    check(
        "and red with no word when one of them could not be read",
        perms(&|p| {
            p.selection_count = 2;
            p.selection_modes = vec![0o100644, 0]
        }),
        "true|true|undefined".into(),
    );
    check(
        "the menu row for a symlink reads the same way",
        perms(&|p| p.row_mode = 0o120777),
        "true|true|symlink".into(),
    );
    check(
        "the menu row for a folder it can change is plain",
        perms(&|p| p.row_mode = 0o040755),
        "false|false|undefined".into(),
    );
    check(
        "permissions rejects missing metadata",
        permissions_entry(0, 1, &[]).disabled,
        true,
    );
    check(
        "permissions rejects fifo",
        permissions_entry(0o010644, 1, &[]).disabled,
        true,
    );
    check(
        "permissions allows read-only inspection of special bits",
        permissions_entry(0o104755, 1, &[]).disabled,
        false,
    );
    check(
        "hidden files toggle uses inverse wording",
        i18n.tr(hidden_row(true).0),
        "Hide Hidden Files".into(),
    );
}

#[test]
fn clamp_flyouts_and_steps() {
    check("menu fits at pointer", clamp(20., 80., 300.), 20.);
    check("menu flips before shifting", clamp(270., 80., 300.), 190.);
    check(
        "oversized menu pins to near edge",
        clamp(30., 500., 300.),
        0.,
    );
    check("negative point clamps", clamp(-20., 80., 300.), 0.);
    check(
        "submenu array is recognized even when empty",
        Entry {
            submenu: Some(Vec::new()),
            ..Entry::default()
        }
        .has_submenu(),
        true,
    );
    check(
        "missing entry has no submenu",
        entry(&[], "open").is_some_and(Entry::has_submenu),
        false,
    );
    check(
        "ordinary entry has no submenu",
        Entry::row("open", Text::Key("menu.row.open"), "folder-open").has_submenu(),
        false,
    );
    let i18n = english();
    check(
        "header keeps required Name column outside toggles",
        header_entries(&[], false)
            .iter()
            .filter(|r| !r.separator)
            .map(|r| r.action.to_string())
            .collect::<Vec<_>>()
            .join(","),
        "col:mode,col:size,col:date,col:kind,toggleHidden".into(),
    );
    check(
        "cold header uses its hidden-column state",
        label(
            &i18n,
            entry(&header_entries(&strings(&["size"]), false), "col:size"),
        ),
        "Show Size".into(),
    );
    check(
        "sort submenu uses real backend order ids",
        sort_entries(false, "")
            .iter()
            .map(|r| r.id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        "name,size,mtime,kind".into(),
    );
    check(
        "Sort By checks the order in effect and no other",
        sort_entries(false, "size")
            .iter()
            .map(|r| r.check == Some(true))
            .collect::<Vec<_>>(),
        vec![false, true, false, false],
    );
    let hidden_rows = listing_entries(&st(|p| p.clipboard_available = true));
    check(
        "the shipped defaults hide the Copy as row",
        entry(&hidden_rows, "copyAs").is_none(),
        true,
    );
    check(
        "and the Paste as row",
        entry(&hidden_rows, "pasteAs").is_none(),
        true,
    );
    check(
        "yet Copy as still builds its flyout from the action",
        flyout_entries("copyAs")
            .iter()
            .map(|l| l.id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        "copyPath,copyRelative,copyName,copyStem,copydirpath,copyUri,copyQuoted".into(),
    );
    check(
        "and Paste as builds its own",
        flyout_entries("pasteAs")
            .iter()
            .map(|l| l.id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        "pasteLink,pasteAbsoluteLink,pasteHardLink".into(),
    );
    check(
        "while any other action builds no flyout",
        flyout_entries("trash").len(),
        0,
    );
    check(
        "hidden Copy as with clipboard opens lone",
        submenu_for("copyAs", &hidden_rows, true),
        Submenu::Lone,
    );
    check(
        "hidden Paste as with clipboard opens lone",
        submenu_for("pasteAs", &hidden_rows, true),
        Submenu::Lone,
    );
    check(
        "hidden Paste as with empty clipboard refuses",
        submenu_for("pasteAs", &listing_entries(&state()), false),
        Submenu::Refuse,
    );
    check(
        "and names the empty-clipboard sentence",
        i18n.tr(EMPTY_CLIPBOARD),
        "There is nothing to paste; y copies and x cuts.".into(),
    );
    // 0.4.0 hides Paste As on an empty clipboard, so the shown case meets no row and refuses the same way.
    check(
        "shown Paste as with empty clipboard refuses",
        submenu_for(
            "pasteAs",
            &listing_entries(&st(|p| p.hidden.clear())),
            false,
        ),
        Submenu::Refuse,
    );
    let full = listing_entries(&st(|p| {
        p.clipboard_available = true;
        p.hidden.clear()
    }));
    check(
        "shown Paste as with clipboard opens its row",
        matches!(submenu_for("pasteAs", &full, true), Submenu::Row(_)),
        true,
    );
    check(
        "shown Copy as opens its row",
        matches!(submenu_for("copyAs", &full, true), Submenu::Row(_)),
        true,
    );
    let tags_row = vec![Entry {
        submenu: Some(vec![Entry::leaf("tag1", Text::Key("menu.row.tags"))]),
        ..Entry::row("tags", Text::Key("menu.row.tags"), "tag")
    }];
    check(
        "a Tags row opens the leaves it carries",
        submenu_for("tags", &tags_row, true),
        Submenu::Row(0),
    );
    check(
        "an action with no flyout opens nothing",
        submenu_for("trash", &full, true),
        Submenu::None,
    );
    check(
        "a known leaf with no move fires",
        lone_choice("copyAs", "copyPath", false, false, true, "a", "a"),
        Lone::Fire("copyAs:copyPath".into()),
    );
    check(
        "and names the fired action with its leaf",
        matches!(lone_choice("copyAs", "copyPath", false, false, true, "a", "a"), Lone::Fire(ref fired) if fired == "copyAs:copyPath"),
        true,
    );
    check(
        "an unknown leaf refuses",
        lone_choice("copyAs", "bogus", false, false, true, "a", "a"),
        Lone::Unknown,
    );
    check(
        "a moved selection refuses",
        lone_choice("copyAs", "copyPath", false, false, true, "a", "b"),
        Lone::Moved,
    );
    check(
        "a moved selection outranks an unknown leaf",
        lone_choice("copyAs", "bogus", false, false, true, "a", "b"),
        Lone::Moved,
    );
    check(
        "a rail move never counts as moved",
        lone_choice("copyAs", "copyPath", true, false, true, "a", "b"),
        Lone::Fire("copyAs:copyPath".into()),
    );
    let steps = vec![
        Entry::row("open", Text::Key("menu.row.open"), "folder-open"),
        Entry::separator(),
        Entry {
            disabled: true,
            ..Entry::row("cut", Text::Key("menu.row.cut"), "cut")
        },
        Entry::row("copy", Text::Key("menu.row.copy"), "copy"),
    ];
    check(
        "a step skips a separator and a disabled row",
        step_row(&steps, 0, 1),
        3,
    );
    check("and back again", step_row(&steps, 3, -1), 0);
    check(
        "an end keeps the cursor where it is",
        (step_row(&steps, 3, 1), step_row(&steps, 0, -1)),
        (3, 0),
    );
    check(
        "the opening cursor is the first row a step from before the top reaches",
        step_row(&steps[1..], -1, 1),
        2,
    );
}

fn leaves(ids: &[&str]) -> Vec<Entry> {
    ids.iter()
        .map(|id| Entry::leaf(id.to_string(), Text::Data(id.to_string())))
        .collect()
}

fn with_sub(action: &'static str, ids: &[&str]) -> Entry {
    Entry {
        submenu: Some(leaves(ids)),
        ..Entry::row(action, Text::Key("menu.row.taildrop"), "send")
    }
}

fn drawn(known: Option<&Settled>, live: &Live) -> Vec<Entry> {
    let view = provider_view(known, live);
    listing_entries(&st(|p| {
        p.taildrop_peers = view.taildrop_peers;
        p.taildrop_refreshing = view.taildrop_refreshing;
        p.dropbox_path = view.dropbox_path;
        p.dropbox_refreshing = view.dropbox_refreshing;
        p.localsend_installed = true;
        p.localsend_peers = vec![peer("Phone", "Phone")];
        p.localsend_checking = view.localsend_checking;
    }))
}

fn moved(before: &[Entry], after: &[Entry]) -> String {
    before
        .iter()
        .zip(after)
        .filter(|(a, b)| a != b)
        .map(|(a, _)| a.action.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn refresh_keeps_rows_and_cursor() {
    let absent = listing_entries(&st(|p| {
        p.taildrop_installed = false;
        p.dropbox_installed = false
    }));
    let available = listing_entries(&state());
    let trash = absent.iter().position(|r| r.action == "trash").unwrap();
    let refreshed = refreshed_cursor(&absent, &available, trash, None, 0);
    check(
        "new provider inventory inserts both actual providers",
        available
            .iter()
            .filter(|r| r.action == "taildrop" || r.action == "dropbox")
            .count(),
        2,
    );
    check(
        "new providers do not steal the current action",
        available[refreshed.cursor].action.as_ref(),
        "trash",
    );
    let back = refreshed_cursor(&available, &absent, refreshed.cursor, None, 0);
    check(
        "removed providers leave the existing action selected",
        absent[back.cursor].action.as_ref(),
        "trash",
    );
    check(
        "removed providers are absent from fresh inventory",
        absent
            .iter()
            .filter(|r| r.action == "taildrop" || r.action == "dropbox")
            .count(),
        0,
    );
    let old = vec![with_sub("taildrop", &["b"])];
    let reordered = refreshed_cursor(&old, &[with_sub("taildrop", &["a", "b"])], 0, Some(0), 0);
    check(
        "a submenu keeps its target id when peers reorder",
        reordered.submenu_cursor,
        1,
    );
    check(
        "a retained submenu keeps its parent action",
        reordered.submenu_row,
        Some(0),
    );
    check(
        "a vanished peer closes rather than retargeting the submenu",
        refreshed_cursor(&old, &[with_sub("taildrop", &["a"])], 0, Some(0), 0).submenu_row,
        None,
    );
    let next = vec![
        Entry::separator(),
        Entry {
            disabled: true,
            ..Entry::row("copy", Text::Key("menu.row.copy"), "copy")
        },
        Entry::row("open", Text::Key("menu.row.open"), "folder-open"),
    ];
    check(
        "a removed action selects an eligible row rather than a separator",
        refreshed_cursor(&[with_sub("taildrop", &[])], &next, 0, None, 0).cursor,
        2,
    );
    let answer = Live {
        refreshing: false,
        taildrop_installed: true,
        taildrop_peers: vec![peer("box", "Box")],
        dropbox_installed: true,
        dropbox_path: "/tmp/Dropbox".into(),
        localsend_checking: false,
        localsend_answered: true,
    };
    let known = settle(&answer);
    let before = drawn(Some(&known), &answer);
    let reading = Live {
        refreshing: true,
        taildrop_peers: Vec::new(),
        dropbox_path: String::new(),
        localsend_checking: true,
        ..answer.clone()
    };
    let during = drawn(Some(&known), &reading);
    check(
        "a refresh behind the open menu builds exactly the rows drawn",
        before == during,
        true,
    );
    check(
        "so Taildrop keeps its peer and stays live",
        (
            entry(&during, "taildrop").unwrap().disabled,
            entry(&during, "taildrop")
                .unwrap()
                .submenu
                .as_ref()
                .unwrap()
                .len(),
        ),
        (false, 1),
    );
    check(
        "Dropbox keeps its folder",
        entry(&during, "dropbox").unwrap().disabled,
        false,
    );
    check(
        "and LocalSend stays live while it looks again",
        entry(&during, "localsend").unwrap().disabled,
        false,
    );
    check(
        "the same answer landing rebuilds nothing",
        before == drawn(Some(&settle(&answer)), &answer),
        true,
    );
    let more = Live {
        taildrop_peers: vec![peer("box", "Box"), peer("laptop", "Laptop")],
        ..answer.clone()
    };
    let after = drawn(Some(&settle(&more)), &more);
    check("a changed answer is a change", before == after, false);
    check(
        "and it moves only the Taildrop row",
        moved(&before, &after),
        "taildrop".into(),
    );
    check(
        "which now offers both peers",
        entry(&after, "taildrop")
            .unwrap()
            .submenu
            .as_ref()
            .unwrap()
            .iter()
            .map(|p| p.id.to_string())
            .collect::<Vec<_>>()
            .join(","),
        "box,laptop".into(),
    );
    let gone = Live {
        dropbox_path: String::new(),
        ..answer.clone()
    };
    let after = drawn(Some(&settle(&gone)), &gone);
    check(
        "a Dropbox that stopped answering reads red, and only its row moves",
        format!(
            "{}|{}",
            moved(&before, &after),
            entry(&after, "dropbox").unwrap().errored
        ),
        "dropbox|true".into(),
    );
    let first = drawn(
        None,
        &Live {
            localsend_answered: false,
            ..reading.clone()
        },
    );
    let dim = |a: &str| {
        (
            entry(&first, a).unwrap().disabled,
            entry(&first, a).unwrap().errored,
        )
    };
    check(
        "with nothing answered yet the first refresh dims Taildrop, as it always did",
        dim("taildrop"),
        (true, false),
    );
    check("and Dropbox", dim("dropbox"), (true, false));
    check("and LocalSend", dim("localsend"), (true, false));
    let reinstalled = drawn(
        Some(&settle(&Live {
            taildrop_installed: false,
            taildrop_peers: Vec::new(),
            ..answer.clone()
        })),
        &reading,
    );
    check(
        "a provider installed since its last answer is read afresh, dimmed rather than red",
        (
            entry(&reinstalled, "taildrop").unwrap().disabled,
            entry(&reinstalled, "taildrop").unwrap().errored,
        ),
        (true, false),
    );
}

#[test]
fn choices_validate_against_live_rows() {
    let i18n = english();
    let live = listing_entries(&st(|p| p.hidden = strings(&["trash"])));
    check(
        "cold ordinary validation refuses a hidden row",
        validate_choice(&live, "trash", "", false),
        Err(super::refresh::Refusal::Unavailable),
    );
    check(
        "cold sheet validation allows its hidden copy row",
        validate_choice(
            &listing_entries(&st(|p| p.hidden.clear())),
            "trash",
            "",
            false,
        ),
        Ok(()),
    );
    let all = listing_entries(&st(|p| p.hidden.clear()));
    check(
        "cold flyout validation allows a known leaf",
        validate_choice(&all, "copyAs", "copyPath", false),
        Ok(()),
    );
    check(
        "cold validation refuses an unknown leaf",
        validate_choice(&all, "copyAs", "missing", false),
        Err(super::refresh::Refusal::Unavailable),
    );
    check(
        "cold validation refuses disabled Paste",
        validate_choice(&all, "paste", "", false),
        Err(super::refresh::Refusal::Unavailable),
    );
    check(
        "cold validation refuses a moved selection",
        validate_choice(&all, "copy", "", true),
        Err(super::refresh::Refusal::Moved),
    );
    check(
        "lone copyPath refuses with its named reason",
        i18n.tr(super::refresh::Refusal::Moved.key()),
        "Selected item moved or disappeared; reopen the menu.".into(),
    );
    check(
        "lone unknown refuses with its named reason",
        i18n.tr(super::refresh::Refusal::Unavailable.key()),
        "That action is no longer available; reopen the menu.".into(),
    );
    check(
        "cold header validation ignores row identity",
        validate_choice(
            &header_entries(&strings(&["size"]), false),
            "col:size",
            "",
            false,
        ),
        Ok(()),
    );
}

#[test]
fn a_step_skips_the_busy_row_that_says_the_menu_is_still_asking() {
    let busy = Entry { busy: true, ..Entry::leaf("nearbyLooking", Text::Key("network.looking_hosts")) };
    let rows = [busy, Entry::separator(), Entry::row("addShare", Text::Key("network.add_share"), "plus")];
    assert!(!rows[0].steppable() && !rows[0].disabled && !rows[0].separator);
    assert_eq!(step_row(&rows, 2, -1), 2, "the busy row is never the cursor, so Up keeps Add");
    assert_eq!(step_row(&rows, -1, 1), 2);
}
