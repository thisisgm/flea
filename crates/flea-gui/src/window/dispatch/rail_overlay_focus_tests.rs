//! Overlays opened from the rail hand the keyboard back to the rail row when they close.
use super::*;

// EY-358: Settings and the key sheet opened from the rail close back to the rail row, as v0.3.12 does.
#[gpui::test]
fn fix_views_2_ey358_settings_and_the_key_sheet_close_back_to_the_rail_row(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx, true);
    let (index, _) = row(&shell, cx, Kind::Favourite);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.rail_focus(true, cx);
            shell
                .rail
                .as_ref()
                .unwrap()
                .places
                .update(cx, |producer, cx| producer.set_cursor(index, cx));
            window.focus(&shell.dual_focus.rail, cx);
        })
    });
    settle(cx);
    for (key, opened, sheet, handover) in [
        (",", "settings", false, false),
        ("?", "sheet", true, false),
        (",", "sheet from Settings", true, true),
    ] {
        cx.simulate_keystrokes(key);
        settle(cx);
        if handover {
            // Settings' keyboard row queues Close then OpenSheet, as panel.rs does.
            let view = shell.read_with(cx, |shell, _| shell.settings.panel.clone().unwrap());
            view.update(cx, |view, cx| {
                let row = flea::ui_logic_settings_pages::rows::Row::action(
                    "keyboardSheet",
                    "settings.title",
                    "",
                    Default::default(),
                );
                view.panel.rows.push(row);
                view.panel.activate(view.panel.rows.len() - 1);
                for request in view.panel.take_requests() {
                    cx.emit(request);
                }
            });
            settle(cx);
        }
        shell.read_with(cx, |shell, _| {
            assert_eq!(
                (shell.settings.opened, shell.settings.sheet.is_some()),
                (!sheet, sheet),
                "{key} opens the {opened} alone"
            );
        });
        cx.simulate_keystrokes("escape");
        settle(cx);
        cx.update(|window, cx| {
            let shell = shell.read(cx);
            assert!(
                !shell.settings.opened && shell.settings.sheet.is_none(),
                "escape closes the {opened}"
            );
            assert!(
                shell.dual_focus.rail.contains_focused(window, cx),
                "the {opened} returns the keyboard to the rail"
            );
            assert_eq!(shell.keyboard.context, flea::keys::Context::Rail);
            let producer = shell.rail.as_ref().unwrap().places.read(cx);
            assert!(producer.model.borrow().focused, "the rail row is lit");
            assert_eq!(producer.cursor(), Some(index));
        });
        assert_eq!(read(&shell, cx, "focusView", &[]), "rail");
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
