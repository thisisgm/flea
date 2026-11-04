use super::*;
use std::time::{Duration, Instant};

const RETRY_WINDOW: Duration = Duration::from_millis(1200);
// Longer than the service's 250 ms launch probe deadline, so the first frame still draws the fallback.
const LATE_PROBE_SECONDS: &str = "0.35";

fn mounted() -> (
    TestAppContext,
    Arc<receipt::Recorder>,
    crate::presentation::Service,
    Handle,
) {
    let recorder = receipt::Recorder::new();
    let mut cx =
        TestAppContext::build_with_text_system(TestDispatcher::new(0), None, recorder.clone());
    let (service, initial) = crate::presentation::Service::start();
    cx.update(|cx| initial.install(cx));
    let handle = open(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        shell.follow_presentation(&service, window, cx);
        assert!(shell.filter_action("filter", window, cx));
    });
    draw(&mut cx, handle);
    (cx, recorder, service, handle)
}

fn wait(cx: &mut TestAppContext, handle: Handle, family: &str) {
    let until = Instant::now() + Duration::from_secs(6);
    while edit(cx, handle, |shell, _, _| {
        shell.active_typeface().font.family != family
    }) {
        assert!(Instant::now() < until, "service never published {family}");
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
    draw(cx, handle);
}

#[test]
fn font_gate_late_probe_restyles_already_shaped_filter_and_find() {
    if isolation::enter(
        "font_gate::service::font_gate_late_probe_restyles_already_shaped_filter_and_find",
        &format!("#!/bin/sh\nsleep {LATE_PROBE_SECONDS}\nprintf '%s' '{FAMILY}'\n"),
    ) {
        return;
    }
    let (mut cx, recorder, _service, handle) = mounted();
    assert_eq!(
        edit(&mut cx, handle, |shell, _, _| shell
            .active_typeface()
            .font
            .family
            .to_string()),
        FALLBACK
    );
    recorder.begin();
    wait(&mut cx, handle, FAMILY);
    assert!(assert_runs(
        "late-probe-filter",
        &recorder,
        &["file-0.txt", "esc clears"]
    ));
    edit(&mut cx, handle, |shell, window, cx| {
        assert!(shell.filter_action("search", window, cx));
        shell.search_update(
            0,
            None,
            |search, host| {
                for ch in "Late probe search".chars() {
                    search.key(crate::g2::sr1::StripKey::Text(ch.to_string()), host);
                }
            },
            window,
            cx,
        );
    });
    draw(&mut cx, handle);
    let root = crate::presentation::fixture::Fixture::open(
        std::env::var_os("FONT_GATE_ROOT").unwrap().into(),
    )
    .unwrap();
    let helper = root.path().join("bin/fc-match");
    root.write(&helper, format!("#!/bin/sh\nprintf '%s' '{SECOND}'\n"));
    root.executable(&helper);
    recorder.begin();
    root.write(
        &root.path().join("config/fontconfig/fonts.conf"),
        "changed font input",
    );
    wait(&mut cx, handle, SECOND);
    assert!(assert_family(
        "live-probe-find",
        &recorder,
        &["file-0.txt", "Late probe search"],
        SECOND
    ));
}

#[test]
fn font_gate_failed_probe_draws_named_fallback() {
    if isolation::enter(
        "font_gate::service::font_gate_failed_probe_draws_named_fallback",
        "#!/bin/sh\nexit 1\n",
    ) {
        return;
    }
    let (mut cx, recorder, _service, handle) = mounted();
    assert_eq!(
        edit(&mut cx, handle, |shell, _, _| shell
            .active_typeface()
            .font
            .family
            .to_string()),
        FALLBACK
    );
    // A launch probe that timed out is retried after the worker's 500 ms tick, so the fallback must outlast that retry.
    let until = Instant::now() + RETRY_WINDOW;
    while Instant::now() < until {
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        edit(&mut cx, handle, |shell, _, _| shell
            .active_typeface()
            .font
            .family
            .to_string()),
        FALLBACK,
        "a failed probe must not publish a later family"
    );
    // New text forces a run even when the first frame's layout was retained.
    recorder.begin();
    edit(&mut cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/failed-probe-font", window, cx)
    });
    draw(&mut cx, handle);
    assert!(assert_family(
        "failed-probe-dialog",
        &recorder,
        &["/failed-probe-font"],
        FALLBACK
    ));
}
