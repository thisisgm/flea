// The root helpers, the wire parser, the Jobs registry and the card rhythm.
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::*;

#[test]
fn parts_split_figures_from_words() {
    let got = parts("%1 of %2 · %3", &["1 kB", "2 kB", "3 kB/s"]);
    assert_eq!(joined(&got), "1 kB of 2 kB · 3 kB/s");
    assert_eq!(got.iter().filter(|p| p.figure).count(), 3);
    assert!(!got[1].figure);
    assert_eq!(
        parts("%9 left", &["x"]),
        vec![Part {
            text: "%9 left".into(),
            figure: false
        }]
    );
}

#[test]
fn duration_reads_minutes_then_hours() {
    assert_eq!(duration(4), "0:04");
    assert_eq!(duration(1000), "16:40");
    assert_eq!(duration(8088), "2:14:48");
}

#[test]
fn english_catalog_fills_counts() {
    let cat = Catalog::english();
    assert_eq!(lines::items(&cat.i18n, 1), "1 item");
    assert_eq!(lines::items(&cat.i18n, 1204), "1,204 items");
    assert_eq!(undo_hint(&cat.i18n, "default"), " · z undoes");
}

fn bound(preset: &str, key: &str, text: &str, modifiers: u8) -> &'static str {
    use crate::keys::{lookup, Context, Frontend, Input};
    lookup(
        preset,
        Context::Listing,
        Frontend::Gui,
        Input {
            key,
            text,
            modifiers,
        },
    )
    .action()
}

// The operations rows the presets bind, each answered by the table the window dispatches from.
#[test]
fn every_operations_binding_reaches_its_action() {
    use crate::keys::{CONTROL, SHIFT, SUPER};
    // oracle keys.toml:385
    assert_eq!(bound("default", "delete", "", 0), "trash");
    // oracle keys.toml:32
    assert_eq!(bound("default", "z", "", CONTROL), "undo");
    // oracle keys.toml:1158
    assert_eq!(bound("default", "delete", "", SHIFT), "deletePermanently");
    // oracle keys.toml:1462
    assert_eq!(bound("default", "z", "", CONTROL | SHIFT), "redo");
    // oracle keys.toml:274
    assert_eq!(bound("mac", "delete", "", CONTROL), "trash");
    // oracle keys.toml:1441
    assert_eq!(bound("mac", "d", "", SUPER), "duplicate");
    // oracle keys.toml:1451
    assert_eq!(bound("mac", "z", "", SUPER), "undo");
    // oracle keys.toml:1472
    assert_eq!(bound("mac", "z", "", SUPER | SHIFT), "redo");
    // oracle keys.toml:1331
    assert_eq!(bound("vim", "d", "D", SHIFT), "trash");
    // oracle keys.toml:1341
    assert_eq!(bound("vim", "u", "u", 0), "undo");
    // oracle keys.toml:1552
    assert_eq!(bound("windows", "d", "", CONTROL), "trash");
    // oracle keys.toml:1562
    assert_eq!(bound("windows", "y", "", CONTROL), "redo");
    // oracle keys.toml:525
    assert_eq!(bound("default", "d", "d", 0), "trashArm");
    // oracle keys.toml:536
    assert_eq!(bound("default", "z", "z", 0), "undo");
    // The sheet names the pair and the undo keys the way the window draws them.
    let keys_of = |action: &str| {
        crate::keys::BASE_SHEET
            .iter()
            .find(|row| row.action == action)
            .map(|row| row.keys.to_string())
    };
    // oracle keys.toml:768
    assert_eq!(keys_of("trashArm").as_deref(), Some("dd"));
    // oracle keys.toml:773
    assert_eq!(keys_of("undo").as_deref(), Some("z ^z"));
    // A key no row binds acts on nothing.
    assert_eq!(bound("default", "q", "q", SUPER | CONTROL), "");
}

// A translated catalog replaces the whole sentence, figures still filled, and a missing key falls back to English.
#[test]
fn a_translation_replaces_whole_sentences() {
    let mut cat = Catalog::with_language("de");
    cat.write_catalog("de", r#"{"jobs.transfer.copying": "Kopiere %1 von %2"}"#);
    let t = transfer::Transfer {
        n: 1204,
        index: 203,
        ..Default::default()
    };
    assert_eq!(t.head(&cat.i18n), "Kopiere 204 von 1.204");
    assert_eq!(
        lines::trashed(&cat.i18n, "default", 0, 1, ""),
        "That item could not be moved to Trash."
    );
}

// A FAT rename keeps the item's kind and icon, so its row draws the item's own mark.
#[test]
fn fat_renames_carry_the_mark_of_each_item() {
    let line = r#"{"t":"fatnames","id":7,"fstype":"exfat","volume":"128GB","renames":[{"old":"clip: x.mp4","new":"clip_ x.mp4","d":false,"i":"video-x-generic"},{"old":"a:b","new":"a_b","d":true}]}"#;
    let Some(Event::FatNames { renames, .. }) = parse(line) else {
        panic!("not a fatnames line")
    };
    assert_eq!(
        renames[0],
        Rename {
            old: "clip: x.mp4".into(),
            new: "clip_ x.mp4".into(),
            dir: false,
            icon: "video-x-generic".into()
        }
    );
    assert_eq!((renames[1].dir, renames[1].icon.as_str()), (true, ""));
    // Negative: a rename without its new name is not a fatnames line at all.
    assert_eq!(
        parse(r#"{"t":"fatnames","id":7,"renames":[{"old":"a:b"}]}"#),
        None
    );
}
