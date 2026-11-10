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

#[test]
fn startup_metadata_is_only_the_work_cache_stat() {
    let d = TestDir::new("empty-window-workfile");
    // A plain file answers the sweep's statx, so std makes no availability probe and no sweep thread starts.
    d.dir("flea");
    d.file("flea/work", "");
    let (library, counts) = observer(&d);
    let mut backend = Backend::observed(&d, Some((&library, &counts)));
    backend.ask(r#"{"c":"window","start":999,"count":0}"#);
    let mut expected = reader_only_counts();
    expected[METADATA_CALLS] = 1;
    assert_eq!(
        counters(&counts),
        expected,
        "the work-cache stat is the only startup metadata call"
    );
    backend.quit();
}
