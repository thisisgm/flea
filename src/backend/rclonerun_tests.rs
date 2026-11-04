use super::*;

#[test]
fn rclone_child_targets_distinguish_the_remote_root_from_a_colon_folder() {
    for (folder, expected) in [
        ("gdrive:", "gdrive:item"),
        ("gdrive:photos", "gdrive:photos/item"),
        ("gdrive:photos/", "gdrive:photos/item"),
        ("gdrive:photos:", "gdrive:photos:/item"),
        ("gdrive::", "gdrive::/item"),
        ("gdrive:/absolute/remote:", "gdrive:/absolute/remote:/item"),
    ] {
        assert_eq!(child_target(folder, "item"), expected);
    }
}

#[test]
fn rclone_commands_force_source_traversal_and_skip_reporting_despite_backend_overrides() {
    for op in ["copy", "copyto", "check"] {
        let args = arguments(op, "/source", "gdrive:target", false);
        for flag in [
            "--copy-links=false",
            "--links=false",
            "--local-links=false",
            "--skip-links=false",
            "--skip-specials=false",
            "--one-file-system=false",
        ] {
            assert!(args.iter().any(|arg| arg == flag), "{op} {flag}");
        }
    }
}

#[test]
fn rclone_skip_notices_fail_even_when_the_child_exits_successfully() {
    for msg in [
        "Can't follow symlink without -L/--copy-links",
        "Can't transfer non file/directory",
    ] {
        let mut log = Log::default();
        log.line(&format!(
            r#"{{"level":"notice","object":"deep/skipped","msg":"{}"}}"#,
            msg
        ));
        log.line(r#"{"level":"info","msg":"1 matching files"}"#);
        for checking in [false, true] {
            assert!(verdict(true, &log, "tree", "gdrive", checking)
                .unwrap_err()
                .contains("deep/skipped"));
        }
    }
}
