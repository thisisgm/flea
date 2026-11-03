//! The chooser's chrome text draws in the installed face, as its rows do; a run that inherits the toolkit default draws sans.
use super::ipc::PickerGeometry;
use super::tests::fixture::fixture;
use gpui::TestAppContext;

#[gpui::test]
fn picker_gm_title_text_inherits_the_installed_family(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d"}"#, None);
    for family in ["DejaVu Sans Mono", "DejaVu Sans"] {
        f.cx.update(|window, cx| {
            let fonts = crate::presentation::Fonts::from_sources("", "", family.into());
            cx.set_global(fonts.typeface(&flea::jsondoc::Json::Null));
            cx.set_global(fonts);
            window.refresh();
        });
        f.read(|picker| picker.drawn.arm());
        f.cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let members = f.read(|picker| picker.drawn.snapshot_members());
        assert!(
            members.contains(&format!(r#""family":"{family}""#)),
            "the title inherits {family}: {members}"
        );
    }
}
