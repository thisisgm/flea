//! Parity readers and disabled menu rows through the production Root and its input handlers.
use super::{
    menus_tests::{answer, choose, validated},
    preview_tests::{self, draw, file},
    test_shell::{land, open_root, set_cursor},
};
use crate::window::u07_tests::{press, read};
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;

#[gpui::test]
fn pb_fileops_duplicate_stays_disabled_in_a_multiple_selection(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![file("a.txt", "text-plain"), file("b.txt", "text-plain")], 1);
    shell.update(cx, |shell, _| shell.panes[0].as_mut().unwrap().selected = vec![0, 1]);
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[0o100644, 0o100644]);
    let rows = jsondoc::parse(&read(&shell, cx, "contextMenuModel", &[])).unwrap();
    let (index, duplicate) = rows.as_array().unwrap().iter().enumerate()
        .find(|(_, entry)| entry.get("action").and_then(Json::as_str) == Some("duplicate"))
        .expect("Duplicate stays visible beside Rename 2 Items");
    assert_eq!(duplicate.get("disabled").and_then(Json::as_bool), Some(true));
    let centre = read(&shell, cx, "contextMenuRowCentre", &[&index.to_string()]);
    press(cx, &centre, 1);
    draw(cx);
    assert_eq!(read(&shell, cx, "contextMenuVisible", &[]), "true", "a disabled row cannot run");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn rectangle(bounds: gpui::Bounds<gpui::Pixels>) -> String {
    format!("{} {} {} {}", f32::from(bounds.left()).round(), f32::from(bounds.top()).round(),
        f32::from(bounds.size.width).round(), f32::from(bounds.size.height).round())
}

#[gpui::test]
fn pb_fileops_convert_geometry_reads_the_mounted_menu_card(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![file("photo.png", "image-x-generic")], 1);
    for name in ["convertCardRect", "convertTitleCentre"] {
        assert_eq!(read(&shell, cx, name, &[]), "", "no conversion card has been built");
    }
    cx.simulate_keystrokes("m");
    answer(&shell, cx, &[0o100644]);
    choose(&shell, cx, "convert");
    validated(&shell, cx, "convert", &["/files/photo.png"]);
    draw(cx);
    for (name, selector, centre) in [("convertCardRect", "op4-card", false), ("convertTitleCentre", "op4-title", true)] {
        let bounds = cx.debug_bounds(selector).expect("the output card painted");
        let expected = if centre {
            format!("{} {}", f32::from(bounds.center().x).round(), f32::from(bounds.center().y).round())
        } else { rectangle(bounds) };
        assert_eq!(read(&shell, cx, name, &[]), expected, "{name} reads the actual frame");
    }
    let before = read(&shell, cx, "convertCardRect", &[]);
    cx.simulate_resize(gpui::size(gpui::px(720.), gpui::px(600.)));
    draw(cx);
    assert_eq!(read(&shell, cx, "convertCardRect", &[]), rectangle(cx.debug_bounds("op4-card").unwrap()));
    assert_ne!(read(&shell, cx, "convertCardRect", &[]), before, "resizing updates the reader");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!read(&shell, cx, "convertCardRect", &[]).is_empty(), "the built dialog retains its geometry");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_fileops_preview_archive_names_follow_space_and_metadata(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = preview_tests::opened(cx);
    land(&shell, cx, "/files", vec![file("pack.zip", "package-x-generic"), file("plain.txt", "text-plain")], 1);
    assert_eq!(read(&shell, cx, "previewArchiveNames", &[]), "");
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    let asks = preview_tests::sent(&mut requests, "meta");
    let token = preview_tests::token(asks.last().expect("Quick Look asks for the archive index"));
    preview_tests::answer(&shell, cx, format!(
        r#"{{"t":"meta","row":0,"entries":3,"unpacked":12,"afailed":false,"names":[{{"n":"docs","d":true}},{{"n":"a.txt","d":false}},{{"n":"a|b.txt","d":false}}],"token":{token}}}"#));
    assert_eq!(read(&shell, cx, "previewArchiveNames", &[]), "docs|a.txt|a|b.txt");
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.simulate_keystrokes("down space");
    draw(cx);
    assert_eq!(read(&shell, cx, "previewArchiveNames", &[]), "", "a text preview never reuses archive names");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn retained_setting(cx: &mut TestAppContext, name: &str, initial: &str, edited: &str, keys: &str) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![file("photo.png", "image-x-generic")], 1);
    assert_eq!(read(&shell, cx, name, &[]), if name == "convertFormat" { "" } else { "false" });
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
    assert_eq!(read(&shell, cx, name, &[]), edited, "the closed dialog retains its last setting");
    cx.simulate_keystrokes("ctrl-p");
    draw(cx);
    assert_eq!(read(&shell, cx, name, &[]), edited, "Print cannot replace conversion settings");
    cx.simulate_keystrokes("escape");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_fileops_convert_format_retains_the_last_dialog_selection(cx: &mut TestAppContext) {
    retained_setting(cx, "convertFormat", "jpg", "png", "down");
}

#[gpui::test]
fn pb_fileops_convert_strip_retains_the_last_dialog_selection(cx: &mut TestAppContext) {
    retained_setting(cx, "convertStrip", "false", "true", "tab space");
}
