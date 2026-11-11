// tests/js/previewkeys.js and compare.js key checks: what the open preview does with each action.
use super::keys::*;

fn strip() -> Vec<Control> {
    (0..6).map(|i| Control { enabled: i != 0 && i != 2, visible: true }).collect()
}

#[test]
fn the_pdf_strip_focus_walk() {
    let mut controls = strip();
    // PDF focus enters first enabled control
    assert_eq!(pdf_action("focusNext", Some(&controls), -1), PdfCommand::Focus(1));
    assert_eq!(pdf_action("preview", Some(&controls), 1), PdfCommand::Activate(1));
    // reverse focus wraps to Close
    assert_eq!(pdf_action("focusPrevious", Some(&controls), 1), PdfCommand::Focus(5));
    assert_eq!(pdf_action("open", Some(&controls), 5), PdfCommand::Activate(5));
    // a control disabled after focus never activates
    controls[5].enabled = false;
    assert_eq!(pdf_action("preview", Some(&controls), 5), PdfCommand::Nothing);
    // forward wrap skips disabled Previous
    controls[5].visible = false;
    assert_eq!(pdf_action("focusNext", Some(&controls), 5), PdfCommand::Focus(1));
    assert_eq!(pdf_action("trash", Some(&controls), 1), PdfCommand::Nothing);
    // with no control picked, Return activates nothing, l turns the page, + zooms, Shift+Tab enters at Close
    let fresh = strip();
    assert_eq!(pdf_action("open", Some(&fresh), -1), PdfCommand::Nothing);
    assert_eq!(pdf_action("pageForward", Some(&fresh), -1), PdfCommand::TurnPage(1));
    assert_eq!(pdf_action("zoomIn", Some(&fresh), -1), PdfCommand::Zoom(1));
    assert_eq!(pdf_action("focusPrevious", Some(&fresh), -1), PdfCommand::Focus(5));
}

#[test]
fn the_column_pdf_opens_turns_and_scrolls() {
    let seen: Vec<PdfCommand> = ["preview", "open", "expand", "seekForward", "seekBack", "pageForward", "parent", "cursorDown", "cursorUp", "zoomIn", "focusNext"]
        .iter().map(|a| pdf_action(a, None, -1)).collect();
    use PdfCommand::*;
    assert_eq!(seen, [ToggleExpand, ToggleExpand, ToggleExpand, TurnPage(1), TurnPage(-1), TurnPage(1), TurnPage(-1), Scroll(1), Scroll(-1), Nothing, Nothing]);
}

#[test]
fn space_and_escape_close_every_kind() {
    for s in [Surface::default(), Surface { is_media: true, ..Surface::default() }, Surface { is_pdf: true, ..Surface::default() }] {
        assert_eq!(act("preview", s), Command::Close);
        assert_eq!(act("escape", s), Command::Close);
    }
    // p plays and pauses media and does nothing to a still preview
    assert_eq!(act("playPause", Surface { is_media: true, ..Surface::default() }), Command::TogglePlay);
    assert_eq!(act("playPause", Surface::default()), Command::Nothing);
}

#[test]
fn documents_compare_and_the_bar() {
    let doc = Surface { is_document: true, ..Surface::default() };
    assert_eq!(act("markdownView", doc), Command::ToggleMarkdownView);
    assert_eq!(act("markdownView", Surface::default()), Command::Nothing);
    let compare = Surface { is_compare: true, ..Surface::default() };
    // the action reaches the overlay once, and copies nothing outside Compare
    assert_eq!(act("copyPatch", compare), Command::CopyPatch);
    assert_eq!(act("copyPatch", Surface::default()), Command::Nothing);
    // the cursor keys leave a Compare overlay as it is
    assert_eq!(act("cursorDown", compare), Command::Nothing);
    assert_eq!(act("cursorDown", Surface::default()), Command::Follow(1));
    assert_eq!(act("cursorUp", Surface { html_rendered: true, ..Surface::default() }), Command::ScrollDocument(-1));
    assert_eq!(act("focusPrevious", compare), Command::CompareFocus(true));
    let bar = Surface { has_bar: true, ..Surface::default() };
    assert_eq!(act("focusNext", bar), Command::ToggleCloseFocus);
    assert_eq!(act("open", bar), Command::Nothing);
    assert_eq!(act("open", Surface { close_focused: true, ..bar }), Command::Close);
}

#[test]
fn seek_page_and_zoom() {
    let pdf = Surface { is_pdf: true, ..Surface::default() };
    assert_eq!((act("seekBack", pdf), act("seekForward", pdf)), (Command::TurnPage(-1), Command::TurnPage(1)));
    assert_eq!((act("seekBack", Surface::default()), act("seekForward", Surface::default())), (Command::Seek(-SEEK_MS), Command::Seek(SEEK_MS)));
    assert_eq!((act("zoomIn", pdf), act("zoomOut", pdf), act("zoomFit", pdf)), (Command::Zoom(1), Command::Zoom(-1), Command::ZoomFit));
    assert_eq!((zoom_step(1.0, 1), zoom_step(1.0, -1), zoom_step(3.9, 2)), (1.25, 1.0, 4.0));
    assert_eq!(act("trash", Surface::default()), Command::Nothing);
}
