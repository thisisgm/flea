//! Properties: a file's facts, a folder's walk, a tagged file, several items and the working note, as plain data.
use super::keys::Key;
use super::properties::Properties;
use super::shell::{Block, Card, Fact, Ink};
use super::tests::{buttons, english, notes, sent};
use super::Effect;
use flea::jsondoc::parse;

fn facts(card: &Card) -> Vec<Fact> {
    card.body
        .iter()
        .find_map(|b| match b {
            Block::Facts(f) => Some(f.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn fact_lines(card: &Card) -> String {
    facts(card)
        .iter()
        .map(|f| match f {
            Fact::Text { name, value, tail } => format!("{name}={value}{tail}"),
            Fact::Tags { name, tags } => format!("{name}={}", tags.join("+")),
        })
        .collect::<Vec<_>>()
        .join("|")
}

#[test]
fn properties_shows_a_files_facts_on_the_boards_lines_with_close_the_focused_default() {
    let i18n = english();
    let (mut card, effects) = Properties::open(5, 3, &[2], 2);
    assert_eq!(
        sent(&effects),
        vec![r#"{"c":"menuaction","op":"snapshot","id":5,"listing":3,"rows":[2],"cursor":2}"#]
    );
    let acknowledged = card.reply(
        &i18n,
        &parse(r#"{"t":"menuaction","op":"snapshot","id":5,"ok":true,"kinds":[33188]}"#).unwrap(),
    );
    assert_eq!(
        sent(&acknowledged),
        vec![r#"{"c":"menuaction","op":"properties","id":5}"#]
    );
    card.set_extras(Vec::new(), "Plain text document");
    card.reply(&i18n, &parse(r#"{"op":"properties","id":5,"ok":true,"path":"/fx/v/work/notes.txt","kind":"File","bytes":99,"modified":0,"mode":"0644","owner":"gm","uid":1000,"gid":1000}"#).unwrap());
    let drawn = card.card(&i18n);
    assert_eq!((drawn.title.as_str(), drawn.width), ("Properties", 552.));
    let lines = fact_lines(&drawn);
    assert!(
        lines.starts_with(
            "Path=/fx/v/work/notes.txt|Kind=Plain text document|Size=99 B (99 bytes)|Modified="
        ),
        "{lines}"
    );
    assert!(
        lines.ends_with("|Permissions=0644|Owner=gm (uid 1000)|Group=gid 1000"),
        "{lines}"
    );
    assert_eq!(buttons(&drawn), "Close*@");
    assert_eq!(
        card.key(Key::Escape),
        vec![
            Effect::Send(r#"{"c":"menuaction","op":"close","id":5}"#.into()),
            Effect::Close
        ]
    );
    assert_eq!(card.key(Key::Enter).len(), 2, "Enter closes as Escape does");
}

#[test]
fn properties_of_a_folder_walk_its_contents_and_a_tagged_file_lists_its_tags() {
    let i18n = english();
    let (mut card, _) = Properties::open(6, 1, &[0], 0);
    let asked = card.reply(&i18n, &parse(r#"{"op":"properties","id":6,"ok":true,"path":"/fx/v/work/Media","kind":"Directory","directory":true,"bytes":4096,"modified":0,"mode":"0755","owner":"gm","uid":1000,"gid":1000}"#).unwrap());
    assert_eq!(
        sent(&asked),
        vec![r#"{"c":"menuaction","op":"contents","id":6}"#]
    );
    assert!(fact_lines(&card.card(&i18n)).contains("Contains=> 0 items \u{b7} counting\u{2026}"));
    card.reply(&i18n, &parse(r#"{"op":"contents","id":6,"ok":true,"bytes":2560562,"items":11,"partial":false,"done":true}"#).unwrap());
    let lines = fact_lines(&card.card(&i18n));
    assert!(
        lines.contains("Kind=Folder|Size=2.6 MB (2,560,562 bytes)|Contains=11 items|"),
        "{lines}"
    );
    card.reply(
        &i18n,
        &parse(r#"{"op":"contents","id":99,"ok":true,"bytes":1,"items":1,"done":true}"#).unwrap(),
    );
    assert_eq!(
        fact_lines(&card.card(&i18n)),
        lines,
        "another id's reply changes nothing"
    );
    let (mut tagged, _) = Properties::open(8, 1, &[1], 0);
    tagged.reply(&i18n, &parse(r#"{"op":"properties","id":8,"ok":true,"path":"/fx/v/work/tagged.txt","kind":"File","bytes":13,"modified":0,"mode":"0644","owner":"gm","uid":1000,"gid":1000,"tags":["Red","Blue"]}"#).unwrap());
    assert!(
        fact_lines(&tagged.card(&i18n)).contains("Size=13 B (13 bytes)|Tags=Red+Blue|Modified=")
    );
}

#[test]
fn several_items_properties_summarise_and_a_cancel_closes_quietly() {
    let i18n = english();
    let (mut card, _) = Properties::open(9, 1, &[0, 1, 2], 0);
    card.reply(&i18n, &parse(r#"{"op":"properties","id":9,"ok":true,"kinds":"2 images, 1 video","bytes":52300000,"newest":"2026-09-23 10:14","oldest":"2026-08-21 12:02","partial":false,"done":true}"#).unwrap());
    assert_eq!(
        fact_lines(&card.card(&i18n)),
        "Kinds=2 images, 1 video|Combined=52.3 MB|Newest=2026-09-23 10:14|Oldest=2026-08-21 12:02"
    );
    let (mut cancelled, _) = Properties::open(10, 1, &[0, 1], 0);
    assert_eq!(
        cancelled.reply(
            &i18n,
            &parse(r#"{"op":"properties","id":10,"ok":true,"cancelled":true}"#).unwrap()
        ),
        vec![Effect::Close],
        "no error toast"
    );
    let (mut refused, _) = Properties::open(11, 1, &[0, 1], 0);
    refused.reply(&i18n, &parse(r#"{"op":"properties","id":11,"ok":false,"error":"Selected items changed; reopen the menu."}"#).unwrap());
    assert_eq!(
        notes(&refused.card(&i18n)),
        vec![(
            "Selected items changed; reopen the menu.".into(),
            Ink::Error
        )]
    );
}

#[test]
fn properties_says_working_until_its_first_reply_lands() {
    let i18n = english();
    let (mut card, _) = Properties::open(12, 1, &[0], 0);
    assert_eq!(
        notes(&card.card(&i18n)),
        vec![("Working\u{2026}".into(), Ink::Muted)]
    );
    card.reply(&i18n, &parse(r#"{"op":"properties","id":12,"ok":true,"path":"/fx/a","kind":"File","bytes":1,"modified":0,"mode":"0644","owner":"gm","uid":1000,"gid":1000}"#).unwrap());
    assert!(notes(&card.card(&i18n)).is_empty());
}
