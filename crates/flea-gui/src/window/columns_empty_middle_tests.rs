//! EY-587: the empty active folder must not disable a painted parent's middle-click target.
use super::host;
use crate::{
    columns::geometry::Slot,
    state::Shell,
    window::{
        dispatch::test_shell::{land, open_root},
        navigate::Step,
        startup::View,
    },
};
use gpui::{
    point, px, Context, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, Window,
};

fn act(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    action: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>),
) {
    cx.update(|window, cx| shell.update(cx, |shell, cx| action(shell, window, cx)));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn sol_ey587_empty_active_parent_middle_uses_the_painted_folder(cx: &mut TestAppContext) {
    // Each case enters the empty folder through Return, then uses the production root capture listener.
    for (index, name, expected) in [(1, "full", 2), (2, "alpha.txt", 1)] {
        let (shell, cx) = open_root(cx, "default");
        act(&shell, cx, |shell, window, cx| {
            shell.launch.settings.columns = 3;
            shell.set_view(View::Columns, window, cx);
            assert_eq!(
                shell.step(0, Step::Open("/fixture/A".into()), window, cx),
                Ok(true)
            );
        });
        land(
            &shell,
            cx,
            "/fixture/A",
            vec![
                host::row("empty", true),
                host::row("full", true),
                host::row("alpha.txt", false),
                host::row("beta.md", false),
                host::row("gamma.txt", false),
            ],
            10,
        );
        act(&shell, cx, |shell, window, cx| {
            let pane = shell.panes[0].as_mut().unwrap();
            pane.pane.cursor = 0;
            window.focus(&shell.focus, cx);
            shell.keyboard.context = flea::keys::Context::Listing;
        });
        cx.simulate_keystrokes("enter");
        act(&shell, cx, |shell, _, _| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().pane.place(),
                "/fixture/A/empty"
            );
        });
        land(&shell, cx, "/fixture/A/empty", Vec::new(), 11);
        act(&shell, cx, |shell, window, cx| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().pane.state.kind,
                crate::pane::Kind::Empty
            );
            host::peek(
                shell,
                "/fixture/A",
                &[
                    ("empty", true),
                    ("full", true),
                    ("alpha.txt", false),
                    ("beta.md", false),
                    ("gamma.txt", false),
                ],
                false,
                window,
                cx,
            );
            host::settle(shell, window, cx);
        });
        let at = cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                let painter = columns.painters.get(Slot::Parent).unwrap().borrow();
                let drawn = painter.drawn.as_ref().expect("the parent is painted");
                match &drawn.view().rows {
                    crate::columns::TrackRows::Peek(rows) => assert_eq!(rows[index].name, name),
                    _ => panic!("the target must be a real parent peek row"),
                }
                drop(painter);
                let reply = host::call(
                    shell,
                    "columnParentRowCentre",
                    &[&index.to_string()],
                    window,
                    cx,
                );
                let xy: Vec<f32> = reply
                    .stdout
                    .split_whitespace()
                    .map(|n| n.parse().unwrap())
                    .collect();
                assert_eq!(xy.len(), 2, "the actual reader must resolve the target");
                point(px(xy[0]), px(xy[1]))
            })
        });
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.simulate_mouse_down(at, MouseButton::Middle, Modifiers::none());
        cx.simulate_mouse_up(at, MouseButton::Middle, Modifiers::none());
        let target = if expected == 2 {
            "/fixture/A/full"
        } else {
            "/fixture/A/empty"
        };
        act(&shell, cx, |shell, _, _| {
            let tabs = &shell.strip.tabs[0];
            assert_eq!(
                tabs.count(),
                expected,
                "middle on {name} with an empty active folder"
            );
            assert_eq!(tabs.current(), expected - 1);
            let paths: Vec<_> = tabs
                .tabs
                .iter()
                .flat_map(|tabs| &tabs.items)
                .map(|tab| tab.path.as_str())
                .collect();
            if expected == 2 {
                assert_eq!(paths, ["/fixture/A/empty", "/fixture/A/full"]);
            }
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.place(), target);
        });
        if expected == 2 {
            land(&shell, cx, target, vec![host::row("notes.txt", false)], 12);
        }
        act(&shell, cx, |shell, _, _| {
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.path(), target);
        });
    }
}
