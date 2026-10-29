//! Rename watch debt is measured by request counts on the production keyboard path.
use super::{
    rename_tests::{close, draw, ready, receive, sent},
    test_shell::{land, lists, row},
};
use crate::backend::Event as BackendEvent;
use gpui::TestAppContext;

#[gpui::test]
fn bfx265_rename_spends_its_watch_debt_before_repeated_navigation(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    let initial = lists(&shell, cx);
    for _ in 0..24 {
        cx.simulate_keystrokes("up");
        assert_eq!(
            shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
            1
        );
        cx.simulate_keystrokes("down");
        assert_eq!(
            shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
            2
        );
    }
    assert_eq!(
        lists(&shell, cx),
        initial,
        "navigation before rename lists nothing"
    );
    cx.simulate_keystrokes("f2");
    draw(cx);
    cx.simulate_input("draft");
    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&mut lines).len(), 1);
    receive(
        &shell,
        cx,
        BackendEvent::Changed {
            path: "/files".into(),
        },
    );
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        assert!(!host
            .nav
            .fire_watch(&mut host.pane, &host.selected, u64::MAX));
        assert!(host.nav.watch.stale, "the pending rename owns the rows");
    });
    receive(
        &shell,
        cx,
        BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/draft.txt"}"#.into()),
    );
    let rows = vec![
        row("alpha", true),
        row("beta", true),
        row("draft.txt", false),
    ];
    land(&shell, cx, "/files", rows.clone(), 2);
    for _ in 0..24 {
        cx.simulate_keystrokes("up");
        assert_eq!(
            shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
            1
        );
        cx.simulate_keystrokes("down");
        assert_eq!(
            shell.read_with(cx, |shell, _| shell.panes[0].as_ref().unwrap().pane.cursor),
            2
        );
    }
    let (stale, reread) = shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        let stale = host.nav.watch.stale;
        let reread = host
            .nav
            .fire_watch(&mut host.pane, &host.selected, u64::MAX);
        (stale, reread)
    });
    let after = lists(&shell, cx);
    eprintln!("BFX265 counts before={initial} after_rename_and_48_keys={after} watch_rereads={} stale={stale}", usize::from(reread));
    assert!(
        shell.read_with(cx, |shell, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .watch_task
            .is_none()),
        "the consumed deadline has no task left"
    );
    assert_eq!(after, initial + 1, "the rename owns exactly one re-read");
    assert!(
        !stale && !reread,
        "rename's notice must not leave a second re-read behind"
    );
    receive(
        &shell,
        cx,
        BackendEvent::Changed {
            path: "/files".into(),
        },
    );
    let foreign = shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.nav
            .fire_watch(&mut host.pane, &host.selected, u64::MAX)
    });
    assert!(foreign, "a later foreign write still re-reads");
    land(&shell, cx, "/files", rows, 3);
    close(&shell, cx);
}

#[gpui::test]
fn bfx265_later_changes_and_a_refused_refresh_keep_their_watch_debt(cx: &mut TestAppContext) {
    for competing in [false, true] {
        let (shell, cx, mut lines) = ready(cx, "default");
        cx.simulate_keystrokes("f2");
        draw(cx);
        cx.simulate_input("draft");
        cx.simulate_keystrokes("enter");
        assert_eq!(sent(&mut lines).len(), 1);
        if competing {
            cx.update(|window, cx| {
                shell.update(cx, |shell, cx| {
                    assert_eq!(
                        shell.step(0, crate::window::navigate::Step::Refresh(None), window, cx),
                        Ok(true)
                    );
                })
            });
            receive(
                &shell,
                cx,
                BackendEvent::Changed {
                    path: "/files".into(),
                },
            );
        }
        receive(
            &shell,
            cx,
            BackendEvent::Producer(r#"{"t":"renamed","ok":true,"path":"/files/draft.txt"}"#.into()),
        );
        if !competing {
            receive(
                &shell,
                cx,
                BackendEvent::Changed {
                    path: "/files".into(),
                },
            );
        }
        let (lists_before, paid_before) = (
            lists(&shell, cx),
            shell.read_with(cx, |shell, _| {
                shell.panes[0].as_ref().unwrap().nav.watch.rereads
            }),
        );
        // land() advances the clock past the watch deadline, so the armed task may pay the notice itself.
        land(
            &shell,
            cx,
            "/files",
            vec![
                row("alpha", true),
                row("beta", true),
                row("draft.txt", false),
            ],
            2,
        );
        let (paid, owed) = shell.update(cx, |shell, _| {
            let host = shell.panes[0].as_mut().unwrap();
            (host.nav.watch.rereads - paid_before, host.nav.watch.stale)
        });
        eprintln!("BFX265 competing={competing} paid={paid} owed={owed}");
        assert_eq!(
            paid + usize::from(owed),
            1,
            "competing={competing}: the later notice is paid or still owed, exactly once"
        );
        assert_eq!(lists(&shell, cx), lists_before + paid);
        close(&shell, cx);
    }
}
