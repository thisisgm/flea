//! A normal Root with recorded backend traffic and quiet external services.
use crate::{backend::Backend, state::Shell, window::root::Root};
use flea::{ipc::wire::Request, jsondoc};
use gpui::{px, size, AnyView, AppContext, Entity, TestAppContext, VisualTestContext};
use std::rc::Rc;

struct Quiet;
impl crate::g2::pl1::process::Runner for Quiet {
    fn start(&self, _: crate::g2::pl1::process::Job) -> crate::g2::pl1::process::Handle {
        crate::g2::pl1::process::Handle::detached().0
    }
}

pub(super) fn open(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(crate::g2::pl1::source::fixture::Services {
            runner: Rc::new(Quiet),
            hostname: "fixture".into(),
        })
    });
    let (backend, _requests) = Backend::test_channel();
    let (root, cx) = cx.add_window_view(|window, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = crate::state::tests::shell_with(backend, cx);
            shell.launch.settings.data =
                jsondoc::parse(r#"{"view":"list","places":{"rail":"shown","showNetwork":true}}"#)
                    .unwrap();
            let mut join = crate::window::rail::mount(
                shell.locale.catalog.clone(),
                &shell.launch.settings.data,
                0,
                cx,
            );
            join.network_runner = Rc::new(Quiet);
            shell.slots.rail = Some(AnyView::from(join.host.clone()));
            shell.rail = Some(join);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    let shell = root.read_with(cx, |root, _| root.shell.clone());
    cx.simulate_resize(size(px(1100.), px(700.)));
    draw(cx);
    (shell, cx)
}

pub(super) fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

pub(super) fn read(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    function: &str,
    args: &[&str],
) -> String {
    let reply = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                super::server::Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: function.into(),
                        arguments: args.iter().map(|s| (*s).into()).collect(),
                    },
                    answer,
                },
                window,
                cx,
            );
            receive.try_recv().unwrap().reply
        })
    });
    assert_eq!(reply.code, 0, "{function}: {reply:?}");
    reply.stdout.trim_end_matches('\n').into()
}
