//! The pane state surfaces mount on their state and draw in the window; a window with no such state builds none.
use crate::{
    backend::{Backend, Event as BackendEvent, Failure},
    g2::fb1::state::Facts,
    pane::states::Kind,
    state::{tests::shell_with, Shell},
    window::{
        composition::Rect,
        locale::Snapshot,
        startup::{Arguments, Launch, Settings},
    },
};
use flea::ui_logic_fb1::errors::sentence;
use gpui::{AppContext, Bounds, Pixels, Quad, TestAppContext, VisualTestContext, WindowHandle};
use std::sync::Arc;

fn open(cx: &mut TestAppContext, backend: Backend) -> WindowHandle<Shell> {
    cx.add_window(|_, cx| shell_with(backend, cx))
}

// The real startup constructor with controlled external services.
fn open_startup(cx: &mut TestAppContext) -> WindowHandle<Shell> {
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        })
    });
    cx.add_window(|window, cx| {
        let (_events_tx, events) = async_channel::bounded(1);
        let locale = Snapshot {
            revision: 1,
            catalog: Arc::new(crate::test_catalogs::english()),
        };
        let settings = Settings::from_json(flea::jsondoc::parse("{}").unwrap());
        let launch = Launch::resolve(
            settings,
            Arguments {
                path: Some("/files".into()),
                select: None,
            },
            "/home/test",
        );
        Shell::new(launch, locale, events, window, cx)
    })
}

// The window's painted quads for one whole frame, after the shell has drawn its state.
fn frame<V: 'static>(cx: &mut TestAppContext, handle: WindowHandle<V>) -> Vec<Quad> {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window.painted_quads()
    })
    .expect("the shell window is open")
}

fn same_frame(before: &[Quad], after: &[Quad]) -> bool {
    format!("{before:?}") == format!("{after:?}")
}

// The laid-out bounds of one tagged part, read from the frame the window drew.
fn marks<V: 'static>(
    cx: &mut TestAppContext,
    handle: WindowHandle<V>,
    selector: &'static str,
) -> Option<Bounds<Pixels>> {
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.debug_bounds(selector)
}

// The left pane's rect, so a state surface is observed inside its own slot.
fn pane_rect(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> Rect {
    handle
        .update(cx, |shell, window, _| shell.geometry(window).panes[0])
        .expect("the shell window is open")
}

fn inside(pane: Rect, part: Bounds<Pixels>) -> bool {
    let x = f32::from(part.origin.x);
    let y = f32::from(part.origin.y);
    x >= pane.x
        && y >= pane.y
        && x + f32::from(part.size.width) <= pane.x + pane.width
        && y + f32::from(part.size.height) <= pane.y + pane.height
}

fn set_kind(cx: &mut TestAppContext, handle: WindowHandle<Shell>, kind: Kind) {
    handle
        .update(cx, |shell, _, cx| {
            if let Some(host) = shell.panes[0].as_mut() {
                host.pane.state.kind = kind;
            }
            cx.notify();
        })
        .expect("the shell window is open");
}

// Every G2 surface the window can hold: both panes' state entities, and the overlay card.
fn built(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> usize {
    handle
        .update(cx, |shell, _, _| {
            shell.surfaces[0].built()
                + shell.surfaces[1].built()
                + usize::from(shell.overlay.is_some())
        })
        .expect("the shell window is open")
}

fn facts(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> Option<Facts> {
    handle
        .update(cx, |shell, _, cx| shell.surfaces[0].facts(cx))
        .expect("the shell window is open")
}

fn catalog_sentence(cx: &mut TestAppContext, handle: WindowHandle<Shell>, key: &str) -> String {
    handle
        .update(cx, |shell, _, _| shell.locale.catalog.tr(key))
        .expect("the shell window is open")
}

#[gpui::test]
fn a_window_that_shows_no_state_builds_no_state_surface(cx: &mut TestAppContext) {
    let handle = open_startup(cx);
    handle
        .update(cx, |shell, _, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.backend.is_some() && !host.stopped);
            assert_eq!(host.pane.state.kind, Kind::Loading);
            let trash = shell.trash.as_ref().unwrap();
            assert!(trash.pane.read(cx).monitor_start.is_none());
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(built(cx, handle), 0, "startup builds no surface");
    set_kind(cx, handle, Kind::Loading);
    frame(cx, handle);
    assert_eq!(built(cx, handle), 0, "a loading pane builds no surface");
}

#[gpui::test]
fn a_refused_reply_draws_the_directory_mode_from_the_backend(cx: &mut TestAppContext) {
    for (mode, permissions) in [(0o40700, "rwx------"), (0o40000, "---------")] {
        let (backend, _receiver) = Backend::test_channel();
        let handle = open(cx, backend);
        frame(cx, handle);
        let line = format!(
            r#"{{"t":"error","where":"scan","path":"/files","msg":"Permission denied","mode":{mode}}}"#
        );
        let reply = flea::protocol::parse_reply(&line).unwrap();
        let expected = handle
            .update(cx, |shell, window, cx| {
                shell.panes[0].as_mut().unwrap().pane.begin(
                    "/files".into(),
                    crate::pane::swap::Options::default(),
                    0,
                );
                shell.receive(0, BackendEvent::Reply(reply), window, cx);
                flea::ui_logic_fb1::errors::locked_line(mode, &shell.locale.catalog)
            })
            .expect("the shell window is open");
        frame(cx, handle);
        let drawn = facts(cx, handle).expect("the refused reply mounts the Locked message");
        assert_eq!(drawn.kind, crate::g2::fb1::state::Kind::Locked);
        assert!(expected.contains(permissions));
        assert_eq!(drawn.detail.as_ref(), expected);
        let detail = marks(cx, handle, "fb1-state-detail")
            .expect("the directory's permission line draws in the Locked pane");
        assert!(inside(pane_rect(cx, handle), detail));
    }
}

#[gpui::test]
fn an_empty_pane_mounts_the_hero_and_its_quads_replace_the_list(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    let listed = frame(cx, handle);
    set_kind(cx, handle, Kind::Empty);
    frame(cx, handle);
    assert_eq!(built(cx, handle), 1, "the empty pane builds the hero once");
    let pane = pane_rect(cx, handle);
    for part in ["fb1-hero-mark", "fb1-hero-caption"] {
        let drawn = marks(cx, handle, part)
            .unwrap_or_else(|| panic!("the empty pane draws {part} in its state slot"));
        assert!(
            inside(pane, drawn),
            "{part} at {drawn:?} leaves the pane {pane:?}"
        );
    }
    set_kind(cx, handle, Kind::Ready);
    let back = frame(cx, handle);
    assert!(
        same_frame(&back, &listed),
        "a listed pane draws what it drew before the hero"
    );
    assert_eq!(
        marks(cx, handle, "fb1-hero-mark"),
        None,
        "the hero leaves with its quads"
    );
}

#[gpui::test]
fn a_refused_listing_mounts_its_message_and_clears_on_return(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    frame(cx, handle);
    refuse(cx, handle, "/files", "Permission denied", 0o200);
    frame(cx, handle);
    assert_eq!(built(cx, handle), 1, "the refusal builds the message once");
    let pane = pane_rect(cx, handle);
    for part in ["fb1-state-mark", "fb1-state-sentence"] {
        let drawn = marks(cx, handle, part)
            .unwrap_or_else(|| panic!("the locked pane draws {part} in its state slot"));
        assert!(
            inside(pane, drawn),
            "{part} at {drawn:?} leaves the pane {pane:?}"
        );
    }
    // The refusal drives navigation (after_failure consumes the relist, transitions re-arm), so return proves the message leaves and the listing returns; only the pure empty path proves byte-identical restoration.
    set_kind(cx, handle, Kind::Ready);
    frame(cx, handle);
    let kind = handle
        .update(cx, |shell, _, _| {
            shell.panes[0].as_ref().map(|host| host.pane.state.kind)
        })
        .expect("the shell window is open");
    assert_eq!(
        kind,
        Some(Kind::Ready),
        "the pane lists again after the refusal"
    );
    assert_eq!(
        marks(cx, handle, "fb1-state-mark"),
        None,
        "the mark leaves with the message"
    );
    assert_eq!(
        marks(cx, handle, "fb1-state-sentence"),
        None,
        "the sentence leaves with the message"
    );
    let slot = handle
        .update(cx, |shell, _, _| shell.slots.states[0].is_some())
        .expect("the shell window is open");
    assert!(!slot, "the state slot is released when the message leaves");
}

// A refusal settles only a listing in flight, so the pane begins one as a navigation does.
fn refuse(
    cx: &mut TestAppContext,
    handle: WindowHandle<Shell>,
    path: &str,
    message: &str,
    mode: u32,
) {
    handle
        .update(cx, |shell, window, cx| {
            if let Some(host) = shell.panes[0].as_mut() {
                host.pane
                    .begin(path.into(), crate::pane::swap::Options::default(), 0);
            }
            shell.receive(
                0,
                BackendEvent::Refused {
                    operation: "scan".into(),
                    path: path.into(),
                    message: message.into(),
                    mode,
                },
                window,
                cx,
            );
        })
        .expect("the shell window is open");
}

#[gpui::test]
fn a_backend_failure_after_a_listing_draws_its_own_sentence(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    refuse(cx, handle, "/files", "Permission denied", 0o200);
    frame(cx, handle);
    let denied = sentence("scan", "Permission denied", "").render(
        &handle
            .update(cx, |shell, _, _| shell.locale.catalog.clone())
            .expect("open"),
    );
    assert_eq!(
        facts(cx, handle)
            .as_ref()
            .map(|facts| facts.sentence.clone()),
        Some(denied.clone().into()),
        "the locked pane draws the refusal's sentence"
    );
    // A listing lands: leaving the failure state clears the refusal.
    set_kind(cx, handle, Kind::Ready);
    frame(cx, handle);
    assert_eq!(
        facts(cx, handle),
        None,
        "no failure outlives the listing that ends it"
    );
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                BackendEvent::Failed(Failure {
                    key: "window.backend_eof",
                    detail: String::new(),
                }),
                window,
                cx,
            );
        })
        .expect("the shell window is open");
    frame(cx, handle);
    let generic = catalog_sentence(cx, handle, "feedback.error.generic");
    let drawn = facts(cx, handle).expect("the failed pane draws a sentence");
    assert_eq!(
        drawn.sentence.as_ref(),
        generic.as_str(),
        "the error pane draws its own generic sentence"
    );
    assert_ne!(
        drawn.sentence.as_ref(),
        denied.as_str(),
        "the first folder's sentence stays buried"
    );
    assert!(
        marks(cx, handle, "fb1-state-sentence").is_some(),
        "the generic sentence draws in the state slot"
    );
}

#[path = "surfaces_backend_fault_tests.rs"]
mod backend_faults;
