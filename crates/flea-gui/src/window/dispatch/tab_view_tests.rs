use super::{close, home, land, read, row, two_tabs};
use crate::{state::Shell, window::startup::View};
use gpui::{Entity, TestAppContext, VisualTestContext};

// Pane.qml adoptView (EY-442): returning to a tab stores its view for the next launch.
#[gpui::test]
fn a_tab_brought_back_makes_its_view_the_stored_view(cx: &mut TestAppContext) {
    let (shell, cx) = two_tabs(cx, "default");
    let stored = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            shell
                .read(cx)
                .launch
                .settings
                .data
                .get("view")
                .and_then(flea::jsondoc::Json::as_str)
                .map(str::to_owned)
        })
    };
    let step = |shell: &Entity<Shell>, cx: &mut VisualTestContext, action: &str| {
        cx.update(|window, cx| {
            shell.update(
                cx,
                |shell, cx| assert!(shell.tab_action(action, window, cx)),
            )
        });
    };
    let choose = |shell: &Entity<Shell>, cx: &mut VisualTestContext, view| {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.choose_settings_view(view, window, cx))
        });
    };
    step(&shell, cx, "tabPrevious");
    land(&shell, cx, "/files", home(), 3);
    choose(&shell, cx, View::Grid);
    step(&shell, cx, "tabNext");
    land(&shell, cx, "/files/alpha", vec![row("inner", true)], 4);
    choose(&shell, cx, View::List);
    assert_eq!(stored(&shell, cx).as_deref(), Some("list"));
    step(&shell, cx, "tabPrevious");
    land(&shell, cx, "/files", home(), 5);
    assert_eq!(read(&shell, cx, "viewMode", &[]), "grid");
    assert_eq!(stored(&shell, cx).as_deref(), Some("grid"));
    close(&shell, cx);
}

#[gpui::test]
fn adopting_the_default_list_view_does_not_reapply_settings(cx: &mut TestAppContext) {
    let (shell, cx) = two_tabs(cx, "vim");
    let revision = shell.update(cx, |shell, _| {
        let flea::jsondoc::Json::Obj(fields) = &mut shell.launch.settings.data else {
            panic!("settings is an object");
        };
        fields.retain(|(name, _)| name != "view");
        shell.settings.revision
    });
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.tab_action("tabPrevious", window, cx));
        })
    });
    land(&shell, cx, "/files", home(), 3);
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.view, View::List);
        assert_eq!(shell.settings.revision, revision);
        assert!(shell.launch.settings.data.get("view").is_none());
    });
    close(&shell, cx);
}
