use super::arabic;
use crate::{
    g2::{op2, op3},
    window::dispatch::rename_tests::ready_root,
};
use gpui::{KeyDownEvent, Keystroke, Modifiers, TestAppContext};

fn key(name: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            key: name.into(),
            key_char: (name.len() == 1).then(|| name.into()),
            us_key: None,
            is_keypad: false,
            modifiers: Modifiers::none(),
        },
        is_held: false,
        prefer_character_input: false,
    }
}

#[gpui::test]
fn i50_rtl_the_production_grid_decoder_swaps_sideways_navigation(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready_root(cx, "default");
    arabic(&shell, cx);
    cx.update(|window, app| {
        shell.update(app, |shell, _| {
            shell.view = crate::window::startup::View::Grid;
            shell.keyboard.context = flea::keys::Context::Listing;
            assert_eq!(
                shell.grid_sideways(&key("left"), window.layout_mirrored()),
                Some("cursorRight")
            );
            assert_eq!(
                shell.grid_sideways(&key("right"), window.layout_mirrored()),
                Some("cursorLeft")
            );
        })
    });
}

#[gpui::test]
fn i50_rtl_the_production_archive_handler_walks_formats_in_reading_order(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready_root(cx, "default");
    arabic(&shell, cx);
    shell.update(cx, |shell, _| {
        let (mut card, _) = op2::compress::Compress::open(
            vec!["/files/notes.txt".into()],
            "/files",
            "files",
            1,
            &["zip", "7z"],
        );
        card.pick_format(0);
        card.focus(op2::compress::Stop::Format);
        shell.operations.archive.compress = Some(card);
    });
    for (physical, expected) in [("left", 1), ("right", 0)] {
        cx.update(|window, app| {
            shell.update(app, |shell, cx| {
                assert!(shell.archive_key(&key(physical), window, cx));
                let card = shell
                    .operations
                    .archive
                    .compress
                    .as_ref()
                    .unwrap()
                    .card(&shell.locale.catalog);
                let selected = card.body.iter().find_map(|block| match block {
                    op2::cards::Block::Chips(chips) => {
                        assert!(chips.options.len() >= 2);
                        Some(chips.current)
                    }
                    _ => None,
                });
                assert_eq!(selected, Some(expected));
            })
        });
    }
}

#[gpui::test]
fn i50_rtl_the_production_run_card_handler_walks_three_buttons(cx: &mut TestAppContext) {
    use flea::ui_logic_op3::runfile::Button;
    let (shell, cx, _lines) = ready_root(cx, "default");
    arabic(&shell, cx);
    shell.update(cx, |shell, cx| {
        let ask = r#"{"ask":"run","kind":"script","size":21,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/files/hello.sh"}"#;
        shell.open_run_card(0, "/files/hello.sh".into(), ask.into(), false, cx);
        shell.run_card.as_mut().unwrap().card.focus = Button::Text;
    });
    for (physical, expected) in [
        ("left", Button::Terminal),
        ("right", Button::Text),
        ("h", Button::Terminal),
        ("l", Button::Text),
    ] {
        cx.update(|window, app| {
            shell.update(app, |shell, cx| {
                assert!(shell.run_card_key(&key(physical), window, cx));
                assert_eq!(shell.run_card.as_ref().unwrap().card.focus, expected);
            })
        });
    }
}

#[gpui::test]
fn i50_rtl_the_production_permissions_handler_walks_its_real_grid(cx: &mut TestAppContext) {
    use flea::jsondoc::parse;
    use op3::permissions::{host::View, Permissions, Stop};
    let catalog = std::sync::Arc::new(op3::tests::english());
    let (mut model, asks) =
        Permissions::open(7, vec!["/files/a".into(), "/files/b".into()], "items");
    for ask in asks {
        let op3::Effect::Send(ask) = ask else {
            panic!("inspect request")
        };
        let ask = parse(&ask).unwrap();
        let id = ask.get("id").unwrap().as_f64().unwrap() as u64;
        let path = ask.get("path").unwrap().as_str().unwrap();
        let reply = format!(
            r#"{{"t":"permissions","id":{id},"op":"inspect","ok":true,"path":"{path}","mode":"0644","reason":""}}"#
        );
        model.reply(&catalog, &parse(&reply).unwrap());
    }
    model.focus = Stop::Box(1);
    let (view, cx) = cx.add_window_view(move |_, cx| View::new(model, catalog, 0., cx));
    cx.update(|window, app| {
        view.update(app, |view, cx| {
            window.set_layout_mirrored(true);
            view.focus(window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("left");
    assert_eq!(view.read_with(cx, |view, _| view.model.focus), Stop::Box(2));
    cx.simulate_keystrokes("right");
    assert_eq!(view.read_with(cx, |view, _| view.model.focus), Stop::Box(1));
}

#[gpui::test]
fn i50_rtl_the_production_rail_adapter_enters_and_leaves_the_eject_mark(cx: &mut TestAppContext) {
    use flea::ui_logic_pl4::{
        order::{Heading, RailSection},
        rail::{Group, Kind, RailEntry},
    };
    let (shell, cx, _lines) = ready_root(cx, "default");
    arabic(&shell, cx);
    let producer = shell.update(cx, |shell, cx| {
        let join = crate::window::rail::mount(
            shell.locale.catalog.clone(),
            &shell.launch.settings.data,
            0,
            cx,
        );
        let producer = join.places.clone();
        shell.slots.rail = Some(gpui::AnyView::from(join.host.clone()));
        shell.rail = Some(join);
        producer
    });
    cx.run_until_parked();
    producer.update(cx, |producer, cx| {
        producer.replace_sections(
            vec![RailSection {
                heading: Heading::None,
                rows: vec![RailEntry {
                    group: Group::Device,
                    kind: Kind::Volume,
                    label: "USB".into(),
                    path: "/fixture/usb".into(),
                    glyph: "drive",
                    releasable: true,
                    ..Default::default()
                }],
            }],
            cx,
        );
        producer.model.borrow_mut().cursor = Some(0);
    });
    shell.update(cx, |shell, _| {
        shell.keyboard.context = flea::keys::Context::Rail;
        shell.rail.as_mut().unwrap().focus_pending = Some(true);
    });
    for (physical, marked) in [("left", true), ("right", false)] {
        cx.update(|window, app| {
            shell.update(app, |shell, cx| {
                assert_eq!(shell.rail_key(&key(physical), window, cx), Some(true));
            })
        });
        assert_eq!(
            producer.read_with(cx, |producer, _| producer.model.borrow().mark_focused),
            marked
        );
    }
}
