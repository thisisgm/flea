// The words a refused LocalSend run shows; each pins the sentence a person reads, not the branch that raises it.
use super::*;

// A refused pty call says why to the person and which call refused in the log, never the bare sentence.
#[test]
fn a_pty_setup_fault_carries_its_cause() {
    let message = setup_fault("posix_openpt", &std::io::Error::from_raw_os_error(24));
    assert_eq!(message, "LocalSend could not start: too many files are open.");
}

// A write the terminal's input side refuses is the CLI no longer answering, not a fault in a terminal.
#[test]
fn a_key_that_cannot_be_written_says_localsend_stopped_answering() {
    let unwritable = OwnedFd::from(std::fs::File::open("/dev/null").expect("/dev/null"));
    let error = press(&unwritable, KEY_ENTER).unwrap_err();
    assert!(error.starts_with("LocalSend stopped answering: ") && error.ends_with('.') && !error.contains("CLI"), "{}", error);
}

// Lines no fixture reaches (a pty that will not open, a descriptor that will not clone) are pinned by their source.
#[test]
fn retired_localsend_wording_is_gone_from_the_live_source() {
    let live = include_str!("localsend.rs").split("\n#[cfg(test)]\n#[path = \"localsend_tests.rs\"]").next().unwrap();
    for retired in ["its own CLI", "for its own", "terminal setup failed", "could not reach", "its own terminal", "could not name the terminal",
                    "could not size the terminal", "could not prepare a terminal", "could not open a terminal", "could not open {}"] {
        assert!(!live.contains(retired), "retired wording came back: {}", retired);
    }
    assert_eq!(live.matches("LocalSend could not start: {}.").count(), 7, "the setup helper, the slave open, its three clones and the two master clones");
    assert_eq!(live.matches("LocalSend stopped answering: {}.").count(), 3, "the three key writes");
    assert!(live.contains("LocalSend stopped before sending to {}."));
    assert!(!live.contains("NOT_STARTED"), "no setup refusal is the bare sentence");
    for step in ["posix_openpt", "grantpt", "unlockpt", "ptsname_r", "TIOCSWINSZ"] {
        assert!(live.contains(&format!("setup_fault(\"{}\"", step)), "{} names itself in the log", step);
    }
}
