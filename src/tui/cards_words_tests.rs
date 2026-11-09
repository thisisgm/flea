use crate::backend::testdir::TestDir;
use crate::i18n::{I18n, Paths};
use crate::jsondoc::parse;
use crate::tui::{
    action_key_tests::drain,
    combine, convert,
    input::Key,
    model::Model,
    print, render,
    render::frame_tests::{entry, model},
    wire::{self, Wire},
};

pub(crate) fn english() -> I18n {
    let root = TestDir::new("card-words-english");
    root.dir("catalog/en");
    root.file("catalog/en/flea.json", r#"{
  "tui.batch.not_unique": "Rename selection is not a unique absolute selection",
  "tui.batch.invalid_text": "Bulk rename cannot handle a name that is not valid text",
  "tui.batch.line_break": "Bulk rename cannot represent a filename containing a line break",
  "tui.batch.no_editor": "No $EDITOR is set, so there is nothing to hand the names to",
  "tui.batch.count_changed": "Bulk rename refused: the number of names changed",
  "tui.batch.invalid_name": "Bulk rename refused: a name is invalid",
  "tui.batch.source_changed": "Bulk rename refused: a selected item changed",
  "tui.batch.no_parent": "Selected item has no parent",
  "tui.batch.collision": "Bulk rename refused: two names collide",
  "tui.batch.exists": "Bulk rename refused: a destination already exists",
  "tui.batch.nested": "Rename selected folders separately from their selected contents",
  "tui.batch.unexpected": "Unexpected rename completion",
  "tui.batch.mismatched": "Rename completion did not match the pending item",
  "tui.batch.too_many": "Rename selection is too large",
  "tui.batch.too_long": "An edited name is too long",
  "tui.batch.prepare_failed": "Bulk rename could not prepare its temporary folder",
  "tui.batch.cleanup_failed": "%1; cleanup could not finish: %2",
  "tui.batch.folder_changed": "Bulk rename stopped because its temporary folder %1 changed",
  "tui.batch.cleanup_outside": "Bulk rename would not remove %1, which is not inside its temporary folder %2",
  "tui.batch.cleanup_left": "Bulk rename left temporary files at %1: %2",
  "tui.taildrop.unread": "Taildrop status could not be read",
  "tui.taildrop.too_large": "Taildrop status was too large to read",
  "tui.taildrop.unavailable": "Taildrop unavailable: tailscale could not start",
  "tui.taildrop.timeout": "Taildrop status timed out",
  "tui.taildrop.status_failed": "Tailscale reported an error",
  "tui.taildrop.sent": "Sent to %1",
  "tui.taildrop.failed": "Could not send to %1 (%2)",
  "tui.taildrop.busy": "Taildrop is already busy",
  "tui.taildrop.absolute": "Taildrop needs an absolute file selection",
  "tui.taildrop.disabled": "disabled for this account"
}"#);
    I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.file("language", "en"),
        catalogs: vec![root.join("catalog")],
    })
}

fn locale(root: &TestDir, japanese: bool) -> I18n {
    root.dir("catalog/en");
    root.dir("catalog/ja");
    root.dir("omarchy/default/i18n");
    root.file(
        "omarchy/default/i18n/locales.json",
        include_str!("../i18n/fixtures/locales.json"),
    );
    root.file(
        "catalog/en/flea.json",
        include_str!("../../i18n/en/flea.json"),
    );
    root.file(
        "catalog/ja/flea.json",
        r#"{
      "dialogs.output.converting":"変換中です",
      "dialogs.output.error.pages":"ページ範囲を入力してください",
      "dialogs.output.error.copies":"部数を入力してください",
      "dialogs.output.error.columns":"列数を入力してください",
      "dialogs.output.completed":"変換先 %1",
      "menu.dropbox":"Dropboxに移動",
      "menu.state.signedOut":"未ログイン",
      "dialogs.output.choice.Row":"長い横並びの選択肢です",
      "dialogs.output.choice.Column":"長い縦並びの選択肢です",
      "dialogs.output.choice.Grid":"長い格子の選択肢です",
      "dialogs.output.choice.None":"間隔がない選択肢です",
      "dialogs.output.choice.Small":"小さい間隔の選択肢です",
      "dialogs.output.choice.Large":"大きい間隔の選択肢です",
      "dialogs.output.cancel":"キャンセル",
      "dialogs.output.convert":"変換",
      "dialogs.output.print":"印刷",
      "dialogs.output.combine":"結合"
    }"#,
    );
    let i18n = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.file("language", if japanese { "ja" } else { "en" }),
        catalogs: vec![root.join("catalog")],
    });
    assert_eq!(i18n.language(), if japanese { "ja" } else { "en" });
    i18n
}
fn answer(m: &mut Model, w: &mut Wire, action: &str, paths: &str) {
    let id = m.action_id;
    for line in [
        format!(r#"{{"t":"menuaction","op":"snapshot","id":{id},"ok":true}}"#),
        format!(
            r#"{{"t":"menuaction","op":"validate","id":{id},"action":"{action}","ok":true,"paths":{paths}}}"#
        ),
    ] {
        m.receive(parse(&line).unwrap(), w).unwrap();
        drain(w);
    }
}
fn opened(w: &mut Wire, action: &str) -> Model {
    let mut m = model();
    m.rows.insert(7, entry("one.png", false, 8));
    m.rows.insert(8, entry("two.png", false, 8));
    if action == "combine" {
        m.selected = [7, 8].into_iter().collect();
    }
    match action {
        "combine" => combine::open(&mut m, w, false).unwrap(),
        "print" => print::open(&mut m, w, false).unwrap(),
        _ => convert::open(&mut m, w, false).unwrap(),
    }
    drain(w);
    answer(
        &mut m,
        w,
        action,
        if action == "combine" {
            r#"["/work/one.png","/work/two.png"]"#
        } else {
            r#"["/work/one.png"]"#
        },
    );
    m
}

#[test]
fn tui_card_words_keep_geometry_and_hit_targets_aligned() {
    let root = TestDir::new("card-words-geometry");
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "combine");
    m.i18n = locale(&root, true);
    for columns in [80, 130, 240] {
        m.columns = columns;
        let g = combine::geometry(&m);
        for focus in [1, 4] {
            let fields: Vec<_> = g.fields.iter().filter(|f| f.focus == focus).collect();
            assert_eq!(
                fields.len(),
                3,
                "translation hid an option at {columns} columns"
            );
            for field in fields {
                assert!(field.rect.width > 0);
                assert!(field.rect.hit(field.rect.x + 1, field.rect.y + 1));
                assert!(field.rect.x + field.rect.width <= g.card.left() + g.card.content());
                let mut key = Key::named("Pointer", "");
                key.pointer = Some(crate::tui::input::Pointer {
                    x: field.rect.x + 1,
                    y: field.rect.y + 1,
                    button: 0,
                    motion: false,
                    released: false,
                });
                combine::key(&mut m, &key, &mut w).unwrap();
                let form = m.combining.card.as_ref().unwrap();
                if focus == 1 {
                    assert_eq!(
                        form.settings.layout,
                        [
                            flea::ui_logic_print_convert::composition::Layout::Row,
                            flea::ui_logic_print_convert::composition::Layout::Column,
                            flea::ui_logic_print_convert::composition::Layout::Grid,
                        ][field.choice]
                    );
                } else {
                    assert_eq!(
                        form.settings.spacing,
                        [
                            flea::ui_logic_print_convert::composition::Spacing::None,
                            flea::ui_logic_print_convert::composition::Spacing::Small,
                            flea::ui_logic_print_convert::composition::Spacing::Large,
                        ][field.choice]
                    );
                }
            }
        }
    }
    assert!(drain(&mut w).is_empty());
    let mut menu = model();
    menu.i18n = locale(&root, false);
    menu.menu_path = "/work/one.png".into();
    menu.menu_count = 1;
    menu.menu_ready = true;
    menu.menu_view.dropbox = Some("menu.state.signedOut".into());
    let map = crate::tui::keymap::Map::load();
    let before = render::menu_layout::entries(&menu, &map);
    assert!(before
        .iter()
        .flatten()
        .any(|row| row.id == "dropbox" && row.label == "Move to Dropbox" && row.trail.is_empty()));
    root.file("language", "ja");
    assert!(menu.refresh_locale(&[render::cards::refresh_locale]));
    let rows = render::menu_layout::entries(&menu, &map);
    let index = rows
        .iter()
        .position(|row| row.as_ref().is_some_and(|row| row.id == "dropbox"))
        .unwrap();
    let row = rows[index].as_ref().unwrap();
    // GM 2026-10-29 keeps unavailable root rows free of state words.
    assert_eq!(row.label, "Dropboxに移動");
    assert!(row.trail.is_empty() && !row.keycap);
    assert!(!row.enabled());
    let place = render::menu_layout::place(&menu, &rows);
    assert_eq!(
        render::menu_layout::hit(&menu, &map, place.x + 2, place.y + 2 + index - place.top),
        Some(Some(index))
    );
    // State words still localize in the empty Taildrop submenu.
    menu.menu_view.sub = render::menu_layout::Sub::Taildrop;
    menu.taildrop
        .status(&parse(r#"{"BackendState":"NeedsLogin"}"#).unwrap());
    let rows = render::menu_layout::entries(&menu, &map);
    assert_eq!(rows[0].as_ref().unwrap().label, "未ログイン");
    root.file("language", "en");
    assert!(menu.refresh_locale(&[render::cards::refresh_locale]));
    let rows = render::menu_layout::entries(&menu, &map);
    assert_eq!(rows[0].as_ref().unwrap().label, "signed out");
    wire::finish(w, reader);
}

#[test]
fn tui_card_words_preserve_default_and_disabled_state() {
    let root = TestDir::new("card-words-state");
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "combine");
    let f = m.combining.card.as_mut().unwrap();
    f.settings.spacing = flea::ui_logic_print_convert::composition::Spacing::None;
    f.focus = 7;
    let before = combine::geometry(&m);
    m.i18n = locale(&root, true);
    render::cards::refresh_locale(&mut m);
    let after = combine::geometry(&m);
    assert_eq!(m.combining.card.as_ref().unwrap().focus, 7);
    assert!(before
        .fields
        .iter()
        .filter(|f| f.focus == 5)
        .all(|f| !f.enabled));
    assert!(after
        .fields
        .iter()
        .filter(|f| f.focus == 5)
        .all(|f| !f.enabled));
    assert_eq!(before.card.height, after.card.height);
    let mut out = String::new();
    let theme = crate::tui::theme::Theme::from_text("");
    combine::draw(&mut out, &m, &theme);
    assert!(out.contains(&format!("{}[{}", theme.accent, theme.wash)));
    assert!(drain(&mut w).is_empty());
    wire::finish(w, reader);
}

#[test]
fn tui_card_words_localize_held_receipts_without_rerun() {
    let root = TestDir::new("card-words-receipt");
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "convert");
    m.i18n = locale(&root, false);
    convert::submit(&mut m, &mut w).unwrap();
    assert_eq!(
        drain(&mut w)
            .iter()
            .filter(|v| crate::tui::wire::text(v, "c") == "convertbatch")
            .count(),
        1
    );
    assert!(m.converting.run.is_some());
    let time = m.message_at;
    root.file("language", "ja");
    assert!(m.refresh_locale(&[render::cards::refresh_locale]));
    assert_eq!(m.locale_revision, 1);
    assert_eq!(m.transfer, "変換中です");
    assert!(m.converting.run.is_some());
    assert_eq!(m.message_at, time);
    assert!(
        drain(&mut w).is_empty(),
        "locale refresh replayed a request"
    );
    wire::finish(w, reader);
}

#[test]
fn tui_card_words_switch_open_cards_atomically() {
    let root = TestDir::new("card-words-open");
    let (mut w, reader) = wire::echo_wire();
    for action in ["convert", "print", "combine"] {
        let mut m = opened(&mut w, action);
        m.i18n = locale(&root, false);
        let expected = match action {
            "print" => {
                let id = m.printing.card.as_ref().unwrap().query.unwrap();
                m.receive(parse(&format!(r#"{{"t":"printoptions","id":{id},"ok":true,"destinations":[{{"name":"Brother","default":true}}],"printer":"Brother","duplex":false,"twoSided":false,"color":false,"grayscale":false}}"#)).unwrap(), &mut w).unwrap();
                m.printing.card.as_mut().unwrap().copies = "0".into();
                print::submit(&mut m, &mut w).unwrap();
                "部数を入力してください"
            }
            "combine" => {
                let f = m.combining.card.as_mut().unwrap();
                f.settings.layout = flea::ui_logic_print_convert::composition::Layout::Grid;
                f.columns = "0".into();
                combine::submit(&mut m, &mut w).unwrap();
                "列数を入力してください"
            }
            _ => {
                m.converting.card.as_mut().unwrap().pages = "invalid".into();
                convert::submit(&mut m, &mut w).unwrap();
                "ページ範囲を入力してください"
            }
        };
        let error = |m: &Model| match action {
            "print" => m.printing.card.as_ref().unwrap().error.clone(),
            "combine" => m.combining.card.as_ref().unwrap().error.clone(),
            _ => m.converting.card.as_ref().unwrap().error.clone(),
        };
        let english = error(&m);
        assert!(!english.is_empty());
        root.file("language", "ja");
        assert!(m.refresh_locale(&[render::cards::refresh_locale]));
        assert_eq!(m.locale_revision, 1);
        assert_eq!(error(&m), expected);
        let shown = crate::tui::convert_tests::frame(&m);
        assert!(shown.contains(expected), "{action}: {shown}");
        assert!(!shown.contains(&english));
        assert!(
            m.converting.run.is_none() && m.combining.run.is_none() && m.printing.run.is_none()
        );
        assert!(drain(&mut w).is_empty());
    }
    wire::finish(w, reader);
}

#[test]
fn tui_card_words_rtl_reflects_paint_and_pointer_together() {
    let root = TestDir::new("card-words-rtl");
    let (mut w, reader) = wire::echo_wire();
    let mut m = opened(&mut w, "combine");
    m.i18n = locale(&root, false);
    let before = combine::geometry(&m);
    root.file("language", "ar");
    assert!(m.refresh_locale(&[render::cards::refresh_locale]));
    assert!(m.i18n.locale().right_to_left);
    let after = combine::geometry(&m);
    for (old, new) in before.fields.iter().zip(&after.fields) {
        assert_eq!(
            (old.focus, old.choice, old.enabled),
            (new.focus, new.choice, new.enabled)
        );
        assert_eq!(
            new.rect.x,
            after.card.left() + after.card.content()
                - (old.rect.x - before.card.left())
                - old.rect.width
        );
    }
    let field = after
        .fields
        .iter()
        .find(|f| f.focus == 1 && f.choice == 2)
        .unwrap();
    let mut key = Key::named("Pointer", "");
    key.pointer = Some(crate::tui::input::Pointer {
        x: field.rect.x + 1,
        y: field.rect.y + 1,
        button: 0,
        motion: false,
        released: false,
    });
    combine::key(&mut m, &key, &mut w).unwrap();
    assert_eq!(
        m.combining.card.as_ref().unwrap().settings.layout,
        flea::ui_logic_print_convert::composition::Layout::Grid
    );
    let name = "تقرير 2025\u{202e}.png";
    m.combining.card.as_mut().unwrap().name = name.into();
    let mut out = String::new();
    combine::draw(&mut out, &m, &crate::tui::theme::Theme::from_text(""));
    assert!(out.contains('\u{2068}'));
    assert!(!out.contains('\u{202e}'));
    assert_eq!(m.combining.card.as_ref().unwrap().name, name);
    assert!(drain(&mut w).is_empty());
    wire::finish(w, reader);
}
