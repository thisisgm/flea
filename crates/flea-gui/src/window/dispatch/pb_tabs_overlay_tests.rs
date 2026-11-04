//! Tabs.js dropOverlay: an accepted tab change closes the standing filter; a refused or hidden-tab change keeps it.
use super::test_shell::{land, open_root, row};
use crate::{state::Shell, tabs::bar::Hit};
use gpui::{Context, Entity, TestAppContext, VisualTestContext, Window};

// `tabs` tabs on /files, the current one standing a filter on "a".
fn filtered(cx: &mut TestAppContext, tabs: usize) -> (Entity<Shell>, &mut VisualTestContext) {
    let (shell, cx) = open_root(cx, "default");
    land(
        &shell,
        cx,
        "/files",
        vec![row("a.txt", false), row("b.txt", false)],
        1,
    );
    act(&shell, cx, |shell, window, cx| {
        for _ in 1..tabs {
            assert!(shell.tab_action("tabNew", window, cx));
        }
    });
    cx.simulate_keystrokes("/ a");
    let line = r#"{"t":"filtered","id":1,"query":"a","n":2,"rows":[0],"ms":0.01,"listing":1}"#;
    act(&shell, cx, |shell, window, cx| {
        shell.receive(
            0,
            crate::backend::Backend::test_reply_line(line),
            window,
            cx,
        );
    });
    assert_eq!(count(&shell, cx), tabs);
    assert!(up(&shell, cx), "the fixture stands a filter");
    (shell, cx)
}

fn act(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>),
) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| f(shell, window, cx)));
    cx.run_until_parked();
}

fn up(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> bool {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().filter.owner.up()
    })
}

fn count(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    shell.read_with(cx, |shell, _| shell.strip.tabs[0].count())
}

#[gpui::test]
fn pb_tabs_new_tab_closes_the_standing_filter_even_in_the_same_folder(cx: &mut TestAppContext) {
    let (shell, cx) = filtered(cx, 1);
    act(&shell, cx, |shell, window, cx| {
        shell.tab_tap_hit(Hit::New, false, window, cx)
    });
    assert_eq!(count(&shell, cx), 2);
    assert!(!up(&shell, cx), "an accepted new tab closes the filter");
}

#[gpui::test]
fn pb_tabs_refused_add_and_hidden_close_keep_the_filter(cx: &mut TestAppContext) {
    let (shell, cx) = filtered(cx, crate::tabs::MAX);
    act(&shell, cx, |shell, window, cx| {
        shell.tab_tap_hit(Hit::New, false, window, cx)
    });
    assert_eq!(count(&shell, cx), crate::tabs::MAX);
    assert!(up(&shell, cx), "a refused tenth tab costs nothing");
    act(&shell, cx, |shell, window, cx| {
        shell.tab_tap_hit(Hit::Tab(0), true, window, cx)
    });
    assert_eq!(count(&shell, cx), crate::tabs::MAX - 1);
    assert!(up(&shell, cx), "closing a hidden tab keeps the filter");
    act(&shell, cx, |shell, window, cx| {
        shell.tab_tap_hit(Hit::Tab(0), false, window, cx)
    });
    assert!(!up(&shell, cx), "switching tabs closes the filter");
}

#[gpui::test]
fn pb_tabs_an_accepted_tab_change_closes_quick_look_from_any_source(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    land(&shell, cx, "/files", vec![row("a.txt", false)], 1);
    act(&shell, cx, |shell, window, cx| {
        assert!(shell.open_quick_look(0, window, cx));
        assert!(shell.quick_look_open(cx));
        // An IPC or pointer tab action reaches the model without the card's own key routing.
        assert!(shell.tab_action("tabNew", window, cx));
    });
    assert_eq!(count(&shell, cx), 2);
    assert!(!shell.read_with(cx, |shell, cx| shell.quick_look_open(cx)));
}

#[gpui::test]
fn pb_tabs_an_accepted_tab_change_leaves_a_typed_search(cx: &mut TestAppContext) {
    use flea::ui_logic_sr1::{Kind, Scope, Wire};
    // (keys, Everywhere contents, live): erased owes a relist only, Everywhere contents is live only, local x is both.
    for (keys, everywhere, live) in [
        ("x backspace", false, false),
        ("x", true, true),
        ("x", false, true),
    ] {
        let (shell, cx) = open_root(cx, "default");
        land(&shell, cx, "/files", vec![row("a.txt", false)], 1);
        act(&shell, cx, |shell, window, cx| {
            assert!(shell.search_action("search", window, cx));
            if everywhere {
                let session = &mut shell.panes[0].as_mut().unwrap().search.owner.session;
                session.wire = Wire {
                    everywhere: true,
                    contents: true,
                };
                session.scope = Scope::Everywhere;
                session.kind = Kind::Contents;
            }
        });
        cx.simulate_keystrokes(keys);
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let owner = &host.search.owner;
            assert!(owner.active(), "{keys}: the fixture opens a search");
            assert_eq!(owner.session.live(), live, "{keys}: query live");
            assert_eq!(
                owner.session.gen > owner.session.opened_gen,
                !everywhere,
                "{keys}: relist owed"
            );
            assert!(!host.pane.state.search, "{keys}: no results landed");
        });
        act(&shell, cx, |shell, window, cx| {
            assert!(shell.tab_action("tabNew", window, cx))
        });
        assert_eq!(count(&shell, cx), 2);
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            // Search.leave keeps a live query's strip until the next listing lands; an erased one closes at once.
            let owner = &host.search.owner;
            assert_eq!(owner.session.leaving, live, "{keys}: leaving");
            assert_eq!(owner.active(), live, "{keys}: still drawn");
            assert!(!owner.typing(), "{keys}");
        });
    }
}
