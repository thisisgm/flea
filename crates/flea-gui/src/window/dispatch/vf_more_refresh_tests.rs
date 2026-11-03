//! A produced path already in old rows still owes a read after its write.
use super::{
    rename_tests::{close, sent},
    test_shell::{land, open_root, row},
};
use crate::{backend::Backend, window::navigate::Step};
use gpui::TestAppContext;

#[gpui::test]
fn vf_more_deferred_refresh_does_not_treat_an_existing_output_as_fresh(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    let rows = vec![row("input.png", false), row("output.jpg", false)];
    land(&shell, cx, "/files", rows.clone(), 1);
    let (backend, lines) = Backend::test_lines();
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    let mut lines: super::rename_tests::Lines = Box::new(lines);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(shell.step(0, Step::Refresh(None), window, cx), Ok(true));
        })
    });
    sent(&mut lines);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.step(
                    0,
                    Step::Refresh(Some("/files/output.jpg".into())),
                    window,
                    cx,
                ),
                Ok(false)
            );
            assert!(!shell.panes[0].as_ref().unwrap().nav.watch.stale);
        })
    });
    land(&shell, cx, "/files", rows, 2);
    let requests = sent(&mut lines);
    assert!(
        requests
            .iter()
            .any(|line| line.contains("\"c\":\"list\"") && line.contains("/files/output.jpg")),
        "old rows containing an overwritten output still owe a selected read: {requests:?}"
    );
    close(&shell, cx);
}
