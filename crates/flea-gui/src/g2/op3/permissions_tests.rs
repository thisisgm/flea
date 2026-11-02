//! The Permissions card: the boards' lines and focus states, Apply and its lock, backend death, several items and a failed batch.
use super::keys::Key;
use super::permissions::{Permissions, Stop as PermStop};
use super::shell::{Block, Card, Ink, Tri};
use super::tests::{buttons, english, notes, permissions, sent};
use super::Effect;
use flea::jsondoc::parse;

fn stack(card: &Card) -> Vec<Block> {
    card.body
        .iter()
        .find_map(|b| match b {
            Block::Stack(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap()
}

fn grid_checks(blocks: &[Block]) -> Vec<(Tri, bool)> {
    blocks
        .iter()
        .find_map(|b| match b {
            Block::Grid { rows, .. } => Some(
                rows.iter()
                    .flat_map(|(_, c)| c.iter().map(|c| (c.value, c.focused)))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap()
}

#[test]
fn permissions_open_with_apply_focused_over_the_boards_lines() {
    let i18n = english();
    let card = permissions().card(&i18n);
    assert_eq!((card.title.as_str(), card.width), ("Permissions", 480.));
    assert_eq!(
        buttons(&card),
        "Cancel|Apply*@",
        "Apply, the default, is focused at open"
    );
    let parts = stack(&card);
    assert_eq!(
        parts[0],
        Block::Subject {
            glyph: super::shell::Glyph::File,
            name: "notes.txt".into(),
            kind: "Plain text document".into()
        }
    );
    let Block::Grid { columns, rows } = &parts[2] else {
        panic!()
    };
    assert_eq!(columns.join("|"), "Read|Write|Execute");
    assert_eq!(
        rows.iter()
            .map(|r| r.0.as_str())
            .collect::<Vec<_>>()
            .join("|"),
        "Owner|Group|Everyone"
    );
    use Tri::{Off, On};
    assert_eq!(
        grid_checks(&parts).iter().map(|c| c.0).collect::<Vec<_>>(),
        vec![On, On, Off, On, Off, Off, On, Off, Off],
        "0644"
    );
    let Block::Field(octal) = &parts[4] else {
        panic!()
    };
    assert_eq!(
        (octal.label.as_deref(), octal.value.as_str()),
        (Some("Octal"), "0644")
    );
    assert_eq!(
        parts[5],
        Block::Info {
            label: "Owner".into(),
            value: "gm \u{b7} read-only".into(),
            tail: "uid 1000".into()
        }
    );
    assert_eq!(
        parts[6],
        Block::Info {
            label: "Group".into(),
            value: "gm \u{b7} read-only".into(),
            tail: "gid 1000".into()
        }
    );
    assert_eq!(
        parts[8],
        Block::Section {
            head: "Will change".into(),
            lines: vec![
                "Requested mode 0644".into(),
                "Path /fx/v/work/notes.txt".into(),
                "Scope this item only \u{b7} ownership unchanged".into()
            ]
        }
    );
}

#[test]
fn permissions_focus_states_follow_the_close_check_and_checked_boards() {
    let i18n = english();
    let mut card = permissions();
    card.key(Key::Backtab);
    assert_eq!(
        buttons(&card.card(&i18n)),
        "Cancel@|Apply*",
        "permissions-close"
    );
    card.key(Key::Tab);
    card.key(Key::Tab);
    assert_eq!(
        card.focus,
        PermStop::Box(0),
        "the walk comes round to the first box"
    );
    assert_eq!(
        grid_checks(&stack(&card.card(&i18n)))[0],
        (Tri::On, true),
        "permissions-checked"
    );
    card.key(Key::Right);
    card.key(Key::Right);
    assert_eq!(
        grid_checks(&stack(&card.card(&i18n)))[2],
        (Tri::Off, true),
        "permissions-check"
    );
    assert_eq!(
        buttons(&card.card(&i18n)),
        "Cancel|Apply*",
        "no button holds the ring while a box does"
    );
    card.key(Key::Space);
    assert_eq!(card.text, "0744");
    assert!(
        matches!(&stack(&card.card(&i18n))[8], Block::Section { lines, .. } if lines[0] == "Requested mode 0744")
    );
}

#[test]
fn permissions_apply_closes_with_a_status_line_and_a_reread() {
    let i18n = english();
    let mut card = permissions();
    assert_eq!(
        sent(&card.key(Key::Enter)),
        vec![r#"{"c":"permissions","op":"apply","id":7,"mode":"0644"}"#]
    );
    assert!(
        card.key(Key::Enter).is_empty(),
        "Apply is locked while it runs"
    );
    let refused = card.reply(&i18n, &parse(r#"{"t":"permissions","id":7,"op":"apply","ok":false,"error":"Selected item changed; reopen Permissions."}"#).unwrap());
    assert!(refused.is_empty());
    assert!(notes(&card.card(&i18n)).contains(&(
        "Selected item changed; reopen Permissions.".into(),
        Ink::Error
    )));
    assert_eq!(card.focus, PermStop::Cancel);
    card.key(Key::Tab);
    card.key(Key::Enter);
    let done = card.reply(
        &i18n,
        &parse(r#"{"t":"permissions","id":7,"op":"apply","ok":true,"mode":"0644"}"#).unwrap(),
    );
    assert_eq!(
        done,
        vec![
            Effect::Message("Permissions changed.".into()),
            Effect::Refresh,
            Effect::Send(r#"{"c":"permissions","op":"close","id":7}"#.into()),
            Effect::Close
        ]
    );
}

#[test]
fn permissions_backend_death_says_whether_the_change_may_have_landed() {
    let i18n = english();
    let mut card = permissions();
    card.stopped();
    let drawn = card.card(&i18n);
    assert_eq!(notes(&drawn)[0].0, "Permissions is unavailable because Flea stopped responding. Restart Flea and reopen Permissions.");
    assert_eq!(buttons(&drawn), "Cancel@");
    let mut card = permissions();
    card.key(Key::Enter);
    card.stopped();
    assert!(notes(&card.card(&i18n))[0]
        .0
        .starts_with("Flea stopped responding, so the permission change may not have finished."));
    assert_eq!(card.key(Key::Escape), vec![Effect::Close]);
}

#[test]
fn permissions_for_several_items_mix_boxes_and_send_one_batch() {
    let i18n = english();
    let paths: Vec<String> = (0..9).map(|n| format!("/fx/p/{n}.jpg")).collect();
    let (mut card, effects) = Permissions::open(3, paths, "");
    assert_eq!(
        sent(&effects)[8],
        r#"{"c":"permissions","op":"inspect","id":3008,"path":"/fx/p/8.jpg"}"#
    );
    for at in 0..9 {
        let mode = if at % 2 == 0 { "0644" } else { "0755" };
        card.reply(
            &i18n,
            &parse(&format!(
                r#"{{"op":"inspect","id":{},"ok":true,"mode":"{mode}","reason":""}}"#,
                3000 + at
            ))
            .unwrap(),
        );
    }
    let drawn = card.card(&i18n);
    assert_eq!(drawn.title, "Permissions for 9 Items");
    assert_eq!(buttons(&drawn), "Cancel|Apply*@");
    use Tri::{Mixed, On};
    assert_eq!(
        grid_checks(&drawn.body)
            .iter()
            .map(|c| c.0)
            .collect::<Vec<_>>()[..3],
        [On, On, Mixed]
    );
    assert_eq!(
        notes(&drawn),
        vec![(
            "Mixed boxes keep each item's setting unless you change them.".into(),
            Ink::Foreground
        )]
    );
    let batch = sent(&card.key(Key::Enter));
    assert!(
        batch[0].starts_with(r#"{"c":"permissionsBatch","paths":["/fx/p/0.jpg""#),
        "{batch:?}"
    );
    let done = card.reply(
        &i18n,
        &parse(r#"{"t":"permissions","id":3,"op":"applyMany","ok":true,"mode":"0644","error":""}"#)
            .unwrap(),
    );
    assert_eq!(
        done,
        vec![
            Effect::Message("Permissions changed.".into()),
            Effect::Refresh,
            Effect::Send(r#"{"c":"permissions","op":"close","id":3000}"#.into()),
            Effect::Send(r#"{"c":"permissions","op":"close","id":3001}"#.into()),
            Effect::Send(r#"{"c":"permissions","op":"close","id":3002}"#.into()),
            Effect::Send(r#"{"c":"permissions","op":"close","id":3003}"#.into()),
            Effect::Send(r#"{"c":"permissions","op":"close","id":3004}"#.into()),
            Effect::Send(r#"{"c":"permissions","op":"close","id":3005}"#.into()),
            Effect::Send(r#"{"c":"permissions","op":"close","id":3006}"#.into()),
            Effect::Send(r#"{"c":"permissions","op":"close","id":3007}"#.into()),
            Effect::Send(r#"{"c":"permissions","op":"close","id":3008}"#.into()),
            Effect::Close
        ]
    );
}

#[test]
fn several_items_permissions_close_every_inspect_id_and_a_failed_batch_names_its_skips() {
    let i18n = english();
    let paths: Vec<String> = (0..2).map(|n| format!("/fx/p/{n}.jpg")).collect();
    let (mut card, _) = Permissions::open(3, paths, "");
    let initial = card.card(&i18n);
    assert_eq!(notes(&initial), vec![("Working…".into(), Ink::Muted)]);
    assert_eq!(buttons(&initial), "Cancel@");
    card.reply(
        &i18n,
        &parse(r#"{"op":"inspect","id":3000,"ok":true,"mode":"0644","reason":""}"#).unwrap(),
    );
    card.reply(
        &i18n,
        &parse(r#"{"op":"inspect","id":3001,"ok":false,"error":"Permission denied"}"#).unwrap(),
    );
    let closes = vec![
        r#"{"c":"permissions","op":"close","id":3000}"#.to_string(),
        r#"{"c":"permissions","op":"close","id":3001}"#.to_string(),
    ];
    assert_eq!(
        sent(&card.clone().key(Key::Escape)),
        closes,
        "v0.3.12 close() closes every inspect id"
    );
    card.key(Key::Enter);
    card.reply(
        &i18n,
        &parse(r#"{"t":"permissions","id":3,"op":"applyMany","ok":false,"error":"Disk full"}"#)
            .unwrap(),
    );
    let shown = notes(&card.card(&i18n));
    assert_eq!(
        shown.last().unwrap(),
        &(
            "Disk full.\n1.jpg keeps its mode: Permission denied.".to_string(),
            Ink::Error
        ),
        "{shown:?}"
    );
    assert_eq!(
        buttons(&card.card(&i18n)),
        "Cancel@|Apply*~",
        "the reread holds Apply and focuses Cancel"
    );
    card.reply(
        &i18n,
        &parse(r#"{"op":"inspect","id":3000,"ok":true,"mode":"0644","reason":""}"#).unwrap(),
    );
    let done = card.reply(
        &i18n,
        &parse(r#"{"op":"inspect","id":3001,"ok":true,"mode":"0644","reason":""}"#).unwrap(),
    );
    assert!(done.is_empty());
    card.key(Key::Tab);
    card.key(Key::Enter);
    let ok = card.reply(
        &i18n,
        &parse(r#"{"t":"permissions","id":3,"op":"applyMany","ok":true,"mode":"0644","error":""}"#)
            .unwrap(),
    );
    assert_eq!(
        sent(&ok),
        closes,
        "a done batch closes every inspect id too"
    );
}
