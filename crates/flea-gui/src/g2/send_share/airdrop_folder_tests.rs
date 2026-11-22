use super::*;
use flea::ui_logic_send_share::omdrop::Stack;

// The `omdrop dir` runs a stand-in logged once `want` of them have, at most 10 s; a run that must not happen is
// checked after the same settle every other wait gives it.
fn dirs(root: &std::path::Path, cx: &mut VisualTestContext, want: usize) -> Vec<String> {
    let read = || {
        let log = std::fs::read_to_string(root.join("argv.log")).unwrap_or_default();
        log.lines()
            .filter(|l| l.starts_with("dir "))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    settle(cx, 300);
    while read().len() < want && Instant::now() < deadline {
        settle(cx, 20);
    }
    read()
}

#[gpui::test]
fn a_folder_chosen_for_one_tool_never_reaches_another(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let ((first, a), (second, b)) = (omdrop_stand_in("folder-a"), omdrop_stand_in("folder-b"));
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    let owly = (Stack::Owly, String::new());
    let opened_under_a = shell.update_in(cx, |shell, window, cx| {
        let _ = shell.menu.providers.omdrop.facts(&a);
        // Opened while owly ran AirDrop: Omdrop, running it now, is not told.
        shell.airdrop_folder_landed(&owly, "/tmp/for-owly", window, cx);
        shell.airdrop_tool()
    });
    shell.update_in(cx, |shell, window, cx| {
        let _ = shell.menu.providers.omdrop.facts(&b);
        // Opened under one omdrop, which another replaced: neither is told.
        shell.airdrop_folder_landed(&opened_under_a, "/tmp/for-a", window, cx);
    });
    assert!(dirs(&first, cx, 0).is_empty() && dirs(&second, cx, 0).is_empty());
    let opened_under_b = shell.update_in(cx, |shell, _, _| shell.airdrop_tool());
    shell.update_in(cx, |shell, window, cx| {
        let _ = shell.menu.providers.omdrop.facts("");
        // Opened under Omdrop, which owly took over from: Omdrop is not told.
        shell.airdrop_folder_landed(&opened_under_b, "/tmp/for-b", window, cx);
    });
    assert!(dirs(&second, cx, 0).is_empty());
    shell.update_in(cx, |shell, window, cx| {
        let _ = shell.menu.providers.omdrop.facts(&b);
        shell.airdrop_folder_landed(&opened_under_b, "/tmp/mine", window, cx);
    });
    let ran = dirs(&second, cx, 1);
    let _ = (
        std::fs::remove_dir_all(&first),
        std::fs::remove_dir_all(&second),
    );
    assert_eq!(
        ran,
        ["dir /tmp/mine"],
        "the same install still gets its own choice"
    );
}
