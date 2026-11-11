//! FIX-HARNESS-READERS: readers called through the Root's adapter set, compared with what the window draws.
use super::reader_port_b_tests::document;
use crate::{
    backend::Backend,
    g2::quick_look_overlay::doc::View as Document,
    ipc::server::Event,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::{
    ipc::wire::{Reply, Request},
    jsondoc::{self, Json},
    ui_logic_print_convert::{output::Kind, selection::Source},
};
use gpui::{AppContext, Context, TestAppContext, VisualTestContext, Window, WindowHandle};

#[path = "cohesion_pdf_tests.rs"]
mod cohesion_pdf;

type Handle = WindowHandle<Root>;

/// The window and the backend's request reader, which the caller keeps so the backend stays open.
fn open(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (backend, read) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    (handle, read)
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
        })
        .unwrap()
}

fn call(cx: &mut TestAppContext, handle: Handle, name: &str, arguments: &[&str]) -> Reply {
    edit(cx, handle, |shell, window, cx| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
        shell.ipc(
            Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: name.into(),
                    arguments: arguments.iter().map(|a| (*a).into()).collect(),
                },
                answer,
            },
            window,
            cx,
        );
        receive.try_recv().unwrap().reply
    })
}

fn value(cx: &mut TestAppContext, handle: Handle, name: &str, arguments: &[&str]) -> String {
    let reply = call(cx, handle, name, arguments);
    assert_eq!(reply.code, 0, "{name}: {}", reply.stderr);
    reply.stdout.trim_end_matches('\n').to_owned()
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn reveal(cx: &mut TestAppContext, handle: Handle) {
    edit(cx, handle, |shell, _, cx| {
        shell
            .quick_look
            .as_ref()
            .unwrap()
            .view
            .update(cx, |view, cx| view.reveal_cap(cx));
    });
}

fn centre(bounds: gpui::Bounds<gpui::Pixels>) -> String {
    let centre = bounds.center();
    format!(
        "{} {}",
        f32::from(centre.x).round(),
        f32::from(centre.y).round()
    )
}

fn text<'a>(state: &'a Json, key: &str) -> Option<&'a str> {
    state.get(key).and_then(Json::as_str)
}

// EY-190: pdfState(true) answers the Quick Look PDF's page, zoom and strip controls where IpcPreviewState reads the overlay PdfViewer.
#[gpui::test]
fn fixreaders_ey190_pdf_state_reads_the_open_quick_look_pdf(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (handle, _requests) = open(cx);
    assert_eq!(value(cx, handle, "pdfState", &["true"]), "null");
    let doc = document(cx, handle, "book.pdf", "application-pdf");
    let Document::Pdf(pdf) = doc.view else {
        panic!("Quick Look must mount the PDF view")
    };
    pdf.update(cx, |pdf, _| pdf.surface.sizes = vec![(612., 792.); 3]);
    reveal(cx, handle);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    let state = jsondoc::parse(&value(&mut visual, handle, "pdfState", &["true"])).unwrap();
    assert_eq!(state.get("pages").and_then(Json::as_f64), Some(3.));
    assert_eq!(state.get("page").and_then(Json::as_f64), Some(0.));
    assert_eq!(state.get("control").and_then(Json::as_f64), Some(-1.));
    // focused follows the PdfViewer's hold on the keyboard: true in the focused window, false once it blurs.
    assert_eq!(state.get("focused").and_then(Json::as_bool), Some(true));
    let delivery = jsondoc::parse(&value(&mut visual, handle, "keyDeliveryState", &[])).unwrap();
    assert_eq!(text(&delivery, "activeFocusItem"), Some("PdfViewer"));
    edit(&mut visual, handle, |_, window, cx| window.blur(cx));
    let blurred = jsondoc::parse(&value(&mut visual, handle, "pdfState", &["true"])).unwrap();
    assert_eq!(blurred.get("focused").and_then(Json::as_bool), Some(false));
    let delivery = jsondoc::parse(&value(&mut visual, handle, "keyDeliveryState", &[])).unwrap();
    assert_eq!(text(&delivery, "activeFocusItem"), Some("null"));
    // Refocused, the window hands the keyboard back to the open PDF.
    edit(&mut visual, handle, |shell, window, cx| {
        window.focus(&shell.focus, cx)
    });
    draw(&mut visual);
    let back = jsondoc::parse(&value(&mut visual, handle, "pdfState", &["true"])).unwrap();
    assert_eq!(back.get("focused").and_then(Json::as_bool), Some(true));
    let delivery = jsondoc::parse(&value(&mut visual, handle, "keyDeliveryState", &[])).unwrap();
    assert_eq!(text(&delivery, "activeFocusItem"), Some("PdfViewer"));
    // Settings then takes the keyboard inside the focused window, so the PDF no longer holds it.
    edit(&mut visual, handle, |shell, window, cx| {
        shell.open_settings(window, cx)
    });
    draw(&mut visual);
    let settings = jsondoc::parse(&value(&mut visual, handle, "keyDeliveryState", &[])).unwrap();
    assert_eq!(text(&settings, "activeFocusItem"), Some("QQuickItem"));
    let elsewhere = jsondoc::parse(&value(&mut visual, handle, "pdfState", &["true"])).unwrap();
    // The Quick Look PDF is still open under Settings, so false comes from the keyboard, not a closed overlay.
    assert_eq!(elsewhere.get("pages").and_then(Json::as_f64), Some(3.));
    assert_eq!(
        elsewhere.get("focused").and_then(Json::as_bool),
        Some(false)
    );
    assert!(state.get("zoom").and_then(Json::as_f64).is_some());
    let controls = state.get("controls").and_then(Json::as_array).unwrap();
    assert_eq!(controls.len(), 6, "the six strip controls in focus order");
    for control in controls {
        assert!(!text(control, "name").unwrap_or_default().is_empty());
    }
    // Next is enabled on the first of three pages and draws as pv1-strip-1; the strip's close is the card's close.
    let next = &controls[1];
    assert_eq!(next.get("enabled").and_then(Json::as_bool), Some(true));
    let drawn = visual.debug_bounds("pv1-strip-1").expect("Next is drawn");
    assert_eq!(text(next, "centre"), Some(centre(drawn).as_str()));
    let close = visual.debug_bounds("pv1-close").expect("close is drawn");
    assert_eq!(text(&controls[5], "centre"), Some(centre(close).as_str()));
}

// EY-190: previewSurfaceRect reads Quick Look's surface item, the open document's body, before the column's.
#[gpui::test]
fn fixreaders_ey190_surface_rect_reads_the_open_quick_look_document(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (handle, _requests) = open(cx);
    let doc = document(cx, handle, "book.pdf", "application-pdf");
    let Document::Pdf(pdf) = doc.view else {
        panic!("Quick Look must mount the PDF view")
    };
    pdf.update(cx, |pdf, _| pdf.surface.sizes = vec![(612., 792.); 3]);
    reveal(cx, handle);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    let body = visual
        .debug_bounds("pv1-doc")
        .expect("the document body draws");
    let expected = format!(
        "{} {} {} {}",
        f32::from(body.left()).round(),
        f32::from(body.top()).round(),
        f32::from(body.size.width).round(),
        f32::from(body.size.height).round()
    );
    assert_eq!(
        value(&mut visual, handle, "previewSurfaceRect", &[]),
        expected
    );
    // A PDF draws no picture, as Preview.qml's picture box exists only for images and media.
    assert_eq!(value(&mut visual, handle, "previewPictureRect", &[]), "");
}

// EY-367: combineState reads the open Combine card, and Tab and Right sent through the window move it.
#[gpui::test]
fn fixreaders_ey367_combine_state_follows_tab_and_right(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    assert_eq!(
        value(cx, handle, "combineState", &[]),
        r#"{"opened":false}"#
    );
    edit(cx, handle, |shell, window, cx| {
        let files = ["a.png", "b.png"]
            .iter()
            .map(|name| Source {
                path: format!("/files/{name}"),
                kind: Kind::Image,
            })
            .collect();
        assert!(shell.open_output(
            0,
            crate::g2::print_convert::model::Action::Combine,
            0,
            files,
            window,
            cx
        ));
    });
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    let state = |cx: &mut VisualTestContext| {
        jsondoc::parse(&value(cx, handle, "combineState", &[])).unwrap()
    };
    let opened = state(&mut visual);
    assert_eq!(opened.get("opened").and_then(Json::as_bool), Some(true));
    assert_eq!(text(&opened, "layout"), Some("row"));
    assert_eq!(text(&opened, "format"), Some("png"));
    let images: Vec<_> = opened
        .get("images")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .filter_map(Json::as_str)
        .collect();
    assert_eq!(images, ["a.png", "b.png"]);
    let mut seen = Vec::new();
    for _ in 0..8 {
        visual.simulate_keystrokes("tab");
        draw(&mut visual);
        let focus = text(&state(&mut visual), "focus").map(str::to_owned);
        seen.push(focus.clone());
        if focus.as_deref() == Some("layout") {
            break;
        }
    }
    assert_eq!(
        seen.last().cloned().flatten().as_deref(),
        Some("layout"),
        "Tab walks the card's fields: {seen:?}"
    );
    visual.simulate_keystrokes("right");
    draw(&mut visual);
    let moved = state(&mut visual);
    assert_eq!(
        text(&moved, "layout"),
        Some("column"),
        "Right steps the layout chips"
    );
    assert_eq!(text(&moved, "focus"), Some("layout"));
}

// CTL2 add-on: the GPUI-only readers the scene units found missing are registered, and clipboardText reads the system clipboard.
#[gpui::test]
fn fixreaders_gpui_only_readers_answer(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    for name in [
        "columnsState",
        "clipboardText",
        "combineState",
        "dirSizeState",
    ] {
        let reply = call(cx, handle, name, &[]);
        assert_ne!(
            reply.stdout.trim(),
            "Function not found.",
            "{name} is registered"
        );
    }
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("/files/notes.txt".into()));
    assert_eq!(value(cx, handle, "clipboardText", &[]), "/files/notes.txt");
}

// SCN-DEV-1 flash.run-bl and flash-cancel-bl: flashControlCentre centres the step action the flash pane paints, by name.
#[gpui::test]
fn fixreaders_flash_control_centre_reads_the_painted_step_action(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    assert_eq!(value(cx, handle, "flashControlCentre", &["flash"]), "");
    edit(cx, handle, |shell, _, _| {
        shell
            .operations
            .archive
            .flash
            .open_with_image("/files/disk.iso");
    });
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    let action = visual
        .debug_bounds(crate::g2::archive_flash::flash::SEL_STEP_FLASH)
        .expect("the Flash action draws");
    assert_eq!(
        value(&mut visual, handle, "flashControlCentre", &["flash"]),
        centre(action)
    );
    assert_eq!(
        value(&mut visual, handle, "flashControlCentre", &["cancel"]),
        ""
    );
    // A closed pane keeps its last painted box, which the reader must not answer.
    edit(&mut visual, handle, |shell, _, _| {
        shell.operations.archive.flash.close();
    });
    assert_eq!(
        value(&mut visual, handle, "flashControlCentre", &["flash"]),
        ""
    );
    edit(&mut visual, handle, |shell, _, _| {
        shell
            .operations
            .archive
            .flash
            .open_with_image("/files/disk.iso");
    });
    draw(&mut visual);
    edit(&mut visual, handle, |shell, _, _| {
        shell
            .operations
            .archive
            .flash
            .start_transfer(&shell.locale.catalog, 7, 1024, true, 0);
    });
    draw(&mut visual);
    let cancel = visual
        .debug_bounds(crate::g2::archive_flash::flash::SEL_STEP_CANCEL)
        .expect("Cancel draws while the flash runs");
    assert_eq!(
        value(&mut visual, handle, "flashControlCentre", &["cancel"]),
        centre(cancel)
    );
    assert_eq!(
        value(&mut visual, handle, "flashControlCentre", &["flash"]),
        ""
    );
    assert_eq!(
        value(&mut visual, handle, "flashControlCentre", &["back"]),
        ""
    );
    edit(&mut visual, handle, |shell, _, _| {
        let done = jsondoc::parse(
            r#"{"t":"flashdone","id":7,"ok":true,"phase":"verify","at":0,"err":"","blank":false}"#,
        )
        .unwrap();
        let done = flea::ui_logic_archive_flash::wire::parse_flashdone(&done);
        shell
            .operations
            .archive
            .flash
            .finish_transfer(&shell.locale.catalog, &done);
    });
    draw(&mut visual);
    let end = visual
        .debug_bounds(crate::g2::archive_flash::flash::SEL_STEP_END)
        .expect("the ending draws its end action");
    assert_eq!(
        value(&mut visual, handle, "flashControlCentre", &["end"]),
        centre(end)
    );
    assert_eq!(
        value(&mut visual, handle, "flashControlCentre", &["cancel"]),
        ""
    );
}

// EY-527: once the flash card has closed, flashState answers the built view's state as FlashHost.describe does, never an error.
#[gpui::test]
fn fixreaders_ey527_flash_state_reads_a_closed_flash(cx: &mut TestAppContext) {
    let (handle, _requests) = open(cx);
    assert_eq!(value(cx, handle, "flashState", &[]), r#"{"opened":false}"#);
    edit(cx, handle, |shell, _, _| {
        let flash = &mut shell.operations.archive.flash;
        flash.open_with_drive("/dev/sdz");
        flash.close();
    });
    let state = jsondoc::parse(&value(cx, handle, "flashState", &[])).unwrap();
    assert_eq!(state.get("opened").and_then(Json::as_bool), Some(false));
    assert_eq!(text(&state, "image"), Some(""));
    assert_eq!(state.get("running").and_then(Json::as_bool), Some(false));
    assert_eq!(text(&state, "phase"), Some(""));
}
