//! Cohesion ownership follows the mounted scene, through the production Shell dispatcher.
use super::test_shell::{land, open_root, open_root_with};
use crate::{state::Shell, window::u07_tests::read};
use flea::{
    jsondoc::{self, Json},
    protocol::RowFacts,
};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use std::time::Duration;

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

fn ready(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root(cx, "default");
    cx.update(|window, _| window.activate_window());
    land(
        &shell,
        cx,
        "/files",
        vec![super::menus_tests::entry("notes.txt", 0o100644)],
        1,
    );
    draw(cx);
    (shell, cx)
}

fn receipt(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    jsondoc::parse(&read(shell, cx, "cohesionControls", &[])).unwrap()
}

fn rows(value: &Json) -> &[Json] {
    value.get("controls").and_then(Json::as_array).unwrap()
}

fn has(value: &Json, field: &str, expected: &str) -> bool {
    rows(value)
        .iter()
        .any(|row| row.get(field).and_then(Json::as_str) == Some(expected))
}

fn transfer(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx);
        shell.overlay.as_ref().unwrap().update(cx, |overlay, cx| {
            overlay.model.dialog = None;
            assert!(overlay.line(r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying 0 of 2","file":"notes.txt","done":0,"total":2,"state":"running","output":[]},"unfinished":1}"#, cx));
        });
    }));
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::HOLD_MS));
    draw(cx);
    assert!(has(&receipt(shell, cx), "surface", "TransferCard"));
}

#[gpui::test]
fn cohesion_owner_menu_covers_unfolded_transfer(cx: &mut TestAppContext) {
    let (shell, cx) = ready(cx);
    transfer(&shell, cx);
    cx.simulate_keystrokes("m");
    super::menus_tests::answer(&shell, cx, &[0o100644]);
    draw(cx);
    let menu = jsondoc::parse(&read(&shell, cx, "menuState", &[])).unwrap();
    assert_eq!(menu.get("opened").and_then(Json::as_bool), Some(true));
    let observed = receipt(&shell, cx);
    assert!(has(&observed, "surface", "ContextMenu"), "{observed:?}");
    assert!(has(&observed, "kind", "MenuRow"), "{observed:?}");
    assert!(!has(&observed, "surface", "TransferCard"));
}

#[gpui::test]
fn cohesion_owner_quick_look_covers_unfolded_transfer(cx: &mut TestAppContext) {
    let (shell, cx) = ready(cx);
    transfer(&shell, cx);
    // The unfolded card holds the keyboard, so Quick Look opens through its own entry point.
    cx.update(|window, cx| {
        shell.update(
            cx,
            |shell, cx| assert!(shell.open_quick_look(0, window, cx)),
        )
    });
    draw(cx);
    cx.executor()
        .advance_clock(Duration::from_millis(crate::pane::swap::CAP_MS));
    draw(cx);
    assert!(shell.read_with(cx, |shell, cx| shell.quick_look_open(cx)));
    let observed = receipt(&shell, cx);
    assert!(has(&observed, "surface", "Preview"), "{observed:?}");
    assert!(!has(&observed, "surface", "TransferCard"), "{observed:?}");
}

#[gpui::test]
fn cohesion_owner_network_card_covers_unfolded_transfer(cx: &mut TestAppContext) {
    let (shell, cx) = ready(cx);
    transfer(&shell, cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
    draw(cx);
    let observed = receipt(&shell, cx);
    assert!(has(&observed, "kind", "ProtocolChip"), "{observed:?}");
    assert!(!has(&observed, "surface", "TransferCard"), "{observed:?}");
}

#[gpui::test]
fn cohesion_owner_path_entry_receipt_uses_painted_field(cx: &mut TestAppContext) {
    let (shell, cx) = ready(cx);
    cx.simulate_keystrokes("ctrl-l");
    draw(cx);
    let observed = receipt(&shell, cx);
    assert!(has(&observed, "kind", "PathEntry"), "{observed:?}");
    let focus = observed.get("focus").unwrap();
    assert_eq!(focus.get("kind").and_then(Json::as_str), Some("PathEntry"));
    assert!(focus.get("w").and_then(Json::as_f64).unwrap() > 0.);
}

#[gpui::test]
fn cohesion_owner_rename_receipt_uses_painted_field(cx: &mut TestAppContext) {
    let (shell, cx) = ready(cx);
    cx.simulate_keystrokes("r");
    draw(cx);
    let observed = receipt(&shell, cx);
    assert!(has(&observed, "kind", "RenameField"), "{observed:?}");
    let focus = observed.get("focus").unwrap();
    assert_eq!(
        focus.get("kind").and_then(Json::as_str),
        Some("RenameField")
    );
    assert!(focus.get("enabled").and_then(Json::as_bool).unwrap());
}

#[gpui::test]
fn cohesion_owner_trash_receipt_uses_painted_empty_button(cx: &mut TestAppContext) {
    let (shell, cx) = ready(cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| {
        assert!(shell.trash_action("openTrash", window, cx));
        let view = shell.trash.as_ref().unwrap().pane.read(cx).view();
        assert_eq!(view.pending_op, "list");
        let id = view.request_id;
        shell.receive(0, crate::backend::Event::TrashLine(format!(r#"{{"t":"trashbrowse","id":{id},"op":"list","ok":true,"total":1,"start":0,"rows":[{{"uri":"trash:///notes.txt","original":"/files/notes.txt","size":10,"deleted":"2026-11-10T09:00:00","directory":false,"icon":"text-x-generic","identity":"notes-1","bulkSelected":false,"storageClass":"","thumbnail":null}}]}}"#)), window, cx);
    }));
    super::menus_tests::settle(cx);
    draw(cx);
    let observed = receipt(&shell, cx);
    assert!(has(&observed, "surface", "TrashView"), "{observed:?}");
    assert!(has(&observed, "name", "Empty Trash"), "{observed:?}");
    assert!(rows(&observed)
        .iter()
        .all(|row| row.get("w").and_then(Json::as_f64).unwrap() > 0.));
}

#[gpui::test]
fn cohesion_owner_transfer_receipt_uses_painted_cancel(cx: &mut TestAppContext) {
    let (shell, cx) = ready(cx);
    transfer(&shell, cx);
    let observed = receipt(&shell, cx);
    assert!(has(&observed, "name", "Cancel"), "{observed:?}");
    assert!(rows(&observed)
        .iter()
        .any(
            |row| row.get("name").and_then(Json::as_str) == Some("Cancel")
                && row.get("enabled").and_then(Json::as_bool) == Some(true)
        ));
}

#[gpui::test]
fn cohesion_owner_pdf_lane_joins_main_controls(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    use crate::g2::doc_preview::{Metrics, Palette, PdfView};
    use crate::window::selection_preview::{Frame, Kind, Mounted, Subject};
    let (shell, cx) = open_root_with(
        cx,
        r#""keys":"default","view":"columns","preview":{"column":true,"thumbnails":"off"}"#,
        None,
    );
    cx.update(|window, _| window.activate_window());
    land(
        &shell,
        cx,
        "/files",
        vec![RowFacts {
            name: "manual.pdf".into(),
            icon: "application-pdf".into(),
            kind: "PDF document".into(),
            mode: 0o100644,
            ..Default::default()
        }],
        1,
    );
    draw(cx);
    let pdf = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let preview = shell
                .selection_preview(0)
                .expect("the production column is mounted");
            let subject = Subject::for_pane(shell.panes[0].as_ref().unwrap()).unwrap();
            let pdf = cx.new(|cx| {
                PdfView::new(
                    true,
                    Palette {
                        background: crate::palette::Theme::default().background,
                    },
                    Metrics {
                        gap: 8.,
                        margin: 16.,
                        row_height: 28.,
                    },
                    cx,
                )
            });
            pdf.update(cx, |pdf, cx| {
                pdf.surface.sizes = vec![(612., 792.); 3];
                for page in 0..3 {
                    let image = std::sync::Arc::new(
                        gpui::RenderImage::from_bgra(2, 2, vec![255; 16]).unwrap(),
                    );
                    pdf.surface.cache.insert(page, 612, image, &[0, 1, 2]);
                }
                pdf.set_active(true, window, cx);
            });
            preview.update(cx, |view, cx| {
                view.pending = None;
                view.drawn = Some(Frame {
                    subject,
                    generation: 1,
                    kind: Kind::Pdf,
                    mounted: Mounted::Pdf(pdf.clone()),
                    meta: None,
                    facts: None,
                    thumb_answered: true,
                    thumb_observation: None,
                    source: None,
                    image: None,
                    animation: None,
                    animation_source: None,
                    quality: Default::default(),
                    trace: None,
                    failed: false,
                    manual: false,
                    remote: false,
                    _watch: None,
                });
                view.port.0.borrow_mut().ready = true;
                cx.notify();
            });
            pdf
        })
    });
    draw(cx);
    assert_eq!(
        read(&shell, cx, "columnPdfPages", &[]),
        "3",
        "the actual preview owns the PDF"
    );
    let observed = receipt(&shell, cx);
    assert!(
        has(&observed, "kind", "Crumb"),
        "a mounted PDF cannot replace chrome: {observed:?}"
    );
    let preview = shell
        .read_with(cx, |shell, cx| {
            *shell
                .selection_preview(0)
                .unwrap()
                .read(cx)
                .frame_rect
                .borrow()
        })
        .unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: preview.center(),
        delta: gpui::ScrollDelta::Lines(gpui::point(0., -1.)),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    draw(cx);
    assert!(
        pdf.read_with(cx, |pdf, _| pdf.cohesion_lane().is_some()),
        "the real PDF painter records its lane"
    );
    let observed = receipt(&shell, cx);
    assert!(has(&observed, "kind", "Crumb"), "{observed:?}");
    assert!(has(&observed, "surface", "PreviewColumn"), "{observed:?}");
    assert!(has(&observed, "kind", "ChromeButton"), "{observed:?}");
}
