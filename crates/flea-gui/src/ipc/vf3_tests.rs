//! The service's browser effect must mount the real overlay and expose its painted bounds.
use super::pb_ipc_fixture::{draw, open, read};
use gpui::TestAppContext;

#[gpui::test]
fn vf3_share_browser_service_effect_paints_and_reports_its_owner(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    shell.update_in(cx, |shell, window, cx| {
        let origin = shell.network_origin(0);
        shell.network_service(
            &crate::g2::pl2::mount::Effect::Shares {
                uri: "smb://nas/".into(),
                label: "NAS".into(),
                names: vec!["Docs".into(), "Media".into()],
                origin,
            },
            window,
            cx,
        );
    });
    draw(cx);
    assert!(
        cx.debug_bounds("share-motion").is_some(),
        "production browser must paint"
    );
    assert_eq!(read(&shell, cx, "shareBrowserOpen", &[]), "true");
    assert!(
        !read(&shell, cx, "shareBrowserRect", &[]).is_empty(),
        "mounted overlay has real bounds"
    );
    assert_eq!(
        read(&shell, cx, "shareBrowserRect", &[]),
        read(&shell, cx, "listAreaRect", &[]),
        "the browser covers the listing slot, leaving the header visible"
    );
    let state = flea::jsondoc::parse(&read(&shell, cx, "shareBrowserState", &[])).unwrap();
    assert_eq!(state.get("owner").and_then(|v| v.as_f64()), Some(0.));
    assert_eq!(
        state.get("baseUri").and_then(|v| v.as_str()),
        Some("smb://nas/")
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(read(&shell, cx, "shareBrowserRect", &[]), "");
}
