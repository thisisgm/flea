use super::*;
use crate::backend::testdir::TestDir;

// Every name the search tries is taken, so the answer says no temporary folder could be made.
#[test]
fn a_work_folder_that_cannot_be_made_says_so_in_plain_words() {
    // The sequence is shared by parallel tests, so a band of taken names this wide outlasts whatever they consume meanwhile.
    const TAKEN_BAND: usize = 4096;
    let d = TestDir::new("archworkfull");
    let start = WORK_SEQ.load(Ordering::Relaxed);
    for seq in start..start + TAKEN_BAND {
        std::fs::create_dir(d.join(&format!("{}full-{}-{}", WORK_PREFIX, std::process::id(), seq))).expect("a taken name");
    }
    let error = Work::new(d.path(), "full").err().expect("every name is taken");
    assert_eq!(error.msg, "a temporary folder could not be made beside the destination");
}

// An empty PATH holds no bwrap, so both runners answer before any child starts; the child process keeps PATH off the other tests.
#[test]
fn a_missing_sandbox_names_the_package_to_install() {
    if std::env::var_os("FLEA_SANDBOX_MISSING_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "backend::archivework::text_tests::a_missing_sandbox_names_the_package_to_install"])
            .env("PATH", "").env("FLEA_SANDBOX_MISSING_CHILD", "1").output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"), "the isolated missing-sandbox check did not run");
        return;
    }
    let d = TestDir::new("archnosandbox");
    let mut work = Work::new(d.path(), "arc").expect("work");
    let inner = vec!["/usr/bin/true".to_string()];
    let cancel = AtomicBool::new(false);
    let plain = run_boxed_cancellable("archive", inner.clone(), d.path(), &mut work, &cancel).unwrap_err();
    assert_eq!(plain.msg, "the sandbox is unavailable: bubblewrap or prlimit is not installed");
    let password = crate::backend::archivepass::run_boxed_cancellable_password("archive", "out.7z", inner, d.path(), &mut work, None, &cancel).unwrap_err();
    assert_eq!(password.msg, "the sandbox is unavailable: bubblewrap or prlimit is not installed");
}
