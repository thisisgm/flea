//! Multi-item Properties acceptance through the card's real requests and reply routing.
use super::*;
use crate::g2::open_with_properties::tests::{buttons, english, notes, sent};
use flea::jsondoc::parse;

const SUMMARY: &str = r#"{"t":"menuaction","op":"properties","id":9,"ok":true,"kinds":"2 images, 1 video","bytes":52300000,"newest":"2026-09-23 10:14","oldest":"2026-08-21 12:02","partial":false,"done":true}"#;

fn facts(card: &Card) -> Vec<(&str, &str, &str)> {
    card.body
        .iter()
        .flat_map(|block| match block {
            Block::Facts(facts) => facts.as_slice(),
            _ => &[],
        })
        .filter_map(|fact| match fact {
            Fact::Text { name, value, tail } => {
                Some((name.as_str(), value.as_str(), tail.as_str()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn bfx_172_1_properties_first_size_never_shows_the_directory_entry() {
    let i18n = english();
    let (mut properties, _) = Properties::open(9, 1, &[0], 0);
    let effects = properties.reply(
        &i18n,
        &parse(r#"{"op":"properties","id":9,"ok":true,"directory":true,"bytes":4096}"#).unwrap(),
    );
    assert_eq!(
        sent(&effects),
        [r#"{"c":"menuaction","op":"contents","id":9}"#]
    );
    let card = properties.card(&i18n);
    assert_eq!(
        facts(&card)[2],
        ("Size", "Unknown", " \u{b7} counting\u{2026}")
    );
    properties.reply(&i18n, &parse(r#"{"op":"contents","id":9,"ok":true,"bytes":13,"items":1,"done":true,"partial":false}"#).unwrap());
    assert_eq!(
        facts(&properties.card(&i18n))[2],
        ("Size", "13 B (13 bytes)", "")
    );
}

#[test]
fn bfx_172_2_properties_partial_zero_settles_unknown_but_exact_zero_is_empty() {
    let i18n = english();
    for partial in [true, false] {
        let (mut properties, _) = Properties::open(9, 1, &[0], 0);
        properties.reply(
            &i18n,
            &parse(r#"{"op":"properties","id":9,"ok":true,"directory":true,"bytes":4096}"#)
                .unwrap(),
        );
        properties.reply(&i18n, &parse(&format!(r#"{{"op":"contents","id":9,"ok":true,"bytes":0,"items":0,"done":true,"partial":{partial}}}"#)).unwrap());
        let card = properties.card(&i18n);
        assert_eq!(
            facts(&card)[2],
            (
                "Size",
                if partial { "Unknown" } else { "0 B (0 bytes)" },
                ""
            )
        );
    }
}

#[test]
fn m9_g2rows_05_three_item_properties_uses_aggregate() {
    let i18n = english();
    let (mut properties, _) = Properties::open(9, 1, &[0, 2, 4], 0);
    assert!(!properties.ready());
    properties.set_extras(vec![("Camera".into(), "cursor photo".into())], "JPEG image");
    assert!(properties.reply(&i18n, &parse(SUMMARY).unwrap()).is_empty());
    assert!(properties.ready());
    let card = properties.card(&i18n);
    assert_eq!(
        facts(&card),
        [
            ("Kinds", "2 images, 1 video", ""),
            ("Combined", "52.3 MB", ""),
            ("Newest", "2026-09-23 10:14", ""),
            ("Oldest", "2026-08-21 12:02", ""),
        ]
    );
    assert_eq!(card.title, "3 items");
    assert_eq!(card.width, 552.);
    assert_eq!(buttons(&card), "Close*@");
    assert!(notes(&card).is_empty());
}

#[test]
fn m9_g2rows_05_captured_selection_survives_cursor_change() {
    let i18n = english();
    let mut selection = vec![0, 2, 4];
    let (mut properties, effects) = Properties::open(9, 1, &selection, 4);
    selection.clear();
    selection.push(7);
    assert_eq!(
        sent(&effects),
        [r#"{"c":"menuaction","op":"snapshot","id":9,"listing":1,"rows":[0,2,4],"cursor":4}"#]
    );
    let acknowledged = parse(r#"{"t":"menuaction","op":"snapshot","id":9,"ok":true}"#).unwrap();
    assert_eq!(
        sent(&properties.reply(&i18n, &acknowledged)),
        [r#"{"c":"menuaction","op":"properties","id":9}"#]
    );
    let pending = properties.card(&i18n);
    let other = parse(&SUMMARY.replace("\"id\":9", "\"id\":10")).unwrap();
    assert!(properties.reply(&i18n, &other).is_empty());
    assert_eq!(properties.card(&i18n), pending);
    properties.reply(&i18n, &parse(SUMMARY).unwrap());
    assert_eq!(facts(&properties.card(&i18n))[0].1, "2 images, 1 video");
    assert_eq!(
        properties.card(&i18n).title,
        "3 items",
        "the count is the selection open captured, not the live one"
    );

    let mut captured = captured();
    let (mut properties, effects) = Properties::from_captured(&captured).unwrap();
    captured.rows = selection;
    captured.cursor = 7;
    captured.side = 0;
    captured.id = 10;
    assert_eq!(
        sent(&effects),
        [r#"{"c":"menuaction","op":"properties","id":9}"#],
        "the real host adapter reuses the acknowledged snapshot, never recaptures live rows"
    );
    assert!(properties.reply(&i18n, &other).is_empty());
    properties.reply(&i18n, &parse(SUMMARY).unwrap());
    assert_eq!(properties.card(&i18n).title, "3 items");
    assert_eq!(facts(&properties.card(&i18n))[0].1, "2 images, 1 video");
}

#[test]
fn m9_g2rows_05_refused_snapshot_says_why_and_never_asks() {
    let i18n = english();
    let (mut properties, _) = Properties::open(9, 1, &[0], 0);
    let refused = parse(r#"{"t":"menuaction","op":"snapshot","id":9,"ok":false,"error":"The folder changed; reopen the menu."}"#).unwrap();
    assert!(
        properties.reply(&i18n, &refused).is_empty(),
        "a refused snapshot asks no facts"
    );
    let why: Vec<(String, Ink)> = vec![("The folder changed; reopen the menu.".into(), Ink::Error)];
    assert_eq!(notes(&properties.card(&i18n)), why);
    let late = parse(r#"{"t":"menuaction","op":"snapshot","id":9,"ok":true}"#).unwrap();
    assert!(
        properties.reply(&i18n, &late).is_empty(),
        "a later acknowledgement cannot ask either"
    );
    assert_eq!(
        notes(&properties.card(&i18n)),
        why,
        "a later acknowledgement keeps the reason on the card"
    );
}

#[test]
fn m9_g2rows_05_close_cancels_walk_and_ignores_late_reply() {
    let i18n = english();
    for rows in [vec![0], vec![0, 2, 4]] {
        let (mut properties, _) = Properties::open(9, 1, &rows, 0);
        let first = if rows.len() == 1 {
            r#"{"op":"properties","id":9,"ok":true,"directory":true,"bytes":4096}"#
        } else {
            r#"{"op":"properties","id":9,"ok":true,"kinds":"1 folder, 2 images","bytes":4096,"done":false}"#
        };
        let effects = properties.reply(&i18n, &parse(first).unwrap());
        assert_eq!(
            sent(&effects).len(),
            usize::from(rows.len() == 1),
            "only a single folder starts a separate contents walk"
        );
        let before = properties.card(&i18n);
        assert_eq!(
            properties.key(Key::Escape),
            [
                Effect::Send(r#"{"c":"menuaction","op":"close","id":9}"#.into()),
                Effect::Close,
            ]
        );
        assert!(!properties.ready());
        for late in [
            SUMMARY,
            r#"{"op":"contents","id":9,"ok":true,"bytes":9000,"items":4,"done":true}"#,
            r#"{"op":"properties","id":9,"ok":false,"error":"Selected items changed; reopen the menu."}"#,
        ] {
            assert!(properties.reply(&i18n, &parse(late).unwrap()).is_empty());
            assert_eq!(properties.card(&i18n), before, "closed cards hold still");
        }
        properties.set_extras(vec![("Camera".into(), "late photo".into())], "JPEG image");
        assert_eq!(properties.card(&i18n), before);
    }
}

fn captured() -> crate::g2::context_menus::join::Captured {
    crate::g2::context_menus::join::Captured {
        side: 1,
        listing: 12,
        rows: vec![0, 2, 4],
        cursor: 4,
        path: "/fixture".into(),
        identity: "held selection".into(),
        id: 9,
        ready: true,
        kinds: vec![0o100000; 3],
        background: false,
        target: None,
    }
}

#[test]
fn m9_g2rows_05_host_adapter_requires_an_acknowledged_nonempty_snapshot() {
    let mut selection = captured();
    selection.ready = false;
    assert!(Properties::from_captured(&selection).is_none());
    selection.ready = true;
    selection.rows.clear();
    assert!(Properties::from_captured(&selection).is_none());
    selection.rows.push(0);
    selection.id = 0;
    assert!(Properties::from_captured(&selection).is_none());
}

#[test]
fn m9_g2rows_05_mixed_folder_progress_and_partial_final_keep_dates() {
    let i18n = english();
    let (mut properties, _) = Properties::from_captured(&captured()).unwrap();
    for (bytes, done, partial, combined, tail) in [
        (4096, false, false, "> 4.1 kB", " \u{b7} counting\u{2026}"),
        (52300000, true, true, "> 52.3 MB", ""),
    ] {
        let reply = parse(&format!(
            r#"{{"op":"properties","id":9,"ok":true,"kinds":"1 folder, 1 image, 1 video","bytes":{bytes},"newest":"2026-09-23 10:14","oldest":"2026-08-21 12:02","done":{done},"partial":{partial}}}"#
        ))
        .unwrap();
        assert!(properties.reply(&i18n, &reply).is_empty());
        let card = properties.card(&i18n);
        assert_eq!(
            facts(&card),
            [
                ("Kinds", "1 folder, 1 image, 1 video", ""),
                ("Combined", combined, tail),
                ("Newest", "2026-09-23 10:14", ""),
                ("Oldest", "2026-08-21 12:02", ""),
            ]
        );
        assert!(properties.ready());
    }
}

#[test]
fn m9_g2rows_05_refusal_is_localized_and_cancellation_is_terminal() {
    let i18n = english();
    let (mut refused, _) = Properties::from_captured(&captured()).unwrap();
    refused.reply(&i18n, &parse(r#"{"op":"properties","id":9,"ok":false,"error":"Selected items changed; reopen the menu."}"#).unwrap());
    assert!(refused.ready());
    assert_eq!(
        notes(&refused.card(&i18n)),
        [(
            i18n.tr("dialogs.file.properties.err_items_changed"),
            Ink::Error
        )]
    );
    let (mut cancelled, _) = Properties::from_captured(&captured()).unwrap();
    assert_eq!(
        cancelled.reply(&i18n, &parse(r#"{"op":"properties","id":9,"ok":true,"cancelled":true,"partial":true,"done":false}"#).unwrap()),
        [Effect::Close]
    );
    let before = cancelled.card(&i18n);
    assert!(!cancelled.ready());
    assert!(cancelled.reply(&i18n, &parse(SUMMARY).unwrap()).is_empty());
    assert_eq!(cancelled.card(&i18n), before);
}

#[test]
fn m9_g2rows_05_close_button_and_keys_cancel_the_same_owned_snapshot() {
    let (mut clicked, _) = Properties::from_captured(&captured()).unwrap();
    let click = clicked.close();
    assert_eq!(
        click,
        [
            Effect::Send(r#"{"c":"menuaction","op":"close","id":9}"#.into()),
            Effect::Close,
        ]
    );
    for key in [Key::Escape, Key::Enter, Key::Space] {
        let (mut keyed, _) = Properties::from_captured(&captured()).unwrap();
        assert_eq!(keyed.key(key), click);
        assert!(!keyed.ready());
    }
}
