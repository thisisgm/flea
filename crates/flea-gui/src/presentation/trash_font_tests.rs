//! A delayed fontconfig answer must reach the window and name a family GPUI can actually open.
use super::{
    fixture::Fixture,
    service::{FALLBACK_FAMILY, TICK},
    source::{Sources, INPUT_CAP},
    Service,
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// The child's whole environment, so a parent-side fc-match answers from the fontconfig the child reads.
fn sandboxed<'a>(command: &'a mut Command, root: &Path) -> &'a mut Command {
    command
        .env_clear()
        .env("TRASH_FONT_ROOT", root)
        .env("HOME", root)
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .stdin(Stdio::null());
    for key in ["TRASH_FONT_FACE", "TRASH_FONT_FAMILY"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
}

/// What fontconfig itself answers for `monospace` inside the child's sandbox.
fn sandboxed_answer(fixture: &Fixture) -> Vec<u8> {
    let mut command = Command::new("/usr/bin/fc-match");
    command.args(["-f", "%{family[0]}", "monospace"]);
    let answer = sandboxed(&mut command, fixture.path()).output().unwrap();
    assert!(answer.status.success());
    answer.stdout
}

fn isolated(test: &str, fixture: &Fixture, script: &str) {
    let root = fixture.path();
    let helper = root.join("bin/fc-match");
    fixture.write(&helper, script);
    fixture.executable(&helper);
    let mut command = Command::new("/usr/bin/setsid");
    command
        .arg("-w")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"]);
    let output = sandboxed(&mut command, root).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success(), "isolated font regression failed");
    // libtest exits 0 when --exact matches nothing, so the child must report the one test it ran.
    assert!(
        stdout.contains("test result: ok. 1 passed"),
        "the isolated child ran no test named {test}"
    );
}

fn accepted() -> Option<(Service, super::Snapshot)> {
    let root = std::env::var_os("TRASH_FONT_ROOT")?;
    let fixture = Fixture::open(root.into()).unwrap();
    assert_eq!(std::env::var_os("HOME").unwrap(), fixture.path());
    let (service, mut snapshot) = Service::start();
    assert_eq!(
        snapshot.fonts.family, FALLBACK_FAMILY,
        "the slow probe missed the launch deadline"
    );
    let updates = service.subscribe();
    let until = Instant::now() + Duration::from_secs(6);
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let expected = std::fs::read_to_string(fixture.path().join("expected")).ok();
        if expected
            .as_deref()
            .is_some_and(|name| name.trim() == snapshot.fonts.family)
        {
            break;
        }
        while let Ok(next) = updates.try_recv() {
            snapshot = next;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let expected = std::fs::read_to_string(fixture.path().join("expected")).unwrap();
    println!(
        "TRASH_FONT accepted={:?} expected={:?}",
        snapshot.fonts.family,
        expected.trim()
    );
    assert_eq!(
        snapshot.fonts.family,
        expected.trim(),
        "slow probe never replaced the launch face"
    );
    Some((service, snapshot))
}

#[test]
fn trash_font_repro_fcslow_publishes_without_an_input_change() {
    if let Some((_service, snapshot)) = accepted() {
        assert_eq!(snapshot.fonts.family, "TRASH FONT Custom Mono");
        return;
    }
    let fixture = Fixture::new("trash-font-slow");
    fixture.write(&fixture.path().join("expected"), "TRASH FONT Custom Mono");
    isolated(
        "presentation::trash_font_tests::trash_font_repro_fcslow_publishes_without_an_input_change",
        &fixture,
        "#!/bin/sh\nsleep 1\nprintf 'TRASH FONT Custom Mono'\n",
    );
    fixture.cleanup();
}

#[test]
fn trash_font_fontconfig_answer_resolves_in_gpui_database() {
    if let Some((_service, snapshot)) = accepted() {
        let text = gpui_platform::current_platform(true).text_system();
        if let Some(path) = std::env::var_os("TRASH_FONT_FACE") {
            text.add_fonts(vec![std::borrow::Cow::Owned(std::fs::read(path).unwrap())])
                .unwrap();
        }
        let face = snapshot.fonts.typeface(&flea::jsondoc::Json::Null);
        assert!(
            text.all_font_names().contains(&snapshot.fonts.family),
            "fontconfig family is absent from GPUI's database"
        );
        let id = text
            .font_id(&face.font)
            .expect("GPUI cannot open fontconfig's exact family");
        println!(
            "TRASH_FONT gpui_family={:?} font_id={id:?}",
            snapshot.fonts.family
        );
        assert!(
            text.font_id(&gpui::font("monospace")).is_err(),
            "negative control unexpectedly names a real family"
        );
        return;
    }
    let fixture = Fixture::new("trash-font-family");
    let family = std::env::var("TRASH_FONT_FAMILY")
        .map(String::into_bytes)
        .unwrap_or_else(|_| sandboxed_answer(&fixture));
    fixture.write(&fixture.path().join("expected"), family);
    isolated(
        "presentation::trash_font_tests::trash_font_fontconfig_answer_resolves_in_gpui_database",
        &fixture,
        "#!/bin/sh\nsleep 1\nif [ -n \"${TRASH_FONT_FAMILY:-}\" ]; then printf '%s' \"$TRASH_FONT_FAMILY\"; else exec /usr/bin/fc-match \"$@\"; fi\n",
    );
    fixture.cleanup();
}

#[test]
fn trash_font_window_accepts_a_350ms_fontconfig_answer_without_restart() {
    if let Some(root) = std::env::var_os("TRASH_FONT_ROOT") {
        use gpui::AppContext;
        let fixture = Fixture::open(root.into()).unwrap();
        let expected = std::fs::read_to_string(fixture.path().join("expected")).unwrap();
        let (release, ready) = std::sync::mpsc::channel();
        let (service, initial) = super::fixture::service_after(move || {
            let _ = ready.recv();
        });
        let first = initial.fonts.family.clone();
        assert_eq!(
            first, FALLBACK_FAMILY,
            "the 350 ms answer missed the launch deadline"
        );
        let mut cx = crate::list::bench::real_cx();
        cx.update(|cx| initial.install(cx));
        let (backend, _requests) = crate::backend::Backend::test_channel();
        let handle = cx.add_window(|window, cx| crate::window::root::Root {
            shell: cx.new(|cx| {
                let mut shell = crate::state::tests::shell_with(backend, cx);
                shell.follow_presentation(&service, window, cx);
                shell
            }),
        });
        cx.run_until_parked();
        cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        fixture.write(&fixture.path().join(super::fixture::PROBE_OPEN), "");
        release.send(()).unwrap();
        // The patient probe starts after the fallback has actually been drawn.
        let until = Instant::now() + Duration::from_secs(20);
        let mut family = first.clone();
        while Instant::now() < until {
            cx.executor().advance_clock(Duration::from_millis(20));
            cx.run_until_parked();
            cx.update_window(handle.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .unwrap();
            family = handle
                .update(&mut cx, |root, _, cx| {
                    root.shell
                        .read(cx)
                        .active_typeface()
                        .font
                        .family
                        .to_string()
                })
                .unwrap();
            if family == expected.trim() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        println!(
            "TRASH_FONT_350MS first={first:?} window={family:?} expected={:?}",
            expected.trim()
        );
        assert_eq!(
            family,
            expected.trim(),
            "the real window subscription kept the launch fallback"
        );
        return;
    }
    // A name no fontconfig answers, so the window can only reach it through the late probe and never on the fallback.
    let fixture = Fixture::new("trash-font-350ms");
    fixture.write(&fixture.path().join("expected"), "TRASH FONT Late Mono");
    isolated("presentation::trash_font_tests::trash_font_window_accepts_a_350ms_fontconfig_answer_without_restart", &fixture,
        &super::fixture::gated_probe("#!/bin/sh\nsleep 0.35\nprintf 'TRASH FONT Late Mono'\n"));
    fixture.cleanup();
}

#[test]
fn trash_font_a_hung_fontconfig_costs_one_patient_retry_and_no_more() {
    if let Some(root) = std::env::var_os("TRASH_FONT_ROOT") {
        let calls = Path::new(&root).join("calls");
        let (_service, initial) = Service::start();
        assert_eq!(
            initial.fonts.family, FALLBACK_FAMILY,
            "the hung probe missed the launch deadline"
        );
        // An unbounded retry starts a third probe about two ticks past the patient deadline; six leave slack for a loaded box.
        std::thread::sleep(super::face::PATIENT + TICK * 6);
        let probes = std::fs::read_to_string(&calls)
            .unwrap_or_default()
            .lines()
            .count();
        println!("TRASH_FONT_HUNG probes={probes}");
        assert_eq!(
            probes, 2,
            "a hung fc-match gets the launch probe and one patient retry"
        );
        return;
    }
    let fixture = Fixture::new("trash-font-hung");
    isolated(
        "presentation::trash_font_tests::trash_font_a_hung_fontconfig_costs_one_patient_retry_and_no_more",
        &fixture,
        "#!/bin/sh\necho probe >> \"$TRASH_FONT_ROOT/calls\"\nexec sleep 30\n",
    );
    fixture.cleanup();
}

#[test]
fn trash_font_isolated_refuses_a_child_that_ran_no_test() {
    let fixture = Fixture::new("trash-font-no-test");
    let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        isolated(
            "presentation::trash_font_tests::no_such_test",
            &fixture,
            "#!/bin/sh\nexit 1\n",
        )
    }));
    fixture.cleanup();
    let payload = refused.expect_err("a child that matched no test passed as green");
    // A formatted panic carries a String and a literal one a &str, so read both before matching the guard's words.
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or_default();
    assert!(
        message.contains("the isolated child ran no test named"),
        "isolated refused the child for another reason: {message}"
    );
}

#[test]
fn trash_font_an_unreadable_input_does_not_spend_the_patient_retry() {
    if let Some((_service, snapshot)) = accepted() {
        // A live read that refuses the input leaves the read-error arm as the only way the late face arrives.
        let refused = Sources::current().read().err().map(|error| error.kind());
        assert_eq!(
            refused,
            Some(std::io::ErrorKind::InvalidData),
            "the oversized input read as valid"
        );
        assert_eq!(snapshot.fonts.family, "TRASH FONT Late Mono");
        return;
    }
    let fixture = Fixture::new("trash-font-unreadable");
    // One byte past the input cap: the launch counts the file absent and every live read refuses it.
    fixture.write(
        &fixture.path().join(".config/omarchy/shell.toml"),
        vec![b'#'; INPUT_CAP + 1],
    );
    fixture.write(&fixture.path().join("expected"), "TRASH FONT Late Mono");
    isolated(
        "presentation::trash_font_tests::trash_font_an_unreadable_input_does_not_spend_the_patient_retry",
        &fixture,
        "#!/bin/sh\nsleep 0.35\nprintf 'TRASH FONT Late Mono'\n",
    );
    fixture.cleanup();
}
