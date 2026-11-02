//! The mn2 lifecycle in the window: discovery, rows, preview, run and refusals.
use super::model::{Actions, Said};
use crate::list::bench::english;
use flea::ui_logic_mn2::plugins::{Contribution, Item, Menu, When};
use gpui::TestAppContext;

fn listed(actions: &mut Actions) {
    let open = actions
        .menu_opened(vec!["/fx/a.jpg".into(), "/fx/b.jpg".into()])
        .expect("a two-path menu discovers");
    assert_eq!(
        open,
        r#"{"c":"actions","op":"list","id":1,"paths":["/fx/a.jpg","/fx/b.jpg"]}"#
    );
    actions.receive(r#"{"t":"actions","id":1,"actions":[{"id":"action:resize.toml","name":"Resize to 1600 px","mode":"background","selection":"many","script":false,"mime":["image/*"]}],"errors":[],"error":""}"#);
}

fn checksums() -> Contribution {
    Contribution {
        plugin: "mtorvik.checksums".into(),
        action: "verify".into(),
        label: "Verify Checksum…".into(),
        glyph: "hash".into(),
        menus: vec![Menu::File],
        when: When {
            mime: Vec::new(),
            min_items: 1,
            max_items: None,
            folders: true,
        },
    }
}

fn file_item(mime: &str) -> Item {
    Item {
        mime: mime.into(),
        folder: false,
    }
}

#[test]
fn mn2_a_menu_open_discovers_for_its_own_snapshot() {
    let mut actions = Actions::default();
    listed(&mut actions);
    let row = actions.actions_entry(&[]).unwrap();
    assert_eq!(row.submenu.unwrap().len(), 1);
    assert!(actions.take_said().is_empty());
}

#[test]
fn mn2_a_stale_listing_never_reaches_the_rows() {
    let mut actions = Actions::default();
    actions
        .menu_opened(vec!["/fx/a.jpg".into()])
        .expect("a one-path menu discovers");
    actions.receive(r#"{"t":"actions","id":9,"actions":[{"id":"action:old.toml","name":"Old","mode":"background","selection":"many","script":false}],"errors":[],"error":""}"#);
    assert!(actions.actions_entry(&[]).is_none());
    assert!(
        actions.take_said().is_empty(),
        "a superseded reply says nothing"
    );
}

#[test]
fn mn2_a_refused_listing_is_said_through_its_key() {
    let i18n = english();
    let mut actions = Actions::default();
    actions
        .menu_opened(vec!["relative/path".into()])
        .expect("even a relative menu asks; the backend refuses");
    actions.receive(r#"{"t":"actions","id":1,"actions":[],"errors":[],"error":"Actions require absolute selection paths."}"#);
    assert!(actions.actions_entry(&[]).is_none());
    assert_eq!(
        actions.take_said(),
        [Said::Key {
            key: "actions.error.paths",
            args: vec![]
        }]
    );
    assert_eq!(
        i18n.tr("actions.error.paths"),
        "Actions require absolute selection paths."
    );
}

#[test]
fn mn2_a_huge_selection_skips_discovery_quietly() {
    let mut actions = Actions::default();
    let paths: Vec<String> = (0..5000).map(|n| format!("/fx/{n}.jpg")).collect();
    assert_eq!(actions.menu_opened(paths), None);
    assert!(actions.actions_entry(&[]).is_none());
    assert!(actions.take_said().is_empty());
}

#[test]
fn mn2_a_choice_runs_explicitly_under_its_own_id() {
    let mut actions = Actions::default();
    listed(&mut actions);
    let run = actions.run(
        "action:resize.toml",
        vec!["/fx/a.jpg".into(), "/fx/b.jpg".into()],
    );
    assert_eq!(
        run,
        r#"{"c":"actions","op":"run","id":2,"action":"action:resize.toml","paths":["/fx/a.jpg","/fx/b.jpg"]}"#
    );
}

#[test]
fn mn2_a_refused_run_is_said_and_a_terminal_failure_stays_data() {
    let mut actions = Actions::default();
    actions.run_sent(2);
    actions.run_sent(3);
    actions.receive(r#"{"t":"actionstarted","requestId":2,"ok":false,"error":"That action no longer matches the selection."}"#);
    actions.receive(
        r#"{"t":"actionstarted","requestId":3,"ok":false,"error":"empty terminal action"}"#,
    );
    assert_eq!(
        actions.take_said(),
        [
            Said::Key {
                key: "actions.error.stale",
                args: vec![]
            },
            Said::Key {
                key: "actions.error.start",
                args: vec!["empty terminal action".into()],
            },
        ]
    );
}

#[test]
fn mn2_an_exited_run_says_its_status() {
    let mut actions = Actions::default();
    actions.run_sent(2);
    actions.receive(r#"{"t":"actionstarted","requestId":2,"ok":false,"error":"Action exited with exit status: 2."}"#);
    assert_eq!(
        actions.take_said(),
        [Said::Key {
            key: "actions.error.exited",
            args: vec!["exit status: 2".into()],
        }]
    );
}

#[test]
fn mn2_a_broken_manifest_is_said_through_its_key() {
    let i18n = english();
    let mut actions = Actions::default();
    actions
        .menu_opened(vec!["/fx/a.jpg".into()])
        .expect("a one-path menu discovers");
    actions.receive(r#"{"t":"actions","id":1,"actions":[],"errors":[{"file":"broken.toml","error":"unknown manifest key"}],"error":""}"#);
    assert_eq!(
        actions.take_said(),
        [Said::Key {
            key: "actions.manifest.unknown_key",
            args: vec!["broken.toml".into()],
        }]
    );
    assert_eq!(
        i18n.tr_args("actions.manifest.unknown_key", &["broken.toml"]),
        "broken.toml names an unknown manifest key."
    );
}

#[test]
fn mn2_a_preview_names_the_cursor_row_and_a_newer_one_supersedes_quietly() {
    let mut actions = Actions::default();
    let first = actions.preview("action:inspect.toml", "/fx/a.jpg".into());
    assert_eq!(
        first,
        r#"{"c":"actions","op":"preview","id":1,"action":"action:inspect.toml","paths":["/fx/a.jpg"]}"#
    );
    actions.preview("action:inspect.toml", "/fx/b.jpg".into());
    actions.receive(r#"{"t":"actionpreview","id":1,"ok":true,"text":"1600x1200","error":""}"#);
    assert_eq!(
        actions.preview_text(),
        None,
        "the superseded preview never lands"
    );
    assert!(actions.take_said().is_empty());
    actions.receive(r#"{"t":"actionpreview","id":2,"ok":true,"text":"800x600","error":""}"#);
    assert_eq!(actions.preview_text(), Some("800x600"));
}

#[test]
fn mn2_a_failed_preview_is_said_and_closing_drops_the_preview_wait() {
    let mut actions = Actions::default();
    actions.preview("action:inspect.toml", "/fx/a.jpg".into());
    actions.receive(
        r#"{"t":"actionpreview","id":1,"ok":false,"text":"","error":"Preview output timed out."}"#,
    );
    assert_eq!(
        actions.take_said(),
        [Said::Key {
            key: "actions.preview.timed_out",
            args: vec![]
        }]
    );
    let mut waiting = Actions::default();
    waiting
        .menu_opened(vec!["/fx/a.jpg".into()])
        .expect("a one-path menu discovers");
    waiting.preview("action:inspect.toml", "/fx/a.jpg".into());
    waiting.menu_closed();
    waiting.receive(r#"{"t":"actions","id":1,"actions":[],"errors":[],"error":""}"#);
    assert!(
        waiting.actions_entry(&[]).is_none(),
        "the dropped listing never lands"
    );
    waiting.preview_closed();
    waiting.receive(r#"{"t":"actionpreview","id":2,"ok":true,"text":"late","error":""}"#);
    assert_eq!(waiting.preview_text(), None);
    assert!(waiting.take_said().is_empty());
}

#[test]
fn mn2_a_menu_open_keeps_the_shown_preview() {
    let mut actions = Actions::default();
    actions.preview("action:inspect.toml", "/fx/a.jpg".into());
    actions.receive(r#"{"t":"actionpreview","id":1,"ok":true,"text":"1600x1200","error":""}"#);
    assert_eq!(actions.preview_text(), Some("1600x1200"));
    actions
        .menu_opened(vec!["/fx/a.jpg".into()])
        .expect("a one-path menu discovers");
    assert_eq!(
        actions.preview_text(),
        Some("1600x1200"),
        "a context menu on the previewed file keeps the preview"
    );
}

#[test]
fn mn2_a_pending_preview_outlives_an_unrelated_menu() {
    let mut actions = Actions::default();
    actions.preview("action:inspect.toml", "/fx/a.jpg".into());
    actions
        .menu_opened(vec!["/fx/a.jpg".into()])
        .expect("a one-path menu discovers");
    actions.menu_closed();
    actions.receive(r#"{"t":"actionpreview","id":1,"ok":true,"text":"1600x1200","error":""}"#);
    assert_eq!(actions.preview_text(), Some("1600x1200"));
}

#[test]
fn mn2_repeated_opens_return_one_line_each_and_keep_nothing() {
    let mut actions = Actions::default();
    let paths: Vec<String> = (0..1000).map(|n| format!("/fx/{n}.jpg")).collect();
    let mut ids = Vec::new();
    for _ in 0..200 {
        let line = actions.menu_opened(paths.clone()).expect("under the cap");
        let id: usize = line
            .split("\"id\":")
            .nth(1)
            .and_then(|rest| rest.split(',').next())
            .and_then(|n| n.parse().ok())
            .expect("a list id");
        ids.push(id);
    }
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 200, "every open sends its own discovery line");
    assert!(actions.take_said().is_empty());
}

#[test]
fn mn2_the_background_menu_gains_the_agent_and_no_actions() {
    let actions = Actions::default();
    let agent = actions.agent_entry(&[]).unwrap();
    assert_eq!(agent.id.as_ref(), "openAgent");
    assert!(
        actions.actions_entry(&[]).is_none(),
        "undiscovered means no row"
    );
}

#[test]
fn mn2_plugin_rows_come_only_from_eligible_contributions() {
    let mut actions = Actions::default();
    actions.set_contributions(vec![checksums()]);
    let one = [file_item("text/plain")];
    let file = actions.plugin_entries(&[], false, &one);
    assert_eq!(file.len(), 1);
    assert_eq!(file[0].id.as_ref(), "mtorvik.checksums:verify");
    assert!(actions.plugin_entries(&[], true, &one).is_empty());
    assert!(actions.plugin_entries(&[], false, &[]).is_empty());
    assert!(actions
        .plugin_entries(&["mtorvik.checksums".into()], false, &one)
        .is_empty());
}

#[gpui::test]
fn mn2_every_emitted_mark_is_drawn_or_handed_to_the_join(cx: &mut TestAppContext) {
    use crate::g2::mn1::lines::{self, Env, Lead};
    use crate::list::Typeface;
    use flea::i18n::{I18n, Paths};
    use flea::ui_logic_mn2::rows;
    use gpui::{div, prelude::*, Render};
    struct Root;
    impl Render for Root {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div()
        }
    }
    let mut actions = Actions::default();
    listed(&mut actions);
    actions.set_contributions(vec![checksums()]);
    let one = [file_item("image/png")];
    let mut entries = Vec::new();
    entries.push(actions.agent_entry(&[]).unwrap());
    entries.push(actions.actions_entry(&[]).unwrap());
    entries.extend(actions.plugin_entries(&[], false, &one));
    let window = cx.add_window(|_, _| Root);
    window
        .update(cx, |_, window, _| {
            let catalog = I18n::load(Paths {
                omarchy: "/nonexistent".into(),
                language_file: "/nonexistent".into(),
                catalogs: vec![crate::test_catalogs::english_only()],
            });
            let typeface = Typeface::omarchy_default();
            let env = Env {
                catalog: &catalog,
                preset: "default",
                key_hints: false,
                typeface: &typeface,
                inset: 4.,
            };
            let built = lines::build(
                &entries,
                &env,
                window,
                flea::ui_logic_mn1::fit::MENU_W,
                1100.,
            );
            assert_eq!(built.lines.len(), entries.len());
            for (entry, line) in entries.iter().zip(built.lines.iter()) {
                let Some(name) = entry.glyph else { continue };
                let drawn = matches!(line.lead, Lead::Cut(_));
                let handed = rows::mark_path(name).is_some() || crate::g2::mn1::glyphs::known(name);
                assert!(drawn || handed, "{name} draws nowhere");
                if entry.id.as_ref() == "openAgent" {
                    assert!(handed, "the bot mark reaches the join");
                }
                if matches!(entry.id.as_ref(), "openAgent" | "actions") {
                    assert!(drawn, "{name} is in the menu's own cut table");
                }
            }
        })
        .unwrap();
}

#[test]
fn int3_only_a_held_run_has_its_refusal_said_once() {
    let mut actions = Actions::default();
    let refused =
        r#"{"t":"actionstarted","requestId":9,"ok":false,"error":"empty terminal action"}"#;
    actions.receive(refused);
    assert!(
        actions.take_said().is_empty(),
        "a run this window never sent is not said"
    );
    actions.run_sent(9);
    actions.receive(r#"{"t":"actionstarted","requestId":9,"id":40,"ok":true,"error":""}"#);
    assert!(actions.take_said().is_empty(), "a started run is quiet");
    actions.receive(refused);
    actions.receive(refused);
    assert_eq!(
        actions.take_said().len(),
        1,
        "a held run's refusal is said once"
    );
}

#[path = "uish_misc_tests.rs"]
mod uish_misc;
