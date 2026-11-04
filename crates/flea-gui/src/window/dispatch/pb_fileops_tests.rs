//! Parity readers and disabled menu rows through the production Root and its input handlers.
use super::{
    menus_tests::{answer, choose, validated},
    preview_tests::{self, draw, file},
    test_shell::{land, open_root, set_cursor},
};
use crate::window::u07_tests::read;
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;

#[gpui::test]
fn pb_fileops_duplicate_is_hidden_over_a_multiple_selection(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![file("a.txt", "text-plain"), file("b.txt", "text-plain")],
        1,
    );
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![0, 1]
    });
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[0o100644, 0o100644]);
    let rows = jsondoc::parse(&read(&shell, cx, "contextMenuModel", &[])).unwrap();
    let actions: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry.get("action").and_then(Json::as_str))
        .collect();
    assert!(
        actions.contains(&"rename"),
        "the menu opened over the two selected files"
    );
    assert!(
        !actions.contains(&"duplicate"),
        "Duplicate acts on one item, so a wider selection hides it (HIG CHANGES.md, GM 2026-10-17)"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn rectangle(bounds: gpui::Bounds<gpui::Pixels>) -> String {
    format!(
        "{} {} {} {}",
        f32::from(bounds.left()).round(),
        f32::from(bounds.top()).round(),
        f32::from(bounds.size.width).round(),
        f32::from(bounds.size.height).round()
    )
}

#[gpui::test]
#[ignore = "REMAINING PB-FILEOPS: convertCardRect and convertTitleCentre need observations.rs mounted and a conversion model"]
fn pb_fileops_convert_geometry_reads_the_mounted_menu_card(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![file("photo.png", "image-x-generic")],
        1,
    );
    for name in ["convertCardRect", "convertTitleCentre"] {
        assert_eq!(
            read(&shell, cx, name, &[]),
            "",
            "no conversion card has been built"
        );
    }
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[0o100644]);
    choose(&shell, cx, "convert");
    validated(&shell, cx, "convert", &["/files/photo.png"]);
    draw(cx);
    for (name, selector, centre) in [
        ("convertCardRect", "op4-card", false),
        ("convertTitleCentre", "op4-title", true),
    ] {
        let bounds = cx.debug_bounds(selector).expect("the output card painted");
        let expected = if centre {
            format!(
                "{} {}",
                f32::from(bounds.center().x).round(),
                f32::from(bounds.center().y).round()
            )
        } else {
            rectangle(bounds)
        };
        assert_eq!(
            read(&shell, cx, name, &[]),
            expected,
            "{name} reads the actual frame"
        );
    }
    let before = read(&shell, cx, "convertCardRect", &[]);
    cx.simulate_resize(gpui::size(gpui::px(720.), gpui::px(600.)));
    draw(cx);
    assert_eq!(
        read(&shell, cx, "convertCardRect", &[]),
        rectangle(cx.debug_bounds("op4-card").unwrap())
    );
    assert_ne!(
        read(&shell, cx, "convertCardRect", &[]),
        before,
        "resizing updates the reader"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        !read(&shell, cx, "convertCardRect", &[]).is_empty(),
        "the built dialog retains its geometry"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_fileops_preview_archive_names_follow_space_and_metadata(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = preview_tests::opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![
            file("pack.zip", "package-x-generic"),
            file("plain.txt", "text-plain"),
        ],
        1,
    );
    assert_eq!(read(&shell, cx, "previewArchiveNames", &[]), "");
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    let asks = preview_tests::sent(&mut requests, "meta");
    let token = preview_tests::token(asks.last().expect("Quick Look asks for the archive index"));
    preview_tests::answer(
        &shell,
        cx,
        format!(
            r#"{{"t":"meta","row":0,"entries":3,"unpacked":12,"afailed":false,"names":[{{"n":"docs","d":true}},{{"n":"a.txt","d":false}},{{"n":"a|b.txt","d":false}}],"token":{token}}}"#
        ),
    );
    assert_eq!(
        read(&shell, cx, "previewArchiveNames", &[]),
        "docs|a.txt|a|b.txt"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.simulate_keystrokes("down space");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewArchiveNames", &[]),
        "",
        "a text preview never reuses archive names"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn retained_setting(cx: &mut TestAppContext, name: &str, initial: &str, edited: &str, keys: &str) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![file("photo.png", "image-x-generic")],
        1,
    );
    assert_eq!(
        read(&shell, cx, name, &[]),
        if name == "convertFormat" { "" } else { "false" }
    );
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[0o100644]);
    choose(&shell, cx, "convert");
    validated(&shell, cx, "convert", &["/files/photo.png"]);
    draw(cx);
    assert_eq!(read(&shell, cx, name, &[]), initial);
    cx.simulate_keystrokes(keys);
    draw(cx);
    assert_eq!(read(&shell, cx, name, &[]), edited);
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        read(&shell, cx, name, &[]),
        edited,
        "the closed dialog retains its last setting"
    );
    cx.simulate_keystrokes("ctrl-p");
    draw(cx);
    assert_eq!(
        read(&shell, cx, name, &[]),
        edited,
        "Print cannot replace conversion settings"
    );
    cx.simulate_keystrokes("escape");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
#[ignore = "REMAINING PB-FILEOPS: convertFormat keeps its dialog selection after close; the output model discards it"]
fn pb_fileops_convert_format_retains_the_last_dialog_selection(cx: &mut TestAppContext) {
    retained_setting(cx, "convertFormat", "jpg", "png", "down");
}

#[gpui::test]
#[ignore = "REMAINING PB-FILEOPS: convertStrip keeps its dialog selection after close; the output model discards it"]
fn pb_fileops_convert_strip_retains_the_last_dialog_selection(cx: &mut TestAppContext) {
    retained_setting(cx, "convertStrip", "false", "true", "tab space");
}
