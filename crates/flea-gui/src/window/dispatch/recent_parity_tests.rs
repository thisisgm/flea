use super::{
    railrecent_tests::{frame, read},
    test_shell::{land, open_root, open_root_at, row, sent},
};
use crate::state::Shell;
use gpui::{Entity, TestAppContext, VisualTestContext};

fn recent(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    land(shell, cx, "/files", vec![row("note.txt", false)], 1);
    shell.update(cx, |shell, _| {
        let host = shell.panes[0].as_mut().unwrap();
        host.recent.on = true;
        host.recent.from = "/files".into();
    });
    sent();
}

#[gpui::test]
fn pb_recent_hidden_toggle_refuses_without_changing_preferences(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    recent(&shell, cx);
    cx.simulate_keystrokes("ctrl-shift-.");
    frame(cx);
    assert_eq!(
        read(&shell, cx, "statusPrimary"),
        "Recent shows every item it lists, hidden ones included"
    );
    shell.read_with(cx, |shell, _| {
        assert!(!shell.panes[0].as_ref().unwrap().nav.folders.borrow().hidden)
    });
    assert!(sent().is_empty(), "the refusal sends no request");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn pb_recent_new_folder_key_refuses_before_mkdir(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    recent(&shell, cx);
    cx.simulate_keystrokes("ctrl-shift-n");
    frame(cx);
    assert_eq!(
        read(&shell, cx, "statusPrimary"),
        "Recent cannot hold a new folder. Open a folder first"
    );
    assert!(!sent().iter().any(|line| line.contains("mkdir")));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn ey392_recent_draws_back_and_up_live_because_both_close_it(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_at(cx, "/");
    land(&shell, cx, "/", vec![row("note.txt", false)], 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.nav.history.back.is_empty());
            assert!(!host.nav.can_go_up(&host.pane));
            shell.recent_open(0, Vec::new(), window, cx);
        })
    });
    frame(cx);
    let (back, up) = shell.read_with(cx, |shell, cx| {
        let inputs = &shell.chrome.view.as_ref().unwrap().read(cx).inputs;
        (inputs.back, inputs.up)
    });
    assert!(back, "v0.3.12 draws Back at full ink in Recent");
    assert!(up, "v0.3.12 draws Up at full ink in Recent");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

fn arabic(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root.join("src/keys/fixtures/keycaps");
    let catalog = flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: fixture.clone(),
        language_file: fixture.join("language-ar"),
        catalogs: vec![root.join("i18n")],
    });
    assert_eq!(catalog.language(), "ar");
    shell.update(cx, |shell, cx| {
        shell.apply_locale(
            crate::window::locale::Snapshot {
                revision: 2,
                catalog: std::sync::Arc::new(catalog),
            },
            cx,
        );
    });
    cx.run_until_parked();
}

#[gpui::test]
fn i50_public_a68_recent_hidden_refusal_paints_arabic(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    recent(&shell, cx);
    arabic(&shell, cx);
    cx.simulate_keystrokes("ctrl-shift-.");
    frame(cx);
    assert_eq!(
        read(&shell, cx, "statusPrimary"),
        "تعرض العناصر الأخيرة كل ما في قائمتها، بما في ذلك العناصر المخفية"
    );
    shell.read_with(cx, |shell, _| {
        assert!(!shell.panes[0].as_ref().unwrap().nav.folders.borrow().hidden)
    });
    assert!(sent().is_empty(), "the refusal sends no request");
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn i50_public_a68_recent_new_folder_refusal_paints_arabic(cx: &mut TestAppContext) {
    let (shell, cx) = open_root(cx, "default");
    recent(&shell, cx);
    arabic(&shell, cx);
    cx.simulate_keystrokes("ctrl-shift-n");
    frame(cx);
    assert_eq!(
        read(&shell, cx, "statusPrimary"),
        "لا يمكن إنشاء مجلد في العناصر الأخيرة. افتح مجلدًا أولًا"
    );
    assert!(!sent().iter().any(|line| line.contains("mkdir")));
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
