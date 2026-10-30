//! A failed Restore or Delete in Trash keeps the bar to one line and puts each failure in the detail strip under it.
use super::join_tests::{answer, draw, opened, root, shell};
use crate::list::bench::real_cx;

#[test]
fn trash_font_failed_restore_keeps_the_bar_on_one_line_and_the_failure_in_the_detail() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    opened(&mut cx, window);
    shell(&mut cx, window, |shell, window, cx| {
        let pane = shell.trash.as_ref().unwrap().pane.clone();
        pane.update(cx, |pane, cx| {
            pane.menu_action(Some("restoreTrashSelection"), cx)
        });
        shell.flush_trash(window, cx);
    });
    answer(
        &mut cx,
        window,
        "restore",
        r#","done":0,"failed":1,"failures":[{"uri":"trash:///alpha","name":"alpha.txt","error":"The item could not be restored: already exists. It stays in the Trash."}]"#,
    );
    draw(&mut cx, window);
    shell(&mut cx, window, |shell, _, cx| {
        let line = shell.trash.as_ref().unwrap().messages.read(cx);
        let drawn = line.drawn();
        assert_eq!(
            drawn.primary.as_ref(),
            "Restored 0 of 1 item · 1 failed",
            "the bar says the headline alone, on one line"
        );
        assert!(drawn.error, "a failed restore is an error");
        assert_eq!(
            drawn.detail.as_ref(),
            "alpha.txt failed: The item could not be restored: already exists. It stays in the Trash.",
            "the failure goes to the detail strip under the bar"
        );
        // The stored entry, not only the drawn split: a headline joined to its detail by a newline reads the same once drawn.
        let errors = line.messages().errors();
        assert_eq!(errors.len(), 1, "one failed restore is one error entry");
        assert_eq!(
            errors[0].text, "Restored 0 of 1 item · 1 failed",
            "the entry's text holds the headline alone"
        );
        assert_eq!(
            errors[0].detail,
            "alpha.txt failed: The item could not be restored: already exists. It stays in the Trash.",
            "the entry's detail holds the failure"
        );
    });
}

/// Applies a font-set as the presentation service publishes one, then draws the window once.
fn font_set(
    cx: &mut gpui::TestAppContext,
    window: gpui::WindowHandle<crate::window::root::Root>,
    family: &str,
) {
    shell(cx, window, |shell, window, cx| {
        let (theme, parsed) = crate::palette::Theme::from_text_observed("");
        let fonts =
            crate::presentation::Fonts::from_sources("[font]\nbase-size=14", "", family.into());
        shell.apply_presentation(
            crate::presentation::Snapshot {
                theme,
                parsed,
                fonts,
            },
            window,
            cx,
        );
    });
    draw(cx, window);
    draw(cx, window);
}

#[test]
fn trash_font_every_chrome_surface_follows_a_live_system_font_set() {
    let mut cx = real_cx();
    let window = root(&mut cx);
    shell(&mut cx, window, |shell, _, cx| shell.mount_rail(cx));
    // Non-default faces, each set while the window stays open: the main view first, then the Trash view.
    let families = ["Noto Sans Mono", "Adwaita Mono"];
    for family in families {
        font_set(&mut cx, window, family);
        shell(&mut cx, window, |shell, _, cx| {
            let list = shell.active_typeface();
            assert_eq!(
                list.font.family.as_ref(),
                family,
                "the list follows the font-set"
            );
            let chrome = shell.chrome.view.clone().unwrap();
            let path = chrome.read(cx).path.clone();
            assert_eq!(
                path.read(cx).paint.borrow().typeface.font.family,
                list.font.family,
                "the path bar draws in the list's face"
            );
            let rail = shell.rail.as_ref().expect("the rail is mounted");
            assert_eq!(
                rail.places.read(cx).typeface.font.family,
                list.font.family,
                "the rail draws in the list's face"
            );
            let footer = shell.surfaces[0]
                .footer
                .clone()
                .expect("the status bar is mounted");
            assert_eq!(
                footer.read(cx).line.read(cx).typeface().font.family,
                list.font.family,
                "the status bar draws in the list's face"
            );
        });
    }
    opened(&mut cx, window);
    // The real context-menu key, so the menu opens through open_trash_menu and takes its face from there.
    cx.simulate_keystrokes(window.into(), "m");
    shell(&mut cx, window, |shell, _, _| {
        let menu = shell.trash.as_ref().unwrap().menu.borrow();
        let card = menu.card.as_ref().expect("m opens the Trash menu");
        assert_eq!(
            card.built.typeface.font.family.as_ref(),
            "Adwaita Mono",
            "the Trash menu opens in the list's face"
        );
    });
    for family in families {
        font_set(&mut cx, window, family);
        shell(&mut cx, window, |shell, _, cx| {
            let list = shell.active_typeface();
            assert_eq!(
                list.font.family.as_ref(),
                family,
                "the list follows the font-set"
            );
            let host = shell.trash.as_ref().unwrap();
            assert_eq!(
                host.pane.read(cx).paint.borrow().font.family,
                list.font.family,
                "the Trash rows, strip and header draw in the list's face"
            );
            assert_eq!(
                host.messages.read(cx).typeface().font.family,
                list.font.family,
                "the Trash status line draws in the list's face"
            );
            assert_eq!(
                host.menu
                    .borrow()
                    .card
                    .as_ref()
                    .unwrap()
                    .built
                    .typeface
                    .font
                    .family,
                list.font.family,
                "an already open Trash menu reshapes unchanged entries in the new face"
            );
        });
    }
}
