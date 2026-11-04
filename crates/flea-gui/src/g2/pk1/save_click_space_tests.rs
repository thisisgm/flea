//! A save chooser's row click hands the keyboard to the listing, so Space names Filename after the clicked file.
use super::ipc::PickerGeometry;
use super::process::Line;
use super::tests::fixture::fixture;
use gpui::{px, Modifiers, TestAppContext};

#[gpui::test]
fn picker_gm_save_click_then_space_names_the_file(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"mode":"save","folder":"/d","name":"draft.txt"}"#,
        None,
    );
    f.cx.simulate_resize(gpui::size(px(1100.), px(700.)));
    f.listing.say(Line::Reply(
        r#"{"t":"listed","path":"/d","n":2,"v":1,"w":true}"#.into(),
    ));
    f.listing.say(Line::Reply(r#"{"t":"rows","start":0,"listing":1,"rows":[{"n":"sub","d":true,"s":0,"m":0,"p":16877,"i":"folder"},{"n":"notes.txt","d":false,"s":3,"m":0,"p":33188,"i":"text-plain"}]}"#.into()));
    f.cx.run_until_parked();
    f.cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let centre = f.cx.update(|_, cx| {
        let picker = f.picker.read(cx);
        super::geometry::Live { picker, cx }.row_centre(1)
    });
    let xy: Vec<f32> = centre
        .replace(',', " ")
        .split_whitespace()
        .map(|part| part.parse().unwrap())
        .collect();
    assert_eq!(xy.len(), 2, "notes.txt has a drawn centre: {centre}");
    f.cx.simulate_click(gpui::point(px(xy[0]), px(xy[1])), Modifiers::none());
    f.cx.run_until_parked();
    assert_eq!(f.read(|picker| picker.listing.cursor), 1);
    f.cx.simulate_keystrokes("space");
    f.cx.run_until_parked();
    assert_eq!(
        f.read(|picker| picker.listing.save_name.clone()),
        "notes.txt"
    );
}
