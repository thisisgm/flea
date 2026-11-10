//! Exercise the assigned readers through the same adapter set a Root window mounts.
use crate::{
    backend::Backend,
    g2::quick_look_overlay::doc::View as Document,
    ipc::server::Event,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::ipc::wire::{Reply, Request};
use gpui::{AppContext, Context, TestAppContext, Window, WindowHandle};

type Handle = WindowHandle<Root>;

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

fn call(cx: &mut TestAppContext, handle: Handle, name: &str) -> Reply {
    edit(cx, handle, |shell, window, cx| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
        shell.ipc(
            Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: name.into(),
                    arguments: Vec::new(),
                },
                answer,
            },
            window,
            cx,
        );
        receive.try_recv().unwrap().reply
    })
}

fn value(cx: &mut TestAppContext, handle: Handle, name: &str) -> String {
    let reply = call(cx, handle, name);
    assert_eq!(reply.code, 0, "{name}: {}", reply.stderr);
    reply.stdout.trim_end_matches('\n').to_owned()
}

pub(super) fn document(
    cx: &mut TestAppContext,
    handle: Handle,
    name: &str,
    icon: &str,
) -> crate::g2::quick_look_overlay::doc::Doc {
    crate::g2::doc_preview::render::without_threads(|| {
        edit(cx, handle, |shell, window, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.cursor = 0;
            host.pane
                .receive(flea::protocol::Reply::Listed {
                    path: "/files".into(),
                    total: 1,
                })
                .unwrap();
            host.pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: vec![flea::protocol::RowFacts {
                        name: name.into(),
                        icon: icon.into(),
                        mode: 0o100644,
                        size: 64,
                        ..Default::default()
                    }],
                })
                .unwrap();
            assert!(shell.open_quick_look(0, window, cx));
            let view = shell.quick_look.as_ref().unwrap().view.read(cx);
            let doc = view.subject().unwrap().body.doc().unwrap();
            doc.clone()
        })
    })
}

fn pdf(cx: &mut TestAppContext, handle: Handle) -> gpui::Entity<crate::g2::doc_preview::PdfView> {
    let doc = document(cx, handle, "book.pdf", "application-pdf");
    let Document::Pdf(pdf) = doc.view else {
        panic!("the preview action must mount the PDF view")
    };
    pdf.update(cx, |pdf, _| pdf.surface.sizes = vec![(612., 792.); 3]);
    pdf
}

#[gpui::test]
fn focus_stop_reads_the_strip_index(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    assert_eq!(value(cx, handle, "focusStop"), "list");
    edit(cx, handle, |shell, window, cx| {
        assert!(shell.tab_action("tabNew", window, cx));
        assert_eq!(shell.strip.tabs[0].count(), 2);
        shell.strip.bar.focused_index = 2;
        window.focus(&shell.dual_focus.strip, cx);
    });
    assert_eq!(value(cx, handle, "focusStop"), "strip:2");
}

#[gpui::test]
fn mode_is_the_main_window_browse_contract(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    assert_eq!(value(cx, handle, "mode"), "browse");
}

#[gpui::test]
fn preview_pdf_page_reads_the_mounted_surface(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    assert_eq!(value(cx, handle, "previewPdfPage"), "-1");
    let pdf = pdf(cx, handle);
    pdf.update(cx, |pdf, _| {
        pdf.surface.sizes = vec![(612., 792.); 4];
        pdf.surface.turn(2, 0);
    });
    assert_eq!(value(cx, handle, "previewPdfPage"), "2");
}

#[gpui::test]
fn preview_pdf_zoom_reads_the_viewer_action(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    assert_eq!(value(cx, handle, "previewPdfZoom"), "");
    let pdf = pdf(cx, handle);
    pdf.update(cx, |pdf, cx| {
        pdf.action(flea::ui_logic_doc_preview::viewer::Action::ZoomIn, cx);
    });
    assert_eq!(value(cx, handle, "previewPdfZoom"), "1.25");
}

#[gpui::test]
fn preview_pdf_focus_reads_the_viewer_action(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    assert_eq!(value(cx, handle, "previewPdfFocus"), "-1");
    let pdf = pdf(cx, handle);
    pdf.update(cx, |pdf, cx| {
        pdf.action(flea::ui_logic_doc_preview::viewer::Action::FocusNext, cx);
    });
    assert_eq!(value(cx, handle, "previewPdfFocus"), "1");
}

#[gpui::test]
fn preview_expanded_reads_the_viewer_action(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    assert_eq!(value(cx, handle, "previewExpanded"), "");
    let pdf = pdf(cx, handle);
    assert_eq!(value(cx, handle, "previewExpanded"), "false");
    pdf.update(cx, |pdf, cx| {
        pdf.action(flea::ui_logic_doc_preview::viewer::Action::Expand, cx);
    });
    assert_eq!(value(cx, handle, "previewExpanded"), "true");
}

#[gpui::test]
fn preview_close_state_is_empty_without_a_document(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    assert_eq!(value(cx, handle, "previewCloseState"), "{}");
}

#[gpui::test]
fn preview_swap_state_has_the_exact_unbuilt_shape(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    assert_eq!(
        value(cx, handle, "previewSwapState"),
        concat!(
            r#"{"column":null,"look":{"holding":false,"capturing":false,"fellBack":false,"holds":0,"fallbacks":0,"bursts":0,"heldFrames":0,"midFrames":0,"loadingFrames":0,"last":{"ms":0,"end":""}},"#,
            r#""lookOpen":{"revealed":false,"revealedEarly":false,"landed":false,"lookEmptyFrames":0,"revealCaps":0},"lookVisible":false}"#,
        ),
    );
    pdf(cx, handle);
    let actual = flea::jsondoc::parse(&value(cx, handle, "previewSwapState")).unwrap();
    let expected = flea::jsondoc::parse(concat!(
        r#"{"column":null,"look":{"holding":true,"capturing":false,"fellBack":false,"holds":1,"fallbacks":0,"bursts":0,"heldFrames":0,"midFrames":0,"loadingFrames":0,"last":{"ms":0,"end":""},"shownMs":0},"#,
        r#""lookOpen":{"revealed":false,"revealedEarly":false,"landed":false,"lookEmptyFrames":0,"revealCaps":0,"revealCapStart":{"seq":2},"shownSeq":1},"lookVisible":true}"#,
    )).unwrap();
    assert_eq!(actual, expected);
}

#[gpui::test]
fn preview_selection_state_reads_the_live_unbuilt_pane(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    let expected = edit(cx, handle, |shell, window, _| {
        shell.view = crate::window::startup::View::List;
        let width = shell.geometry(window).panes[0].width;
        format!(
            r#"{{"view":"list","width":{width},"available":{width},"inlineVisible":false,"index":-1,"path":"","focused":false,"pending":0}}"#,
        )
    });
    assert_eq!(value(cx, handle, "previewSelectionState"), expected);
}

#[gpui::test]
fn key_delivery_state_matches_the_complete_oracle_shape(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    edit(cx, handle, |shell, window, cx| {
        assert!(shell.search_action("search", window, cx));
        assert!(shell.search_key(
            &gpui::KeyDownEvent {
                keystroke: gpui::Keystroke {
                    key: "x".into(),
                    key_char: Some("x".into()),
                    ..Default::default()
                },
                is_held: false,
                prefer_character_input: false,
            },
            window,
            cx
        ));
    });
    assert_eq!(
        value(cx, handle, "keyDeliveryState"),
        r#"{"keySequence":"","keySequenceIdentity":"","trashArmedAt":0,"clipboard":{"paths":[],"cut":false},"searchMode":"typing","filterTyping":false,"filterQuery":"","searchRunning":true,"searchQuery":"x","searchScanned":0,"searchCancelled":false,"paneFocus":true,"listFocus":false,"activeFocusItem":"QQuickTextInput","preview":{"index":-1,"focused":false},"history":{"back":[],"forward":[]}}"#
    );
}

#[gpui::test]
fn preview_selection_metadata_token_clears_only_on_its_reply(cx: &mut TestAppContext) {
    use crate::window::selection_preview::{Port, SelectionPreview, Subject};
    let (handle, _read) = open(cx);
    let token = edit(cx, handle, |shell, window, cx| {
        let host = shell.panes[0].as_mut().unwrap();
        host.pane.cursor = 0;
        host.pane
            .receive(flea::protocol::Reply::Listed {
                path: "/files".into(),
                total: 1,
            })
            .unwrap();
        host.pane
            .receive(flea::protocol::Reply::Rows {
                start: 0,
                listing: 3,
                rows: vec![flea::protocol::RowFacts {
                    name: "a\"b.bin".into(),
                    mode: 0o100644,
                    ..Default::default()
                }],
            })
            .unwrap();
        let subject = Subject::for_pane(host).unwrap();
        let preview = cx.new(|cx| {
            SelectionPreview::new(
                Port::default(),
                shell.locale.catalog.clone(),
                crate::list::Typeface::omarchy_default(),
                cx,
            )
        });
        preview.update(cx, |view, cx| {
            view.follow(Some(subject), false, false, window, cx);
        });
        let token = preview.read(cx).pending.as_ref().unwrap().generation;
        shell.panes[0].as_mut().unwrap().preview = Some(preview);
        token
    });
    let state = flea::jsondoc::parse(&value(cx, handle, "previewSelectionState")).unwrap();
    // No Columns preview is drawn here, so the reader hides the subject's path and keeps its pending token.
    assert_eq!(
        state.get("inlineVisible"),
        Some(&flea::jsondoc::Json::Bool(false))
    );
    assert_eq!(
        state.get("path").and_then(flea::jsondoc::Json::as_str),
        Some("")
    );
    assert_eq!(
        state.get("pending").and_then(flea::jsondoc::Json::as_f64),
        Some(token as f64)
    );
    for reply_token in [token + 1, token] {
        edit(cx, handle, |shell, window, cx| {
            let line = format!(r#"{{"t":"meta","token":{reply_token}}}"#);
            let document = flea::jsondoc::parse(&line).unwrap();
            let preview = shell.panes[0].as_ref().unwrap().preview.clone().unwrap();
            preview.update(cx, |view, cx| {
                view.on_line(&document, &line, window, cx);
            });
        });
        let state = flea::jsondoc::parse(&value(cx, handle, "previewSelectionState")).unwrap();
        let expected = if reply_token == token { 0 } else { token };
        assert_eq!(
            state.get("pending").and_then(flea::jsondoc::Json::as_f64),
            Some(expected as f64)
        );
    }
}

#[gpui::test]
fn recent_from_is_empty_while_recent_is_off(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    assert_eq!(value(cx, handle, "recentFrom"), "");
}

#[gpui::test]
fn recent_mode_is_empty_while_recent_is_off(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    assert_eq!(value(cx, handle, "recentMode"), "");
}

#[gpui::test]
fn root_mounts_every_assigned_reader_without_not_ported(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    for name in [
        "focusStop",
        "keyDeliveryState",
        "previewCloseState",
        "previewExpanded",
        "previewPdfFocus",
        "previewPdfPage",
        "previewPdfZoom",
        "previewSelectionState",
        "previewSwapState",
        "recentFrom",
        "recentMode",
        "mode",
    ] {
        let reply = call(cx, handle, name);
        assert!(!reply.stderr.contains("NOT_PORTED"), "{name}: {reply:?}");
    }
}

#[path = "reader_port_b_observation_tests.rs"]
mod observations;

#[path = "reader_port_b_lifecycle_tests.rs"]
mod lifecycle;
