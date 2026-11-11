//! S5 saved Markdown source wrap through the real Quick Look mount.
use super::{
    preview_tests::{close, file, opened, press},
    test_shell::land,
};
use crate::g2::quick_look_overlay::doc::View;
use gpui::TestAppContext;

#[gpui::test]
fn s5_markdown_quick_look_uses_saved_wrap_on_each_mount(cx: &mut TestAppContext) {
    crate::g2::doc_preview::render::without_markdown_threads(|| {
        let (shell, cx, _requests) = opened(cx);
        land(
            &shell,
            cx,
            "/files",
            vec![file("notes.md", "text-markdown")],
            1,
        );
        for wrap in [true, false] {
            shell.update(cx, |shell, _| {
                shell.launch.settings.data =
                    flea::jsondoc::parse(&format!(r#"{{"preview":{{"wrap":{wrap}}}}}"#)).unwrap();
            });
            press(cx, "space");
            let actual = shell.read_with(cx, |shell, cx| {
                let card = shell.quick_look.as_ref().unwrap().view.read(cx);
                let doc = card.subject().unwrap().body.doc().unwrap();
                let View::Markdown(view) = &doc.view else {
                    panic!("Markdown is mounted")
                };
                view.read(cx).wrap
            });
            assert_eq!(actual, wrap, "a fresh Markdown card restores preview.wrap");
            press(cx, "escape");
        }
        close(&shell, cx);
    });
}

#[gpui::test]
fn s5_pdf_e_opens_quick_look_when_the_preview_holds_focus(cx: &mut TestAppContext) {
    crate::g2::doc_preview::render::without_threads(|| {
        let (shell, cx, _) = super::pb_preview_fixture::pdf(cx);
        let read = crate::window::u07_tests::read;
        assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
        press(cx, "ctrl-tab");
        let state = flea::jsondoc::parse(&read(&shell, cx, "pdfState", &["false"])).unwrap();
        assert_eq!(
            state.get("focused").and_then(flea::jsondoc::Json::as_bool),
            Some(true)
        );
        press(cx, "e");
        assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
        assert_eq!(read(&shell, cx, "previewKind", &[]), "pdf");
        close(&shell, cx);
    });
}
