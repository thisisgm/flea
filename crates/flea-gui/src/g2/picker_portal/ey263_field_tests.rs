//! EY-263: the save chooser's Save As field holds the supplied name at picker-save's 300 px, never a sliver.
use super::tests::fixture::fixture;
use gpui::{px, size, TestAppContext};

#[gpui::test]
fn ey263_save_field_holds_the_supplied_name_whole(cx: &mut TestAppContext) {
    for width in [1100., 720.] {
        let mut f = fixture(
            cx,
            r#"{"mode":"save","folder":"/d","name":"sa-d3-new.txt"}"#,
            None,
        );
        f.cx.simulate_resize(size(px(width), px(700.)));
        f.cx.run_until_parked();
        let frame =
            f.cx.debug_bounds("pk1-filename")
                .expect("the mounted Save As field");
        assert_eq!(
            f32::from(frame.size.width),
            300.,
            "picker-save's field at {width} px"
        );
        let field = f
            .read(|picker| picker.browse.filename.clone())
            .expect("a save has Filename");
        assert_eq!(
            f.cx.update(|_, cx| field.read(cx).text().to_owned()),
            "sa-d3-new.txt"
        );
    }
}
