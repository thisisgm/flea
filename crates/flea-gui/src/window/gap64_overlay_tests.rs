//! Programmatic dismissal preserves jobs and closes its captured menu exactly once.
use super::*;

const RUNNING_JOB: &str = r#"{"t":"job","job":{"id":12,"kind":"copy","head":"Copying a.bin","file":"a.bin","done":0,"total":1,"bytes":0,"totalBytes":4,"itemBytes":4,"rate":0,"left":0,"state":"running","reason":"","output":[],"undo":false,"cancelled":false},"unfinished":1}"#;

#[gpui::test]
fn sol_gap64_programmatic_close_keeps_jobs_and_closes_the_menu_once(cx: &mut TestAppContext) {
    let (backend, mut requests) = Backend::test_lines();
    let handle = open(cx, backend);
    frame(cx, handle);
    shell(cx, handle, |shell, window, cx| {
        shell.copy_to_operation(0, 71, false, "/files", window, cx);
        shell.operation_line(0, RUNNING_JOB, window, cx);
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(crate::pane::swap::HOLD_MS));
    cx.run_until_parked();
    frame(cx, handle);
    assert!(marks(cx, handle, "op1-scrim").is_some());
    while requests().is_some() {}
    shell(cx, handle, |shell, window, cx| {
        shell.close_overlay(window, cx)
    });
    cx.run_until_parked();
    frame(cx, handle);
    assert_eq!(marks(cx, handle, "op1-scrim"), None);
    shell(cx, handle, |shell, window, cx| {
        shell.operation_line(0, RUNNING_JOB, window, cx);
        assert!(!shell.overlay.as_ref().unwrap().read(cx).model.open());
    });
    frame(cx, handle);
    shell(cx, handle, |shell, window, cx| {
        let view = shell.overlay.as_ref().unwrap().read(cx);
        assert!(
            view.model.jobs.visible(),
            "the running Jobs card stays visible"
        );
        assert!(!view.model.open());
        shell.close_overlay(window, cx);
        shell.close_overlay(window, cx);
    });
    cx.run_until_parked();
    frame(cx, handle);
    let delivered: Vec<_> = std::iter::from_fn(&mut requests).collect();
    assert_eq!(delivered, [r#"{"c":"menuaction","op":"close","id":71}"#]);
    shell(cx, handle, |shell, _, cx| {
        assert!(shell.overlay.is_none());
        let view = shell.operation_view(0).unwrap();
        let job = &view.read(cx).model.jobs.rows[0];
        assert_eq!(
            (job.id, job.state.as_str(), job.cancelled),
            (12, "running", false)
        );
        assert!(!view.read(cx).model.open());
    });
}
