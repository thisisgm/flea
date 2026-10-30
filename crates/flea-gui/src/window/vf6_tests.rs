#[test]
fn vf6_app_id_matches_desktop_and_picker() {
    let desktop = include_str!("../../../../packaging/com.thisisgm.flea.desktop");
    assert!(desktop
        .lines()
        .any(|line| line == format!("StartupWMClass={}", super::APP_ID)));
    assert_eq!(super::APP_ID, "com.thisisgm.flea");
    // The picker keeps v0.3.12's own id, the class every flea --picker block in bindings.lua names.
    assert_eq!(crate::g2::pk1::launch::APP_ID, "com.thisisgm.flea.picker");
}
