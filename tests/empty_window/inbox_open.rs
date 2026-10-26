use super::*;

#[test]
fn inbox_open_errors_name_the_descriptor_that_failed() {
    let d = TestDir::new("inbox-open-error");
    let source = d.file("fail-open.c", include_str!("fail_open.c"));
    let library = d.join("fail-open.so");
    assert!(Command::new("cc")
        .args(["-shared", "-fPIC", "-O2", "-Wall", "-Werror"])
        .arg(&source)
        .arg("-o")
        .arg(&library)
        .arg("-ldl")
        .status()
        .unwrap()
        .success());
    for descriptor in ["eventfd", "timerfd"] {
        let output = Command::new(env!("CARGO_BIN_EXE_flea"))
            .arg("--backend")
            .env_clear()
            .env("HOME", d.path())
            .env("XDG_CACHE_HOME", d.path())
            .env("LD_PRELOAD", &library)
            .env("FLEA_TEST_FAIL_OPEN", descriptor)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let reply = String::from_utf8(output.stdout).unwrap();
        assert!(
            reply.contains(&format!(r#""path":"{descriptor}""#)),
            "{descriptor}: {reply}"
        );
        assert!(reply.contains(r#""where":"open""#), "{reply}");
        assert!(reply.contains("too many files are open"), "{reply}");
    }
}
