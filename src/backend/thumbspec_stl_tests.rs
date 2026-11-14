use super::*;

/// Check provider precedence, aliases and the saved system fallback.
#[test]
fn native_stl_sits_between_user_and_system_providers() {
    let aliases = Aliases::from_str("application/sla model/stl\n");
    let native = Spec {
        exec: vec!["/bin/true".into(), "--stl-thumbnail-worker".into()],
    };
    let provider = |name: &str| {
        (
            format!("{name}.thumbnailer"),
            format!("[Thumbnailer Entry]\nExec=/bin/true --{name}\nMimeType=application/sla;\n"),
        )
    };
    let system = [provider("system")];
    let table = Thumbnailers::from_layers(&[], &system, &aliases, Some(native.clone()));
    assert_eq!(
        table.for_mime("model/stl", &aliases).unwrap().exec[1],
        "--stl-thumbnail-worker"
    );
    assert_eq!(
        table.stl_fallback("model/stl", &aliases).unwrap().exec[1],
        "--system"
    );
    let without_fallback = Thumbnailers::from_layers(&[], &[], &aliases, Some(native.clone()));
    assert_ne!(table.fingerprint(), without_fallback.fingerprint());

    let user = [provider("user")];
    let table = Thumbnailers::from_layers(&user, &system, &aliases, Some(native));
    assert_eq!(
        table.for_mime("model/stl", &aliases).unwrap().exec[1],
        "--user"
    );
    assert!(table.stl_fallback("model/stl", &aliases).is_none());
}
