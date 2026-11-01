#[test]
fn review_suspend_child() {
    let Some(root) = std::env::var_os("FLEA_T1_REVIEW_SUSPEND_ROOT") else { return; };
    let root = std::path::PathBuf::from(root);
    assert!(root.is_absolute() && root.join(".flea-test-sandbox").is_file());
    unsafe { signal(SIGTSTP, 0); }
    let mut terminal = Terminal::enter_at(&root).unwrap();
    terminal.decode(b"\x1b[?0u", false).unwrap();
    println!("REVIEW_READY");
    if std::env::var("FLEA_T1_REVIEW_SUSPEND_MODE").as_deref() == Ok("editor") {
        let result = terminal.run_editor("true", &root.join("names"));
        assert!(terminal.stopped(), "the editor handoff ended without a stop: {result:?}");
        assert_eq!(result.unwrap_err().to_string(), crate::tui::words::error(EDITOR_INTERRUPTED).to_string());
        return;
    }
    if std::env::var("FLEA_T1_REVIEW_SUSPEND_MODE").as_deref() == Ok("key") {
        let (mut wire, reader) = crate::tui::wire::echo_wire();
        let mut model = crate::tui::action_key_tests::listing(&mut wire, "yazi");
        crate::tui::actions::key(&mut model, &crate::tui::input::Key::named("Z", "ctrl"), &crate::tui::keymap::Map::load(), &mut wire).unwrap();
        assert!(model.parity17.suspend);
        terminal.request_suspend();
        crate::tui::wire::finish(wire, reader);
    }
    while !terminal.stopped() {
        let bytes = terminal.read(-1, 20).unwrap();
        let _ = terminal.decode(&bytes, false).unwrap();
        if terminal.take_resumed() { println!("REVIEW_RESUMED"); }
    }
}

#[test]
fn review_suspension_restores_keyboard_flags_and_terminal_mode() {
    let dir = TestDir::new("review-suspend");
    let mut session = suspension::Session::start(dir.path(), "job", None);
    let saved = session.saved.clone();
    session.read_until(b"REVIEW_READY");
    session.signal(SIGTSTP);
    let suspended = session.wait_stopped();
    assert!(suspended, "child did not suspend");
    session.read_until(LEAVE);
    let mode_while_stopped = suspension::mode(&session.slave);
    let pops_at_stop = session.transcript.windows(super::super::capabilities::KEYBOARD_DISABLE.len())
        .filter(|p| *p == super::super::capabilities::KEYBOARD_DISABLE).count();
    session.signal(SIGCONT);
    session.read_until(b"REVIEW_RESUMED");
    session.signal(SIGTERM);
    let status = session.finish();
    assert!(status.success(), "PTY child {status}: {}", String::from_utf8_lossy(&session.transcript));
    println!("SIGTSTP stopped={suspended}, keyboard pops before stop={pops_at_stop}, raw mode remained={}", mode_while_stopped != saved);
    assert_eq!(pops_at_stop, 1, "keyboard protocol remained enabled while suspended");
    assert_eq!(mode_while_stopped, saved);
    assert_eq!(suspension::mode(&session.slave), saved);
}

// SIGTERM lands while the case's held stty runs; aimed at the helper alone it stands for a job stop no thread has handled yet.
fn interrupted_transition(case: &str, whole_job: bool) {
    // The child's mode and the stty call the sandbox holds: the first restore after startup, or the second raw.
    let (mode, call) = match case {
        "suspend" | "exit" => ("job", "restore"),
        "resume" => ("job", "raw"),
        "editor-leave" => ("editor", "restore"),
        "editor-return" => ("editor", "raw"),
        _ => panic!("unknown case {case}"),
    };
    let dir = TestDir::new("interrupt-suspend");
    let gate = dir.join("stty-gate");
    dir.assert_contains(&gate);
    assert!(Command::new("mkfifo").arg(&gate).status().unwrap().success());
    // Hold the selected stty call at a FIFO open until SIGTERM ends it.
    dir.script("stty", r#"#!/bin/sh
root=$FLEA_T1_REVIEW_SUSPEND_ROOT
call=
if [ "$1" = raw ]; then
    if [ -f "$root/prepared" ]; then call=raw; else : > "$root/prepared"; fi
elif [ "$1" != -g ]; then
    call=restore
fi
if [ "$call" = "$FLEA_T1_INTERRUPT_CALL" ] && [ ! -f "$root/interrupted" ]; then
    : > "$root/interrupted"
    echo $$ > "$root/stty-pid"
    printf 'REVIEW_INTERRUPTING\n'
    exec /bin/cat "$root/stty-gate"
fi
exec /usr/bin/stty "$@"
"#);
    let mut session = suspension::Session::start(dir.path(), mode, Some(call));
    let saved = session.saved.clone();
    session.read_until(b"REVIEW_READY");
    match case {
        "suspend" => session.signal(SIGTSTP),
        "resume" => {
            session.signal(SIGTSTP);
            assert!(session.wait_stopped(), "child did not suspend before resume interruption");
            session.signal(SIGCONT);
        }
        // The first SIGTERM ends the loop, so the held restore is Drop's own.
        "exit" => session.signal(SIGTERM),
        _ => {}
    }
    session.read_until(b"REVIEW_INTERRUPTING");
    if whole_job { session.signal(SIGTERM); } else {
        let helper = std::fs::read_to_string(dir.join("stty-pid")).unwrap();
        crate::procsignal::signal_process(helper.trim().parse().unwrap(), SIGTERM).unwrap();
    }
    let status = session.finish();
    assert!(status.success(), "PTY {case} child {status}: {}", String::from_utf8_lossy(&session.transcript));
    assert_eq!(suspension::mode(&session.slave), saved, "PTY {case} left termios changed");
    assert_eq!(session.transcript.windows(super::super::capabilities::KEYBOARD_DISABLE.len())
        .filter(|p| *p == super::super::capabilities::KEYBOARD_DISABLE).count(), 1);
    println!("SIGTERM to the {} during {case}: successful exit, keyboard popped, termios restored=true", if whole_job { "job" } else { "stty" });
}

#[test]
fn termination_during_suspend_restores_terminal_without_error() { interrupted_transition("suspend", true); }

#[test]
fn termination_during_resume_restores_terminal_without_error() { interrupted_transition("resume", true); }

#[test]
fn a_stty_killed_by_a_stop_signal_ends_the_suspend_without_error() { interrupted_transition("suspend", false); }

#[test]
fn termination_before_the_editor_ends_the_rename_as_interrupted() { interrupted_transition("editor-leave", true); }

#[test]
fn termination_during_the_editor_return_ends_the_rename_as_interrupted() { interrupted_transition("editor-return", true); }

#[test]
fn a_second_termination_during_the_exit_restore_still_restores_the_terminal() { interrupted_transition("exit", true); }

#[test]
fn tui_parity_suspend_key_restores_and_resumes_through_the_terminal_lifecycle() {
    let root=TestDir::new("tui17-suspend-key");
    let mut session=suspension::Session::start(root.path(),"key",None);
    session.read_until(b"REVIEW_READY");
    assert!(session.wait_stopped()); session.read_until(LEAVE);
    assert_eq!(suspension::mode(&session.slave),session.saved);
    let pops=session.transcript.windows(super::super::capabilities::KEYBOARD_DISABLE.len()).filter(|p| *p == super::super::capabilities::KEYBOARD_DISABLE).count();
    assert_eq!(pops,1);
    session.signal(SIGCONT); session.read_until(b"REVIEW_RESUMED");
    assert!(session.transcript.windows(ENTER.len()).filter(|p| *p==ENTER).count()>=2);
    session.signal(SIGTERM); assert!(session.finish().success());
    assert_eq!(suspension::mode(&session.slave),session.saved);
}
