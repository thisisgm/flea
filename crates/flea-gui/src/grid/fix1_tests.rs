//! Readers of an active Grid that has no painted frame: unpainted, and under an error state.
use super::join_tests::{call, frame, named_shell};
use crate::{
    backend::{Backend, Event, Failure},
    window::startup::View,
};
use gpui::TestAppContext;

// A Grid that is active before its host exists must not fall through to the parked List readers.
#[test]
fn u10_fix1_an_active_grid_before_its_first_frame_answers_empty_not_the_list() {
    use crate::ipc::{grid_rows::MountedGrid, Adapter};
    let mounted = MountedGrid {
        grid: None,
        active: true,
    };
    let ask = |name: &str| {
        mounted
            .call(name, &["0".into()])
            .unwrap_or_else(|| panic!("{name} fell through to the List"))
            .stdout
    };
    for name in [
        "rowRect",
        "rowCentre",
        "rowNameRect",
        "rowNameCentre",
        "rowNameText",
        "rowThumbRect",
        "rowNameColor",
        "visibleRowName",
    ] {
        assert_eq!(ask(name).trim(), "", "{name} before the first Grid frame");
    }
    assert_eq!(ask("gridColumns").trim(), "1");
    assert_eq!(ask("rowHovered").trim(), "false");
}

#[gpui::test]
fn u10_fix1_the_list_area_survives_an_error_state_as_in_the_list(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    let area = call(cx, handle, "listAreaRect", &[]).stdout;
    let top = call(cx, handle, "headerTop", &[]).stdout;
    assert_ne!(area.trim(), "0 0 0 0");
    handle
        .update(cx, |shell, window, cx| {
            shell.receive(
                0,
                Event::Failed(Failure {
                    key: "window.backend_stopped",
                    detail: String::new(),
                }),
                window,
                cx,
            )
        })
        .unwrap();
    frame(cx, handle);
    assert_eq!(call(cx, handle, "listAreaRect", &[]).stdout, area);
    assert_eq!(call(cx, handle, "headerTop", &[]).stdout, top);
}
