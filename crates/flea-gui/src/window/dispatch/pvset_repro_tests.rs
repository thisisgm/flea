//! PVSET candidates exercised through the mounted window and its actual keyboard sheet.
use super::previewtypes_tests::pump;
use crate::window::u07_tests::read;
use gpui::TestAppContext;

#[gpui::test]
fn pvset_repro_pdf_pages_the_viewport_without_skipping_a_tall_page(cx: &mut TestAppContext) {
    crate::g2::doc_preview::render::without_threads(|| {
        let (shell, cx, pdf) = super::pb_preview_fixture::pdf(cx);
        pdf.update(cx, |pdf, cx| {
            pdf.surface.sizes = vec![(612., 4000.); 3];
            pdf.surface.set_frame(pdf.frame.0, pdf.frame.1, 1., 9.);
            cx.notify();
        });
        pump(cx);
        for focused in [false, true] {
            if focused {
                cx.simulate_keystrokes("ctrl-tab");
            }
            pump(cx);
            for (forward, back) in [("shift-f->F", "shift-b->B"), ("ctrl-d", "ctrl-u")] {
                if !focused && forward == "ctrl-d" {
                    continue;
                }
                let height = pdf.read_with(cx, |pdf, _| pdf.surface.view_height);
                assert!(height > 0.);
                cx.simulate_keystrokes(forward);
                pump(cx);
                pdf.read_with(cx, |pdf, _| {
                    assert_eq!(pdf.surface.page, 0, "{forward} keeps the tall first page");
                    assert_eq!(pdf.surface.view_top, height, "one viewport of content");
                });
                assert_eq!(read(&shell, cx, "columnPdfPage", &[]), "0");
                cx.simulate_keystrokes(back);
                pump(cx);
                assert_eq!(pdf.read_with(cx, |pdf, _| pdf.surface.view_top), 0.);
                cx.simulate_keystrokes(back);
                pump(cx);
                assert_eq!(pdf.read_with(cx, |pdf, _| pdf.surface.view_top), 0.);
            }
        }
        let height = pdf.read_with(cx, |pdf, _| pdf.surface.view_height);
        cx.simulate_keystrokes("pagedown");
        pump(cx);
        assert_eq!(pdf.read_with(cx, |pdf, _| pdf.surface.view_top), height);
        cx.simulate_keystrokes("pageup");
        pump(cx);
        assert_eq!(pdf.read_with(cx, |pdf, _| pdf.surface.view_top), 0.);
        shell.update(cx, |shell, _| shell.stop_for_app_quit());
    });
}

#[gpui::test]
fn pvset_repro_size_sheet_announces_through_the_painted_footer(cx: &mut TestAppContext) {
    let (shell, cx) = super::test_shell::open_root(cx, "default");
    super::test_shell::land(&shell, cx, "/files", Vec::new(), 1);
    for (query, expected) in [
        (
            "l a r g e r",
            "Text size 16px. Ctrl+Shift+0 follows Omarchy again",
        ),
        ("r e s e t", "Text size follows Omarchy, 14px"),
        (
            "s m a l l e r",
            "Text size 12px. Ctrl+Shift+0 follows Omarchy again",
        ),
    ] {
        shell.update_in(cx, |shell, window, cx| shell.open_sheet(window, cx));
        pump(cx);
        cx.simulate_keystrokes(query);
        cx.simulate_keystrokes("enter");
        pump(cx);
        assert_eq!(read(&shell, cx, "keymapSheetOpen", &[]), "false");
        assert_eq!(read(&shell, cx, "lastMessage", &[]), expected);
        shell.read_with(cx, |shell, cx| {
            let footer = shell.surfaces[0].footer.as_ref().unwrap().read(cx);
            let message = footer.message_line();
            let line = message.read(cx);
            assert_eq!(line.drawn().primary.as_ref(), expected);
            assert_eq!(line.painted.get("primary").unwrap().text.as_ref(), expected);
        });
    }
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
