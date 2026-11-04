use super::*;
use crate::backend::testdir::TestDir;
use crate::tui::{graphics::Protocol, media::Player, pdf::Pdf};

#[test]
fn review_t2_tab_preserves_shipped_context_bindings() {
    let map = Map::load();
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        for context in ["listing", "preview", "pdf", "media"] {
            assert_eq!(map.in_context(&Key::named("Tab", ""), preset, context), "focusNext", "{preset} {context}");
            assert_eq!(map.in_context(&Key::named("Tab", "ctrl"), preset, context), "focusPreview");
        }
    }
}

#[test]
fn review_t2_pdf_tab_keeps_quick_look_controls_and_wraps() {
    let root = TestDir::new("t2-pdf-tab");
    let path = root.file("document.pdf", "fixture");
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        for quicklook in [false, true] {
            let (mut wire, reader) = wire::echo_wire();
            let mut m = listing(&mut wire, preset);
            m.preview_focus = true;
            m.quicklook = quicklook;
            m.pdf = Some(Pdf::new(path.clone(), Protocol::None, 80, 20, (0, 0), "#101315", false));
            let controls = if quicklook { 6 } else { 5 };
            for index in 1..=controls {
                press(&mut m, &mut wire, Key::named("Tab", ""));
                assert_eq!((m.preview_focus, m.quicklook, m.pdf.as_ref().unwrap().control), (true, quicklook, index % controls), "{preset}");
            }
            press(&mut m, &mut wire, Key::named("Tab", "shift"));
            assert_eq!(m.pdf.as_ref().unwrap().control, controls - 1);
            wire::finish(wire, reader);
        }
    }
}

#[test]
fn review_t2_media_tab_keeps_control_traversal() {
    for preset in ["default", "vim", "mac", "windows", "yazi"] {
        let (mut wire, reader) = wire::echo_wire();
        let mut m = listing(&mut wire, preset);
        m.preview_focus = true;
        m.quicklook = true;
        m.player = Some(Player::resting(std::path::Path::new("/listing/clip.mp4"), Protocol::None, false, (0, 0, 80, 20), (80, 24), (0, 0)));
        for index in 1..=4 {
            press(&mut m, &mut wire, Key::named("Tab", ""));
            assert_eq!((m.preview_focus, m.quicklook, m.player.as_ref().unwrap().control), (true, true, index % 2), "{preset}");
        }
        press(&mut m, &mut wire, Key::named("Tab", "shift"));
        assert_eq!(m.player.as_ref().unwrap().control, 1);
        wire::finish(wire, reader);
    }
}

fn cancelled_clipboard(request: char, failure: bool) {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "yazi");
    m.cursor = 1;
    press(&mut m, &mut wire, Key::character(request, ""));
    press(&mut m, &mut wire, Key::character('Y', ""));
    assert!(m.clipboard.is_empty());
    assert!(!m.cut);
    assert_eq!(m.message, "Clipboard cleared");
    press(&mut m, &mut wire, Key::character('y', ""));
    assert_eq!(requests(&drain(&mut wire), "paths").len(), 1, "untagged replies must drain before another request");
    let reply = if failure { r#"{"t":"error","where":"paths","message":"unavailable"}"# }
        else { r#"{"t":"paths","paths":["/listing/file"]}"# };
    m.receive(jsondoc::parse(reply).unwrap(), &mut wire).unwrap();
    assert!(m.clipboard.is_empty(), "a delayed {request} reply cannot undo clear");
    assert!(!m.pending_clipboard);
    assert!(!m.cut);
    assert_eq!(m.message, "Clipboard cleared");
    press(&mut m, &mut wire, Key::character('p', ""));
    assert!(requests(&drain(&mut wire), "transfer").is_empty());
    m.cursor = 0;
    press(&mut m, &mut wire, Key::character('y', ""));
    assert_eq!(requests(&drain(&mut wire), "paths").len(), 1);
    m.receive(jsondoc::parse(r#"{"t":"paths","paths":["/listing/folder"]}"#).unwrap(), &mut wire).unwrap();
    assert_eq!(m.clipboard, ["/listing/folder"]);
    assert_eq!(m.message, "Copied 1 item");
    wire::finish(wire, reader);
}

#[test]
fn review_t2_clear_clipboard_rejects_pending_copy_reply() { cancelled_clipboard('y', false); }

#[test]
fn review_t2_clear_clipboard_rejects_pending_cut_reply() { cancelled_clipboard('x', false); }

#[test]
fn review_t2_cancelled_clipboard_error_releases_request_barrier() { cancelled_clipboard('y', true); }

// A cut still resolving when the folder changes ends there: its reply cannot land, the older clipboard and cut flag go, and the footer says so.
#[test]
fn review_m3_navigation_ends_a_pending_cut_and_says_so() {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = listing(&mut wire, "yazi");
    m.clipboard = vec!["/listing/older".into()];
    m.cursor = 1;
    press(&mut m, &mut wire, Key::character('x', ""));
    assert!(m.pending_clipboard && m.cut);
    m.open(std::path::PathBuf::from("/elsewhere"), &mut wire).unwrap();
    assert!(m.clipboard.is_empty(), "the older clipboard would paste in place of the cut that was asked for");
    assert!(!m.cut);
    assert_eq!(m.error, m.i18n.tr("clipboard.stale"));
    m.receive(jsondoc::parse(r#"{"t":"paths","paths":["/listing/file"]}"#).unwrap(), &mut wire).unwrap();
    assert!(m.clipboard.is_empty(), "a reply for the folder that was left cannot land");
    assert!(!m.pending_clipboard);
    wire::finish(wire, reader);
}

fn delayed_sort(key: Key, start: usize, expected: usize, select: bool) {
    let (mut wire, reader) = wire::echo_wire();
    let mut m = Model::new(PathBuf::from("/listing"), &Json::Null);
    m.preset = "yazi".into();
    m.height = 10;
    m.total = 20;
    m.cursor = start;
    let rows = (0..20).map(|i| format!(r#"{{"n":"file-{i:02}"}}"#)).collect::<Vec<_>>().join(",");
    let rows = jsondoc::parse(&format!(r#"{{"t":"rows","start":0,"rows":[{rows}]}}"#)).unwrap();
    m.receive(rows.clone(), &mut wire).unwrap();
    press(&mut m, &mut wire, Key::character(',', ""));
    press(&mut m, &mut wire, Key::character('s', ""));
    assert!(m.sort_anchor.is_some());
    // A held window can arrive while the new sort's listed reply is still pending.
    m.receive(rows, &mut wire).unwrap();
    press(&mut m, &mut wire, key);
    assert_eq!(m.cursor, expected);
    assert_eq!(m.selected.contains(&start), select);
    let reply = format!(r#"{{"t":"listed","n":20,"path":"/listing","anchor":"/listing/file-{start:02}","anchorIndex":2}}"#);
    m.receive(jsondoc::parse(&reply).unwrap(), &mut wire).unwrap();
    assert_eq!(m.cursor, expected, "a delayed sort must preserve the newer cursor move");
    assert!(m.sort_anchor.is_none());
    wire::finish(wire, reader);
}

#[test]
fn review_t2_half_page_down_cancels_sort_anchor() { delayed_sort(Key::character('D', "ctrl"), 0, 5, false); }

#[test]
fn review_t2_half_page_up_cancels_sort_anchor() { delayed_sort(Key::character('U', "ctrl"), 15, 10, false); }

#[test]
fn review_t2_select_and_advance_cancels_sort_anchor() { delayed_sort(Key::named("Space", ""), 0, 1, true); }
