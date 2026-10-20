//! The pane state surfaces mount on their state and draw in the window; a window with no such state builds none.
use crate::{
    backend::{Backend, Event as BackendEvent},
    pane::states::Kind,
    state::{tests::shell_with, Shell},
};
use gpui::{AppContext, TestAppContext, WindowHandle};

fn open(cx: &mut TestAppContext, backend: Backend) -> WindowHandle<Shell> {
    cx.add_window(|_, cx| shell_with(backend, cx))
}

// The window's painted quad count for one whole frame, after the shell has drawn its state.
fn frame(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> usize {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window.painted_quads().len()
    })
    .expect("the shell window is open")
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

fn built(cx: &mut TestAppContext, handle: WindowHandle<Shell>) -> usize {
    handle
        .update(cx, |shell, _, _| shell.surfaces[0].built())
        .expect("the shell window is open")
}

#[gpui::test]
fn a_window_that_shows_no_state_builds_no_state_surface(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    frame(cx, handle);
    assert_eq!(built(cx, handle), 0, "a listed folder builds no surface");
    set_kind(cx, handle, Kind::Loading);
    frame(cx, handle);
    assert_eq!(built(cx, handle), 0, "a loading pane builds no surface");
}

#[gpui::test]
fn an_empty_pane_mounts_the_hero_and_its_quads_replace_the_list(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    let listed = frame(cx, handle);
    set_kind(cx, handle, Kind::Empty);
    let empty = frame(cx, handle);
    assert_eq!(built(cx, handle), 1, "the empty pane builds the hero once");
    assert_ne!(empty, listed, "the hero draws in the pane's state slot");
    set_kind(cx, handle, Kind::Ready);
    assert_eq!(
        frame(cx, handle),
        listed,
        "a listed pane draws what it drew before the hero"
    );
}

#[gpui::test]
fn a_refused_listing_mounts_its_message_and_clears_on_return(cx: &mut TestAppContext) {
    let (backend, _receiver) = Backend::test_channel();
    let handle = open(cx, backend);
    let listed = frame(cx, handle);
    // A refusal settles only a listing in flight, so the pane begins one as a navigation does.
    handle
        .update(cx, |shell, window, cx| {
            if let Some(host) = shell.panes[0].as_mut() {
                host.pane
                    .begin("/files".into(), crate::pane::swap::Options::default(), 0);
            }
            shell.receive(
                0,
                BackendEvent::Refused {
                    operation: "scan".into(),
                    path: "/files".into(),
                    message: "Permission denied".into(),
                    mode: 0o200,
                },
                window,
                cx,
            );
        })
        .expect("the shell window is open");
    let refused = frame(cx, handle);
    assert_eq!(built(cx, handle), 1, "the refusal builds the message once");
    assert_ne!(
        refused, listed,
        "the locked message draws in the state slot"
    );
    set_kind(cx, handle, Kind::Ready);
    let back = frame(cx, handle);
    assert_ne!(
        back, refused,
        "the message leaves the window once the pane shows a listing"
    );
    let slot = handle
        .update(cx, |shell, _, _| shell.slots.states[0].is_some())
        .expect("the shell window is open");
    assert!(!slot, "the state slot is released when the message leaves");
}
