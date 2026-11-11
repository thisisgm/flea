//! The Actions and plugin choices through a real mn1 `Menu`: each chosen leaf reaches the host under its own route.
use super::model::{Actions, Said, Sent};
use crate::g2::context_menus::lines::Env;
use crate::g2::context_menus::menu::{Face, Menu, Outcome};
use crate::list::Typeface;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_context_menus::entry::Entry;
use flea::ui_logic_context_menus::fit::Rect;
use flea::ui_logic_custom_actions::plugins::{Contribution, Item, Menu as Where, When};
use flea::ui_logic_custom_actions::rows;
use gpui::{div, prelude::*, Render, TestAppContext};

struct Root;

impl Render for Root {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
    }
}

// Opens a File menu over `entries`, presses each key in turn, and returns the last outcome.
fn press(cx: &mut TestAppContext, entries: &[Entry], keys: &[&str]) -> Outcome {
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
            let work = Rect {
                x: 0.,
                y: 0.,
                w: 1100.,
                h: 700.,
            };
            let mut menu = Menu::default();
            menu.open(
                Face::File,
                entries.to_vec(),
                (40., 40.),
                work,
                "a",
                &env,
                window,
            );
            let mut last = Outcome::Nothing;
            for key in keys {
                last = menu.act(key, Some(entries), "a", &env, window);
            }
            last
        })
        .unwrap()
}

fn contribution(plugin: &str, label: &str) -> Contribution {
    Contribution {
        plugin: plugin.into(),
        action: "run".into(),
        label: label.into(),
        glyph: "hash".into(),
        menus: vec![Where::File],
        when: When {
            mime: Vec::new(),
            min_items: 1,
            max_items: None,
            folders: true,
        },
    }
}

// One preview-only action and one background action, the first listed first in the flyout.
fn listed(actions: &mut Actions) {
    actions
        .menu_opened(vec!["/fx/a.jpg".into()])
        .expect("a one-path menu discovers");
    actions.receive(r#"{"t":"actions","id":1,"actions":[{"id":"action:inspect.toml","name":"Inspect","mode":"preview","selection":"one","script":false},{"id":"action:resize.toml","name":"Resize to 1600 px","mode":"background","selection":"many","script":false}],"errors":[],"error":""}"#);
}

#[gpui::test]
fn mn2_r2_a_preview_leaf_reaches_the_host_as_a_preview_and_a_run_leaf_as_a_run(
    cx: &mut TestAppContext,
) {
    let mut actions = Actions::default();
    listed(&mut actions);
    let entries = vec![actions.actions_entry(&[]).expect("the Actions row")];

    let outcome = press(cx, &entries, &["menuRight", "open"]);
    let Outcome::Fire { action, leaf } = outcome else {
        panic!("a flyout leaf fires, got {outcome:?}");
    };
    assert_eq!(
        action,
        rows::FIRE,
        "the menu fires under the parent row's verb"
    );
    assert_eq!(leaf, "action:inspect.toml");
    assert_eq!(
        actions.choose(&leaf, vec!["/fx/a.jpg".into()], "/fx/a.jpg".into()),
        Some(Sent::Preview(
            r#"{"c":"actions","op":"preview","id":2,"action":"action:inspect.toml","paths":["/fx/a.jpg"]}"#.into()
        )),
        "a preview leaf names the cursor row and never the selection"
    );

    let outcome = press(cx, &entries, &["menuRight", "cursorDown", "open"]);
    let Outcome::Fire { action, leaf } = outcome else {
        panic!("a flyout leaf fires, got {outcome:?}");
    };
    assert_eq!(
        (action.as_str(), leaf.as_str()),
        (rows::FIRE, "action:resize.toml")
    );
    assert_eq!(
        actions.choose(&leaf, vec!["/fx/a.jpg".into()], "/fx/a.jpg".into()),
        Some(Sent::Run(
            r#"{"c":"actions","op":"run","id":3,"action":"action:resize.toml","paths":["/fx/a.jpg"]}"#.into()
        ))
    );
}

#[gpui::test]
fn mn2_r2_a_chosen_plugin_row_names_its_plugin_and_action(cx: &mut TestAppContext) {
    let mut actions = Actions::default();
    actions.set_contributions(vec![
        contribution("a.sums", "Verify A"),
        contribution("b.sums", "Verify B"),
    ]);
    let items = [Item {
        mime: "text/plain".into(),
        folder: false,
    }];
    let entries = actions.plugin_entries(&[], false, &items);
    assert_eq!(entries.len(), 2);

    let outcome = press(cx, &entries, &["cursorDown", "open"]);
    assert_eq!(
        outcome,
        Outcome::Fire {
            action: "b.sums:run".into(),
            leaf: String::new()
        },
        "the second row fires under its own plugin and action"
    );
    assert_eq!(actions.plugin_choice("b.sums:run"), Some(("b.sums", "run")));
    assert_eq!(actions.plugin_choice("a.sums:run"), Some(("a.sums", "run")));
    assert_eq!(
        actions.plugin_choice("c.sums:run"),
        None,
        "an unheld plugin names nothing"
    );
}

#[test]
fn mn2_r2_a_choice_the_listing_no_longer_names_is_said_stale() {
    let mut actions = Actions::default();
    listed(&mut actions);
    assert_eq!(
        actions.choose(
            "action:gone.toml",
            vec!["/fx/a.jpg".into()],
            "/fx/a.jpg".into()
        ),
        None
    );
    assert_eq!(
        actions.take_said(),
        [Said::Key {
            key: "actions.error.stale",
            args: vec![]
        }]
    );
}
