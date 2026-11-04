use super::*;

#[test]
fn an_absolute_hook_wins_and_anything_else_is_the_system_binary() {
    assert_eq!(qjs_from(None), PathBuf::from(SYSTEM_QJS));
    assert_eq!(qjs_from(Some(OsString::from(""))), PathBuf::from(SYSTEM_QJS));
    assert_eq!(qjs_from(Some(OsString::from("relative/qjs"))), PathBuf::from(SYSTEM_QJS));
    assert_eq!(
        qjs_from(Some(OsString::from("/tmp/flea-qjs-for-this-test"))),
        PathBuf::from("/tmp/flea-qjs-for-this-test")
    );
}

// A UI tree of its own, so the test names exactly what is missing.
fn ui_tree(name: &str, with_helper: bool) -> crate::backend::testdir::TestDir {
    let dir = crate::backend::testdir::TestDir::new(name);
    let vendor = dir.path().join("vendor");
    std::fs::create_dir(&vendor).expect("test ui vendor dir");
    if with_helper {
        std::fs::write(vendor.join(HELPER_NAME), "// helper\n").expect("test helper file");
    }
    dir
}

#[test]
fn a_missing_sandbox_is_refused_before_anything_is_probed() {
    let dir = ui_tree("figure-no-sandbox", true);
    let err = resolve_with(false, Path::new("/nonexistent-figure-qjs-for-this-test"), Some(dir.path()), None, &[])
        .expect_err("without a sandbox the jail is refused first");
    // Sample refusal: "the figure helper needs bwrap and prlimit, and one of them is missing".
    assert!(err.contains("bwrap"), "the jail is refused first: {err}");
}

#[test]
fn a_missing_engine_is_refused_without_running_anything_unsandboxed() {
    let dir = ui_tree("figure-qjs-missing", true);
    let err = resolve_with(true, Path::new("/nonexistent-figure-qjs-for-this-test"), Some(dir.path()), None, &[])
        .expect_err("a missing qjs must refuse");
    assert!(err.contains("quickjs-ng"), "a missing engine must be named: {err}");
}

#[test]
fn a_missing_helper_file_is_refused() {
    let dir = ui_tree("figure-helper-missing", false);
    // An existing file stands in for the engine, so only the helper is missing.
    std::fs::write(dir.path().join("qjs"), "#!/bin/sh\n").expect("test engine file");
    let err = resolve_with(true, &dir.path().join("qjs"), Some(dir.path()), None, &[])
        .expect_err("a missing helper file must refuse");
    assert_eq!(err, format!("the figure helper is missing at {}", dir.path().join("vendor").join(HELPER_NAME).display()));
}

#[test]
fn a_missing_worker_file_is_refused_before_starting_the_helper() {
    let dir = ui_tree("figure-worker-missing", true);
    let err = resolve_with(true, Path::new("/bin/true"), Some(dir.path()), None, &[])
        .expect_err("a missing worker file must refuse");
    assert_eq!(err, format!("the figure worker module is missing at {}", dir.path().join("js").join(WORKER_NAME).display()));
}

#[test]
fn a_missing_ui_tree_is_refused() {
    let err = resolve_with(true, Path::new("/bin/true"), None, None, &[]).expect_err("no UI tree must refuse");
    assert!(err.contains("UI tree"), "a missing UI tree must be named: {err}");
}

#[test]
fn only_a_usr_binary_is_already_covered_by_the_read_only_usr_bind() {
    assert!(under_usr(Path::new("/usr/bin/qjs")));
    assert!(!under_usr(Path::new("/home/gm/.superpowers/tools/qjs")));
}

// Sample layout: vendor /ui/vendor with the worker at /ui/js/FigureWorker.mjs.
fn argv_for(qjs: &str) -> Vec<String> {
    figure_argv(Path::new(qjs), Path::new("/ui/vendor"), None, &[])
}

#[test]
fn the_argv_carries_the_caps_the_flags_both_binds_and_qjs_last() {
    let got = argv_for("/usr/bin/qjs");
    let joined = got.join(" ");
    assert_eq!(got[0], "prlimit");
    assert!(got.iter().any(|a| a == "--cpu=30"), "the runaway bound: {joined}");
    assert!(got.iter().any(|a| a == "--as=2147483648"), "the address-space cap: {joined}");
    assert!(got.iter().any(|a| a == "--unshare-all"), "the namespace flags: {joined}");
    assert!(got.iter().any(|a| a == "--clearenv"), "no inherited environment: {joined}");
    assert!(joined.contains("--ro-bind /usr /usr"), "system libraries stay visible: {joined}");
    assert!(joined.contains("--ro-bind /ui/vendor /ui/vendor"), "the bundles: {joined}");
    assert!(joined.contains("--ro-bind /ui/js/FigureWorker.mjs /ui/js/FigureWorker.mjs"), "the shared post-processing: {joined}");
    assert!(!joined.contains("--bind "), "nothing writable: {joined}");
    assert!(!got.iter().any(|a| a.contains("FLEA_QJS")), "the hook never reaches argv: {joined}");
    let tail = &got[got.len() - 2..];
    assert_eq!(tail, ["/usr/bin/qjs", "/ui/vendor/figure-helper.mjs"]);
}

// Sample argv: "--clearenv", "--setenv", "TZ", "UTC" ahead of the locale pin.
#[test]
fn the_helper_argv_sets_utc_after_clearing_the_environment() {
    let got = argv_for("/usr/bin/qjs");
    let cleared = got.iter().position(|a| a == "--clearenv").expect("--clearenv");
    assert_eq!(got[cleared + 1..cleared + 4], ["--setenv", "TZ", "UTC"], "the pin must follow --clearenv");
    assert_eq!(got.iter().filter(|a| a.as_str() == "TZ").count(), 1, "the zone is pinned exactly once");
}

#[test]
fn an_argv_without_a_clearenv_still_pins_utc_after_bwrap() {
    let mut argv = ["prlimit", "--cpu=5", "bwrap", "--unshare-all", "/usr/bin/qjs"].map(String::from).to_vec();
    pin_utc(&mut argv);
    assert_eq!(argv, ["prlimit", "--cpu=5", "bwrap", "--setenv", "TZ", "UTC", "--unshare-all", "/usr/bin/qjs"]);
}

#[test]
fn an_argv_that_is_not_a_jail_gets_no_bwrap_flag() {
    for given in [vec!["/usr/bin/qjs", "/ui/vendor/figure-helper.mjs"], vec![]] {
        let mut argv: Vec<String> = given.iter().map(|a| a.to_string()).collect();
        pin_utc(&mut argv);
        assert_eq!(argv, given, "a launch outside bwrap is left as it is");
    }
}

#[test]
fn a_verified_bytecode_directory_is_bound_read_only_and_named_after_the_helper() {
    let got = figure_argv(Path::new("/usr/bin/qjs"), Path::new("/ui/vendor"), Some(Path::new("/cache/flea/figures/k")), &[]);
    let joined = got.join(" ");
    assert!(joined.contains("--ro-bind /cache/flea/figures/k /cache/flea/figures/k"), "{joined}");
    assert!(!joined.contains("--bind "), "the render jail still has nothing writable: {joined}");
    assert_eq!(got[got.len() - 3..], ["/usr/bin/qjs", "/ui/vendor/figure-helper.mjs", "--bytecode=/cache/flea/figures/k"]);
    let source = argv_for("/usr/bin/qjs");
    assert_eq!(source[source.len() - 1], "/ui/vendor/figure-helper.mjs", "no cache, no extra argument");
}

#[test]
fn a_warm_start_names_its_kinds_after_the_helper_and_never_widens_the_jail() {
    let got = figure_argv(Path::new("/usr/bin/qjs"), Path::new("/ui/vendor"), None, &["mermaid", "math"]);
    assert_eq!(got[got.len() - 3..], ["/usr/bin/qjs", "/ui/vendor/figure-helper.mjs", "--warm=mermaid,math"]);
    assert!(!got.join(" ").contains("--bind "), "nothing writable");
    let both = figure_argv(Path::new("/usr/bin/qjs"), Path::new("/ui/vendor"), Some(Path::new("/c/k")), &["math"]);
    assert_eq!(both[both.len() - 2..], ["--bytecode=/c/k", "--warm=math"]);
}

#[test]
fn a_reporting_start_names_the_flag_last_and_widens_nothing() {
    let got = reporting(figure_argv(Path::new("/usr/bin/qjs"), Path::new("/ui/vendor"), Some(Path::new("/c/k")), &["math"]));
    assert_eq!(got[got.len() - 3..], ["--bytecode=/c/k", "--warm=math", "--report"]);
    assert!(!got.join(" ").contains("--bind "), "nothing writable");
    assert!(!argv_for("/usr/bin/qjs").iter().any(|a| a == REPORT_FLAG), "a plain start never reports");
}

#[test]
fn only_known_kinds_survive_the_warm_argument() {
    assert_eq!(warm_kinds(None), Vec::<&str>::new());
    assert_eq!(warm_kinds(Some("--warm=mermaid,../x,math,mermaid")), ["mermaid", "math"]);
    assert_eq!(warm_kinds(Some("--warm=")), Vec::<&str>::new());
    assert_eq!(warm_kinds(Some("--warm=Math,MATH;rm -rf /,--bytecode=/etc")), Vec::<&str>::new());
    assert_eq!(warm_kinds(Some("math")), Vec::<&str>::new(), "the flag prefix is required");
}

#[test]
fn a_hook_outside_usr_is_bound_read_only_beside_the_vendor_tree() {
    let got = argv_for("/home/gm/.superpowers/tools/qjs");
    let joined = got.join(" ");
    assert!(joined.contains("--ro-bind /home/gm/.superpowers/tools/qjs /home/gm/.superpowers/tools/qjs"), "{joined}");
    assert!(joined.contains("--ro-bind /ui/vendor /ui/vendor"), "{joined}");
}

#[test]
fn the_dispatch_claims_only_exact_figure_arguments() {
    let argv = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<String>>();
    // Only the refusals run here, since a claimed mode would exec or serve.
    for refused in [&["flea"][..], &["flea", "--figure-store", "x"], &["flea", "--figure-compile", "x"], &["flea", "--figure-helper", "x"], &["flea", "--figure-helper", "--warm=math", "x"], &["flea", "--figure"], &["flea", "x", "--figure-store"]] {
        assert_eq!(dispatch(&argv(refused)), None, "{refused:?}");
    }
}
