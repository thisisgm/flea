//! Each card drawn in a real window through the stand-in shell, measured against the boards' frame sizes.
use super::keys::Key;
use super::shell::{Card, DialogShell, StandIn};
use super::tests::{english, open_with, permissions};
use crate::list::{Palette, Typeface};
use crate::palette::Theme;
use gpui::{prelude::*, px, size, Bounds, Context, Pixels, Render, TestAppContext, Window};

struct Probe(Card, Typeface);

impl Render for Probe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        StandIn.present(
            &self.0,
            &palette,
            &self.1,
            crate::compositor::corner(cx),
            f32::from(window.viewport_size().width),
        )
    }
}

fn drawn(cx: &mut TestAppContext, card: Card, body: f32) -> (Bounds<Pixels>, Vec<Bounds<Pixels>>) {
    let buttons = card.buttons.len();
    let typeface = Typeface {
        body: px(body),
        ..Typeface::omarchy_default()
    };
    let (_, cx) = cx.add_window_view(|_, _| Probe(card, typeface));
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    let frame = cx
        .debug_bounds("op3-card")
        .expect("the card frame is drawn");
    let names = [
        "op3-button-0",
        "op3-button-1",
        "op3-button-2",
        "op3-button-3",
    ];
    (
        frame,
        names[..buttons]
            .iter()
            .map(|n| cx.debug_bounds(n).expect("each button is drawn"))
            .collect(),
    )
}

fn properties_file() -> Card {
    let i18n = english();
    let (mut card, _) = super::properties::Properties::open(5, 1, &[2], 0);
    card.set_extras(Vec::new(), "Plain text document");
    card.reply(&i18n, &flea::jsondoc::parse(r#"{"op":"properties","id":5,"ok":true,"path":"/fx/v/work/notes.txt","kind":"File","bytes":99,"modified":0,"mode":"0644","owner":"gm","uid":1000,"gid":1000}"#).unwrap());
    card.card(&i18n)
}

#[gpui::test]
fn every_card_draws_at_its_board_width_and_centres_in_the_window(cx: &mut TestAppContext) {
    let i18n = english();
    let run = super::runcard::RunCard::open(&i18n, "flea", "/fx/v/work/hello.sh", r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":1,"mtimeNs":2,"target":"/fx/v/work/hello.sh"}"#).unwrap();
    let bulk =
        super::bulkrename::BulkRename::open(vec!["/fx/b/alpha.txt".into(), "/fx/b/beta.md".into()]);
    for (card, width) in [
        (open_with().card(&i18n), 630.),
        (permissions().card(&i18n), 480.),
        (properties_file(), 552.),
        (run.card(&i18n), 500.),
        (bulk.card(&i18n), 640.),
    ] {
        let title = card.title.clone();
        let (frame, buttons) = drawn(cx, card, 14.);
        assert_eq!(f32::from(frame.size.width), width, "{title}");
        assert!(
            (f32::from(frame.origin.x) - (1100. - width) / 2.).abs() <= 0.5,
            "{title} centres"
        );
        for button in &buttons {
            assert_eq!(
                f32::from(button.size.height),
                30.,
                "{title}: a button stands 30 tall"
            );
        }
        let last = buttons.last().unwrap();
        assert!(
            (f32::from(frame.right()) - f32::from(last.right()) - 17.).abs() <= 0.5,
            "{title}: 16 padding and the hairline frame"
        );
        assert!(
            (f32::from(frame.bottom()) - f32::from(last.bottom()) - 17.).abs() <= 0.5,
            "{title}: the buttons sit 16 over the frame"
        );
    }
}

#[gpui::test]
fn a_files_properties_card_measures_the_boards_298_frame_to_frame(cx: &mut TestAppContext) {
    let (frame, _) = drawn(cx, properties_file(), 14.);
    assert!(
        (f32::from(frame.size.height) - 298.).abs() <= 1.,
        "{:?}",
        frame.size
    );
}

#[gpui::test]
fn a_larger_body_grows_the_card_and_its_buttons(cx: &mut TestAppContext) {
    let (frame, buttons) = drawn(cx, properties_file(), 16.);
    assert_eq!(f32::from(frame.size.width), (552. * 16. / 14_f32).ceil());
    assert!((f32::from(buttons[0].size.height) - 30. * 16. / 14.).abs() <= 0.5);
}

fn properties_of(id: u64, reply: &str, more: &[&str]) -> Card {
    let i18n = english();
    let (mut card, _) = super::properties::Properties::open(id, 1, &[0], 0);
    card.set_extras(Vec::new(), "Plain text document");
    for line in std::iter::once(reply).chain(more.iter().copied()) {
        card.reply(&i18n, &flea::jsondoc::parse(line).unwrap());
    }
    card.card(&i18n)
}

/// Frame heights read from each board PNG: the straight run of the frame's left edge plus its two 8 px corners.
#[gpui::test]
fn every_card_matches_its_boards_frame_height(cx: &mut TestAppContext) {
    let i18n = english();
    let run = super::runcard::RunCard::open(&i18n, "flea", "/fx/v/work/hello.sh", r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":1,"mtimeNs":2,"target":"/fx/v/work/hello.sh"}"#).unwrap();
    let bulk =
        super::bulkrename::BulkRename::open(vec!["/fx/b/alpha.txt".into(), "/fx/b/beta.md".into()]);
    let mut clash = super::bulkrename::BulkRename::open(vec![
        "/fx/b/alpha.txt".into(),
        "/fx/b/gamma.txt".into(),
    ]);
    for key in "alpha"
        .chars()
        .map(|c| Key::Text(c.into()))
        .chain([Key::Tab])
        .chain("gamma".chars().map(|c| Key::Text(c.into())))
    {
        clash.key(key);
    }
    let (mut several, _) = super::permissions::Permissions::open(
        3,
        (0..9).map(|n| format!("/fx/p/{n}.jpg")).collect(),
        "",
    );
    for at in 0..9 {
        let mode = if at % 2 == 0 { "0644" } else { "0755" };
        several.reply(
            &i18n,
            &flea::jsondoc::parse(&format!(
                r#"{{"op":"inspect","id":{},"ok":true,"mode":"{mode}","reason":""}}"#,
                3000 + at
            ))
            .unwrap(),
        );
    }
    let folder = properties_of(
        6,
        r#"{"op":"properties","id":6,"ok":true,"path":"/fx/v/work/Media","kind":"Directory","directory":true,"bytes":4096,"modified":0,"mode":"0755","owner":"gm","uid":1000,"gid":1000}"#,
        &[
            r#"{"op":"contents","id":6,"ok":true,"bytes":2560562,"items":11,"partial":false,"done":true}"#,
        ],
    );
    let tagged = properties_of(
        8,
        r#"{"op":"properties","id":8,"ok":true,"path":"/fx/v/work/tagged.txt","kind":"File","bytes":13,"modified":0,"mode":"0644","owner":"gm","uid":1000,"gid":1000,"tags":["Red","Blue"]}"#,
        &[],
    );
    let boards = [
        ("openwith", open_with().card(&i18n), 546.),
        ("permissions", permissions().card(&i18n), 558.),
        ("permissions-several", several.card(&i18n), 306.),
        ("properties-folder", folder, 323.),
        ("properties-tagged", tagged, 323.),
        ("runfile", run.card(&i18n), 150.),
        ("bulkrename", bulk.card(&i18n), 332.),
        ("bulkrename-clash", clash.card(&i18n), 332.),
    ];
    let mut off = Vec::new();
    for (board, card, height) in boards {
        let drawn = f32::from(drawn(cx, card, 14.).0.size.height);
        if (drawn - height).abs() > 1. {
            off.push(format!("{board} {drawn} against {height}"));
        }
    }
    assert!(off.is_empty(), "{off:?}");
}

#[gpui::test]
fn s6_folder_properties_draws_all_eight_compact_board_rows(cx: &mut TestAppContext) {
    let card = properties_of(
        6,
        r#"{"op":"properties","id":6,"ok":true,"path":"/fx/v/work/Media","kind":"Directory","directory":true,"bytes":4096,"modified":1749981600,"mode":"0755","owner":"gm","uid":1000,"gid":1000}"#,
        &[
            r#"{"op":"contents","id":6,"ok":true,"bytes":2560562,"items":11,"partial":false,"done":true}"#,
        ],
    );
    let super::shell::Block::Facts(facts) = &card.body[0] else {
        panic!("Properties draws facts");
    };
    let pairs: Vec<_> = facts
        .iter()
        .map(|fact| match fact {
            super::shell::Fact::Text { name, value, tail } => {
                assert!(tail.is_empty());
                (name.as_str(), value.as_str())
            }
            _ => panic!("the untagged folder has text facts only"),
        })
        .collect();
    let modified = english().locale().format_date(1749981600).unwrap();
    assert_eq!(
        pairs,
        [
            ("Path", "/fx/v/work/Media"),
            ("Kind", "Folder"),
            ("Size", "2.6 MB (2,560,562 bytes)"),
            ("Contains", "11 items"),
            ("Modified", modified.as_str()),
            ("Permissions", "0755"),
            ("Owner", "gm (uid 1000)"),
            ("Group", "gid 1000"),
        ]
    );
    let (_, cx) = cx.add_window_view(|_, _| Probe(card, Typeface::omarchy_default()));
    cx.simulate_resize(size(px(1100.), px(700.)));
    cx.run_until_parked();
    let frame = cx.debug_bounds("op3-card").unwrap();
    assert!(
        (f32::from(frame.size.height) - 323.).abs() <= 1.,
        "{frame:?}"
    );
    let first = cx.debug_bounds("op3-fact-0").unwrap();
    let names = [
        "op3-fact-0",
        "op3-fact-1",
        "op3-fact-2",
        "op3-fact-3",
        "op3-fact-4",
        "op3-fact-5",
        "op3-fact-6",
        "op3-fact-7",
    ];
    for (at, name) in names.into_iter().enumerate() {
        let row = cx.debug_bounds(name).expect("every fact is painted");
        assert_eq!(f32::from(row.size.height), 24., "row {at}");
        assert_eq!(row.left(), first.left(), "one value column");
        assert_eq!(
            f32::from(row.top() - first.top()),
            at as f32 * 24.,
            "row {at}"
        );
    }
    let close = cx.debug_bounds("op3-button-0").unwrap();
    assert!(close.bottom() < frame.bottom());
}
