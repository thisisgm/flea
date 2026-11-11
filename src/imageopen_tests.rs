use crate::backend::testdir::TestDir;
use std::os::unix::fs::PermissionsExt;

#[test]
fn image_open_child() {
    if let Ok(line) = std::env::var("FLEA_TEST_IMAGES") {
        std::process::exit(crate::open::open_many(&crate::json::field_str_array(
            &line, "paths",
        )));
    }
}

fn handoff(exec: &str, first_body: &str, mime: &str) -> (std::process::Output, String) {
    let (out, receipt) = handoff_with(
        exec,
        first_body,
        mime,
        std::ffi::OsStr::new("unreadable companion.png"),
    );
    (out, String::from_utf8_lossy(&receipt).into_owned())
}

// The companion's name is any bytes, and the receipt is read as bytes so a raw name reads back exactly.
fn handoff_with(
    exec: &str,
    first_body: &str,
    mime: &str,
    companion: &std::ffi::OsStr,
) -> (std::process::Output, Vec<u8>) {
    use flea::bytename::ExactText as _;
    let dir = TestDir::new("image-handoff");
    let apps = dir.path().join("applications");
    std::fs::create_dir(&apps).unwrap();
    dir.file("applications/viewer.desktop", &format!("[Desktop Entry]\nType=Application\nName=Viewer\nExec={exec}\n[Desktop Action Other]\nExec=ignored %F\n"));
    let first = dir.file("first.png", first_body);
    if first_body.starts_with("#!") {
        std::fs::set_permissions(&first, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let log = dir.path().join("handoff.log");
    let stub = dir.script("gio", "#!/bin/sh\ncase \"$1\" in\ninfo) printf '  standard::content-type: %s\\n' \"$FLEA_TEST_MIME\";;\nmime) printf \"Default application for '%s': viewer.desktop\\n\" \"$2\";;\nopen|launch) printf '%s\\n' \"$@\" >> \"$FLEA_TEST_LOG\";;\nesac\n");
    let paths = format!(
        r#"{{"paths":["{}","{}"]}}"#,
        crate::json::escape(&first.to_string_lossy()),
        crate::json::escape(&dir.path().join(companion).exact_text())
    );
    let out = std::process::Command::new("timeout")
        .arg("30")
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "imageopen::handoff_tests::image_open_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("FLEA_TEST_IMAGES", paths)
        .env("FLEA_TEST_MIME", mime)
        .env("FLEA_TEST_LOG", &log)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", stub.parent().unwrap().display()),
        )
        .env("XDG_DATA_HOME", dir.path())
        .env("XDG_DATA_DIRS", dir.path())
        .output()
        .unwrap();
    let receipt = std::fs::read(&log).unwrap_or_default();
    (out, receipt)
}

#[test]
fn a_raw_named_companion_reaches_the_viewer_by_its_own_bytes() {
    use std::os::unix::ffi::OsStrExt;
    let (out, receipt) = handoff_with(
        "viewer %F",
        "picture",
        "image/png",
        std::ffi::OsStr::from_bytes(b"r\xff.png"),
    );
    assert_eq!(out.status.code(), Some(0));
    let lines: Vec<&[u8]> = receipt
        .split(|&b| b == b'\n')
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        4,
        "one launch with both images, never the single-open fallback"
    );
    assert!(
        lines[3].ends_with(b"/r\xff.png"),
        "the companion is the raw file"
    );
}

#[test]
fn the_default_list_handler_gets_one_ordered_launch_and_no_companion_stat() {
    for code in ["%F", "%U"] {
        let (out, receipt) = handoff(&format!("viewer {code}"), "picture", "image/png");
        assert_eq!(out.status.code(), Some(0));
        let lines: Vec<_> = receipt.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "launch");
        assert!(lines[1].ends_with("/applications/viewer.desktop"));
        assert!(lines[2].ends_with("/first.png"));
        assert!(lines[3].ends_with("/unreadable companion.png"));
    }
}

#[test]
fn single_path_handlers_and_nonimages_keep_the_original_open() {
    for (exec, mime) in [
        ("viewer %f", "image/png"),
        ("viewer %u", "image/png"),
        ("viewer %%F", "image/png"),
        ("viewer", "image/png"),
        ("viewer %F", "text/plain"),
    ] {
        let (out, receipt) = handoff(exec, "picture", mime);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(receipt.lines().count(), 2);
        assert!(receipt.starts_with("open\n"));
    }
}

#[test]
fn an_image_named_script_still_asks_before_any_handler_handoff() {
    let (out, receipt) = handoff("viewer %F", "#!/bin/sh\necho never\n", "image/png");
    assert_eq!(out.status.code(), Some(crate::runfile::ASK_RUN));
    assert!(String::from_utf8_lossy(&out.stdout).contains(r#""ask":"run""#));
    assert!(receipt.is_empty());
}

fn assert_escaped_gallery(prefix: &str) {
    for field in ["%F", "%U"] {
        let exec = format!("{prefix}{field}");
        assert!(super::takes_many(&exec), "{exec}");
        let (out, receipt) = handoff(&exec, "picture", "image/png");
        assert_eq!(out.status.code(), Some(0), "{exec}");
        assert_eq!(receipt.lines().count(), 4, "{exec}: {receipt:?}");
        assert!(receipt.starts_with("launch\n"), "{exec}: {receipt:?}");
    }
}

#[test]
fn desktop_escape_space_keeps_list_fields() {
    assert_escaped_gallery(r"viewer\s");
}

#[test]
fn desktop_escape_newline_keeps_list_fields() {
    assert_escaped_gallery(r"viewer\n");
}

#[test]
fn desktop_escape_tab_keeps_list_fields() {
    assert_escaped_gallery(r"viewer\t");
}

#[test]
fn desktop_escape_carriage_return_keeps_list_fields() {
    assert_escaped_gallery(r"viewer\r");
}

#[test]
fn desktop_escape_backslash_before_a_quote_keeps_list_fields() {
    assert_escaped_gallery(r#"viewer "a\\"b" "#);
}

#[test]
fn desktop_escape_space_in_a_quoted_argument_keeps_list_fields() {
    assert_escaped_gallery(r#"viewer "a\sb" "#);
}

#[test]
fn desktop_exec_values_preserve_all_string_escapes_until_decoding() {
    let dir = TestDir::new("image-exec-escapes");
    let exec = r#"viewer "a\\sb" "a\nb" "a\tb" "a\rb" "a\\\\b" %F"#;
    let app = dir.file(
        "viewer.desktop",
        &format!("[Desktop Entry]\nName=Photo\\sViewer\nExec={exec}\n"),
    );
    assert_eq!(
        crate::backend::menu_registry::desktop_exec_raw(&app)
            .unwrap()
            .as_deref(),
        Some(exec)
    );
    assert_eq!(
        crate::backend::menu_registry::desktop_label(&app)
            .unwrap()
            .as_deref(),
        Some("Photo Viewer")
    );
}
