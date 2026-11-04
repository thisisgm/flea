//! Frozen reader contracts through the production Root's mounted adapter chain.
use super::pb_ipc_fixture::{draw, open, read};
use crate::state::Shell;
use gpui::{point, px, Entity, Modifiers, Pixels, Point, TestAppContext, VisualTestContext};

#[gpui::test]
fn pb_ipc_share_browser_cursor_follows_the_live_browser(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    // EY-242: v0.3.12 answered -1 until its ShareBrowser Loader first loaded, and shareBrowserState agrees.
    assert_eq!(read(&shell, cx, "shareBrowserCursor", &[]), "-1");
    assert!(read(&shell, cx, "shareBrowserState", &[]).contains("\"cursor\":-1,"));
    shell.update_in(cx, |shell, window, cx| {
        let origin = shell.network_origin(0);
        let shares = shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares
            .clone();
        shares.update(cx, |view, cx| {
            view.show(
                "smb://server".into(),
                "Server".into(),
                vec!["docs".into(), "media".into()],
                origin,
                window,
                cx,
            )
        });
        cx.notify();
    });
    draw(cx);
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(read(&shell, cx, "shareBrowserCursor", &[]), "1");
    assert!(read(&shell, cx, "shareBrowserState", &[]).contains("\"cursor\":1,"));
    assert_eq!(read(&shell, cx, "shareBrowserOpen", &[]), "true");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "shareBrowserOpen", &[]),
        "false",
        "escape closes the browser"
    );
    assert_eq!(
        read(&shell, cx, "shareBrowserCursor", &[]),
        "1",
        "a mounted closed browser retains its cursor"
    );
}

/// Publishes one mounted SMB share, draws it, and returns the release mark's centre and the row's index.
fn publish_docs_mark(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (Point<Pixels>, String) {
    shell.update(cx, |shell, cx| {
        shell.network_publish(
            Some(vec![flea::ui_logic_pl1::RailEntry {
                uri: "smb://server/docs".into(),
                path: "/media/docs".into(),
                label: "Docs".into(),
                mounted: true,
                group: flea::ui_logic_pl1::Group::Network,
                kind: flea::ui_logic_pl1::Kind::Share,
                ..Default::default()
            }]),
            cx,
        );
    });
    draw(cx);
    let row = shell.read_with(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .read(cx)
            .laid
            .borrow()
            .rows
            .iter()
            .find(|row| row.label == "Docs")
            .unwrap()
            .clone()
    });
    let hit = row
        .eject_hit()
        .expect("the mounted row draws the release mark");
    let centre = point(px(hit.x + hit.width / 2.), px(hit.y + hit.height / 2.));
    (centre, row.index.to_string())
}

#[gpui::test]
fn pb_ipc_eject_hover_reads_the_pointer_on_the_actual_mark(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    assert_eq!(read(&shell, cx, "railMarkHovered", &["-1"]), "false");
    assert_eq!(read(&shell, cx, "railMarkHovered", &["9999"]), "false");
    let (centre, index) = publish_docs_mark(&shell, cx);
    assert_eq!(read(&shell, cx, "railMarkHovered", &[&index]), "false");
    cx.simulate_mouse_move(centre, None, Modifiers::none());
    draw(cx);
    assert_eq!(read(&shell, cx, "railMarkHovered", &[&index]), "true");
    assert_eq!(read(&shell, cx, "railMarkHovered", &["-1"]), "false");
}

#[gpui::test]
fn pb_ipc_eject_hover_clears_when_the_pointer_leaves_the_rail(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let (centre, index) = publish_docs_mark(&shell, cx);
    cx.simulate_mouse_move(centre, None, Modifiers::none());
    draw(cx);
    assert_eq!(read(&shell, cx, "railMarkHovered", &[&index]), "true");
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(read(&shell, cx, "railMarkHovered", &[&index]), "true");
    cx.simulate_mouse_move(point(px(700.), px(400.)), None, Modifiers::none());
    draw(cx);
    assert_eq!(read(&shell, cx, "railMarkHovered", &[&index]), "false");
}
