use super::contract::Spec;
use crate::{
    backend::Backend,
    state::tests::shell_with,
    window::{pane::PaneHost, startup::View},
};
use flea::protocol::Reply;
use gpui::TestAppContext;

#[gpui::test]
fn visual_reapplying_the_mounted_view_never_rewrites_the_dual_session(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (entity, window) = cx.add_window_view(|_, cx| {
        let mut shell = shell_with(backend, cx);
        let mut right = PaneHost::new("/files".into(), None, 620., &shell.locale);
        right
            .pane
            .receive(Reply::Listed {
                path: "/files".into(),
                total: 0,
            })
            .unwrap();
        right
            .pane
            .receive(Reply::Rows {
                start: 0,
                rows: vec![],
                listing: 2,
            })
            .unwrap();
        right.answered = true;
        shell.panes[1] = Some(right);
        shell.view = View::Dual;
        shell
    });
    window.update(|window, cx| {
        entity.update(cx, |shell, cx| {
            let writer = shell.panes[0].as_ref().unwrap().nav.writer.clone();
            let before = writer.borrow().sent;
            let spec = Spec::parse(
                &std::process::id().to_string(),
                "g1-01-dual-r0",
                r#"{"path":"/files"}"#,
            )
            .unwrap();
            let (answer, _receive) = std::sync::mpsc::sync_channel(1);
            shell.apply_scene(spec, answer, window, cx);
            assert!(
                shell.visual.pending.is_some(),
                "the scene was accepted and awaits its frame"
            );
            assert_eq!(
                writer.borrow().sent,
                before,
                "capture must not persist an already mounted view"
            );
            assert_eq!(shell.view, View::Dual);
            assert!(!shell.dual_pending);
        });
    });
}
