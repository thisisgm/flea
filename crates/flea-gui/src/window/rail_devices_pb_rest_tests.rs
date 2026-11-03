//! Ctrl+E keeps the backend's system-volume refusal tied to the shown directory.
use super::*;

#[gpui::test]
fn pb_rest_ctrl_e_says_system_volume_from_matching_backend_facts(cx: &mut TestAppContext) {
    let (shell, service, cx) = shell_at("/fixture/work", cx);
    listing_focus(&shell, cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| {
        shell.receive(0, crate::backend::Backend::test_reply_line(r#"{"t":"fsinfo","path":"/fixture/work","fs":"ext4","free":1000,"class":"local","system":true}"#), window, cx);
    }));
    cx.simulate_keystrokes("ctrl-e");
    frame(cx);
    let expected = shell.read_with(cx, |shell, _| {
        shell.locale.catalog.tr("device.system_volume")
    });
    assert_eq!(status(&shell, cx), expected);
    assert!(!jobs(&service)
        .iter()
        .any(|job| job.starts_with("gio mount -u ") || job.starts_with("gio mount -e ")));
}

#[gpui::test]
fn pb_rest_stale_or_cleared_system_facts_keep_the_current_folder_refusal(cx: &mut TestAppContext) {
    let (shell, _service, cx) = shell_at("/fixture/work", cx);
    listing_focus(&shell, cx);
    for (wire, key) in [
        (
            r#"{"t":"fsinfo","path":"/elsewhere","fs":"ext4","free":1000,"class":"local","system":true}"#,
            "device.no_volume",
        ),
        (
            r#"{"t":"fsinfo","path":"/fixture/work","fs":"ext4","free":1000,"class":"local","system":true}"#,
            "device.system_volume",
        ),
        (
            r#"{"t":"fsinfo","path":"/fixture/work","fs":"ext4","free":1000,"class":"local"}"#,
            "device.no_volume",
        ),
    ] {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                shell.receive(
                    0,
                    crate::backend::Backend::test_reply_line(wire),
                    window,
                    cx,
                );
            })
        });
        cx.simulate_keystrokes("ctrl-e");
        frame(cx);
        let expected = shell.read_with(cx, |shell, _| shell.locale.catalog.tr(key));
        assert_eq!(status(&shell, cx), expected);
    }
}
