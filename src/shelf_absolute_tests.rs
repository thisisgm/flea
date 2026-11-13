use super::absolute;
use crate::backend::testdir::TestDir;
use std::os::unix::ffi::OsStrExt;

#[test]
fn an_absolute_path_needs_no_cwd_but_a_relative_path_does() {
    const CHILD: &str = "FLEA_SHELF_ABSOLUTE_NO_CWD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let cwd = std::env::current_dir().unwrap();
        assert!(root.is_absolute() && cwd.is_absolute() && cwd.starts_with(&root));
        assert_eq!(
            root.parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        assert!(root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("flea-test-"));
        assert!(root.join(".flea-test-sandbox").is_file());
        assert_eq!(cwd.file_name().unwrap(), "vanishing");
        std::fs::remove_dir(&cwd).unwrap();
        assert!(std::env::current_dir().is_err());
        assert_eq!(
            absolute("//tmp/x/../y/").unwrap().as_os_str().as_bytes(),
            b"//tmp/x/../y/"
        );
        assert!(absolute("relative/path").is_err());
        let empty = absolute("").unwrap_err();
        assert_eq!(empty.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(empty.to_string(), "an empty path has no absolute form");
        return;
    }
    let root = TestDir::new("shelf-absolute-cwd");
    let cwd = root.dir("vanishing");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "shelf::absolute_tests::an_absolute_path_needs_no_cwd_but_a_relative_path_does",
            "--nocapture",
        ])
        .env(CHILD, root.path())
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
}
