use super::{edit, frame, key, open, reply, Handle};
use gpui::{point, px, MouseButton, TestAppContext, VisualTestContext};

fn image(cx: &mut TestAppContext, handle: Handle) {
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"image","paths":[],"token":""}"#,
    );
}

#[gpui::test]
fn paste_image_ctrl_v_uses_png_after_browser_offer(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    image(cx, handle);
    lines();
    key(cx, handle, "ctrl-v");
    let sent = lines();
    assert!(
        sent.iter().any(|line| line.contains(r#""c":"pasteimage""#)),
        "{sent:?}"
    );
}

#[gpui::test]
fn paste_image_menu_dispatches_png_without_file_paths(cx: &mut TestAppContext) {
    menu_paste(cx, Some(0), false);
}

#[gpui::test]
fn paste_image_empty_space_menu_matches_ctrl_v(cx: &mut TestAppContext) {
    menu_paste(cx, None, false);
}

#[gpui::test]
fn paste_image_empty_folder_menu_matches_ctrl_v(cx: &mut TestAppContext) {
    menu_paste(cx, None, true);
}

fn menu_paste(cx: &mut TestAppContext, row: Option<usize>, empty_folder: bool) {
    let (handle, mut lines) = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.set_view(crate::window::startup::View::List, window, cx);
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"keys":"default","menu":{"hidden":[]}}"#).unwrap();
    });
    let dest = if empty_folder { "/empty" } else { "/files" };
    if empty_folder {
        edit(cx, handle, |shell, _, _| {
            let pane = &mut shell.panes[0].as_mut().unwrap().pane;
            assert!(pane.begin(dest.into(), Default::default(), pane.now()));
        });
        reply(cx, handle, r#"{"t":"listed","path":"/empty","n":0}"#);
        reply(
            cx,
            handle,
            r#"{"t":"rows","start":0,"listing":4,"rows":[]}"#,
        );
    }
    image(cx, handle);
    lines();
    frame(cx, handle);
    let at = edit(cx, handle, |shell, _, _| {
        let input = &shell.panes[0].as_ref().unwrap().input;
        let bounds = input.frame.expect("the listing drew").bounds;
        let x = bounds.left() + px(40.);
        let top = f32::from(bounds.top());
        let ys = (0..f32::from(bounds.size.height) as usize)
            .map(|dy| top + dy as f32)
            .filter(|&y| input.hit(point(x, px(y))) == row)
            .collect::<Vec<_>>();
        let y = if row.is_some() {
            (ys.first().unwrap() + ys.last().unwrap()) / 2.
        } else {
            *ys.last()
                .expect("the listing has empty space below its rows")
        };
        point(x, px(y))
    });
    let visual = &mut VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_down(at, MouseButton::Right, Default::default());
    visual.simulate_mouse_up(at, MouseButton::Right, Default::default());
    frame(cx, handle);
    let id = edit(cx, handle, |shell, _, _| {
        let capture = shell
            .menu
            .captured
            .as_ref()
            .expect("right click opens the menu");
        assert_eq!(capture.background, row.is_none());
        capture.id
    });
    if row.is_some() {
        for op in ["snapshot", "applications"] {
            reply(
                cx,
                handle,
                &format!(
                    r#"{{"t":"menuaction","id":{id},"op":"{op}","ok":true,"kinds":[32768],"applications":[]}}"#
                ),
            );
        }
    }
    paste_point(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let sheet = shell.sheet_menus();
        assert!(
            !sheet
                .iter()
                .find(|entry| entry.action == "paste")
                .unwrap()
                .disabled
        );
        assert!(
            sheet
                .iter()
                .find(|entry| entry.action == "pasteAs")
                .unwrap()
                .disabled
        );
    });
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"none","paths":[],"token":""}"#,
    );
    edit(cx, handle, |shell, _, _| {
        let menu = shell.menu.menu.borrow();
        let card = menu.card.as_ref().unwrap();
        assert!(
            card.entries
                .iter()
                .find(|row| row.action == "paste")
                .unwrap()
                .disabled
        );
    });
    image(cx, handle);
    let at = paste_point(cx, handle);
    lines();
    let visual = &mut VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_down(at, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(at, MouseButton::Left, Default::default());
    frame(cx, handle);
    let sent = lines();
    let sent = if row.is_some() {
        assert!(
            sent.iter()
                .any(|line| line.contains("validate") && line.contains("paste")),
            "{sent:?}"
        );
        reply(
            cx,
            handle,
            &format!(
                r#"{{"t":"menuaction","id":{id},"op":"validate","ok":true,"action":"paste","paths":["/files/a.txt"]}}"#
            ),
        );
        lines()
    } else {
        sent
    };
    assert_image_request(&sent, dest);
    key(cx, handle, "ctrl-v");
    assert_image_request(&lines(), dest);
}

fn paste_point(cx: &mut TestAppContext, handle: Handle) -> gpui::Point<gpui::Pixels> {
    edit(cx, handle, |shell, _, _| {
        let menu = shell.menu.menu.borrow();
        let card = menu.card.as_ref().expect("the listing menu opened");
        let index = card
            .entries
            .iter()
            .position(|row| row.action == "paste")
            .unwrap();
        let row = &card.entries[index];
        assert!(
            !row.disabled,
            "PNG offers must enable Paste without enabling Paste As"
        );
        assert!(card.entries.iter().all(|row| row.action != "pasteAs"));
        point(px(card.frame.x + 50.), px(card.row_screen_top(index) + 12.))
    })
}

fn assert_image_request(sent: &[String], dest: &str) {
    let image = sent
        .iter()
        .find(|line| flea::json::field_str(line, "c").as_deref() == Some("pasteimage"))
        .unwrap_or_else(|| panic!("Paste must request the clipboard PNG: {sent:?}"));
    assert_eq!(flea::json::field_str(image, "dest").as_deref(), Some(dest));
    let name = flea::json::field_str(image, "name").unwrap();
    assert!(name.starts_with("Pasted image ") && name.ends_with(".png"));
    assert!(
        !sent
            .iter()
            .any(|line| line.contains("transfer") || line.contains("collisions")),
        "{sent:?}"
    );
}
