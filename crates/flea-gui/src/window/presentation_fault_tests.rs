//! Launch and live-change faults of the presentation inputs: the window still opens, pages at the painted row and follows any parsed theme.
use crate::{
    backend::Backend,
    palette::Theme,
    presentation::{
        fixture::{Fixture, SHELL_GUARD},
        Fonts, Snapshot,
    },
    state::tests::shell_with,
};
use gpui::{AppContext, TestAppContext};
use std::{
    path::Path,
    process::{Command, Stdio},
};

const LAUNCH: &str =
    "window::presentation_fault_tests::u01_launch_survives_font_probe_and_input_faults";
const IDLE: &str =
    "window::presentation_fault_tests::u01_unchanged_inputs_never_respawn_the_font_probe";

fn helper(fixture: &Fixture, dir: &Path, script: &str) {
    fixture.mkdir(dir);
    let path = dir.join("fc-match");
    fixture.write(&path, script);
    fixture.executable(&path);
}

fn child(test: &str, env: &[(&str, &str)]) -> (bool, String) {
    let mut command = Command::new("/usr/bin/setsid");
    command
        .arg("-w")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env_clear()
        .stdin(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().unwrap();
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr),
    )
}

#[test]
fn u01_launch_survives_font_probe_and_input_faults() {
    if let Ok(case) = std::env::var("U01_FAULT_CASE") {
        let fixture = Fixture::open(std::env::var_os("U01_FIXTURE_ROOT").unwrap().into()).unwrap();
        fixture
            .check(Path::new(&std::env::var_os("HOME").unwrap()))
            .unwrap();
        let (_service, snapshot) = crate::presentation::Service::start();
        println!("U01-FAMILY {case} {}", snapshot.fonts.family);
        return;
    }
    let fixture = Fixture::new("launch");
    let base = fixture.path();
    let plain = base.join("plain");
    fixture.mkdir(&plain);
    let odd = base.join("odd");
    fixture.mkdir(&odd.join(".config/omarchy/shell.toml"));
    let huge = base.join("huge");
    fixture.write(&huge.join(".config/omarchy/shell.toml"), vec![b'#'; 70_000]);
    let slow = base.join("slow");
    helper(&fixture, &slow, "#!/bin/sh\nsleep 1\nprintf 'Slow Mono'\n");
    let slow_path = format!("{}:/usr/bin:/bin", slow.display());
    let mut refused = Vec::new();
    for (case, home, path, family) in [
        ("no-font-helper", &plain, "/nonexistent", Some("monospace")),
        (
            "slow-font-helper",
            &plain,
            slow_path.as_str(),
            Some("monospace"),
        ),
        ("directory-input", &odd, "/usr/bin:/bin", None),
        ("oversized-input", &huge, "/usr/bin:/bin", None),
    ] {
        let (ok, text) = child(
            LAUNCH,
            &[
                ("U01_FAULT_CASE", case),
                ("U01_FIXTURE_ROOT", base.to_str().unwrap()),
                ("HOME", home.to_str().unwrap()),
                ("PATH", path),
            ],
        );
        let wanted = family.map(|family| format!("U01-FAMILY {case} {family}"));
        if !ok || wanted.is_some_and(|line| !text.contains(&line)) {
            refused.push((case, text));
        }
    }
    fixture.cleanup();
    assert!(
        refused.is_empty(),
        "launch refused or wrong family: {refused:?}"
    );
}

#[test]
fn u01_unchanged_inputs_never_respawn_the_font_probe() {
    if let Ok(home) = std::env::var("U01_IDLE_HOME") {
        let fixture = Fixture::open(std::env::var_os("U01_FIXTURE_ROOT").unwrap().into()).unwrap();
        let counter = Path::new(&home).join("count");
        let spawned = || {
            std::fs::read_to_string(&counter)
                .unwrap_or_default()
                .lines()
                .count()
        };
        let _started = crate::presentation::Service::start();
        let idle = std::time::Duration::from_millis(1600);
        // The launch probe is the only one while three polls see unchanged inputs.
        std::thread::sleep(idle);
        assert_eq!(spawned(), 1, "an idle poll respawned the probe");
        let fonts = Path::new(&home).join(".config/fontconfig/fonts.conf");
        fixture.write(&fonts, "<fontconfig/>");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while spawned() < 2 && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        // The changed input costs exactly one probe, which fails here and is not retried for the same bytes.
        std::thread::sleep(idle);
        assert_eq!(
            spawned(),
            2,
            "the changed input must probe once and no more"
        );
        return;
    }
    let fixture = Fixture::new("idle");
    let base = fixture.path();
    let counter = base.join("count");
    let bin = base.join("bin");
    helper(
        &fixture,
        &bin,
        &format!("#!/bin/sh\n{SHELL_GUARD}\nguard \"$HOME\" \"$HOME/count\" \"$U01_FIXTURE_PARENT\"\necho x >> \"$HOME/count\"\nexit 1\n"),
    );
    let (ok, text) = child(
        IDLE,
        &[
            ("U01_IDLE_HOME", base.to_str().unwrap()),
            ("U01_FIXTURE_ROOT", base.to_str().unwrap()),
            ("U01_FIXTURE_PARENT", Fixture::parent().to_str().unwrap()),
            ("HOME", base.to_str().unwrap()),
            ("PATH", &format!("{}:/usr/bin:/bin", bin.display())),
        ],
    );
    let spawned = std::fs::read_to_string(&counter)
        .unwrap_or_default()
        .lines()
        .count();
    fixture.cleanup();
    assert!(ok, "{text}");
    assert_eq!(spawned, 2, "{text}");
}

#[gpui::test]
fn u01_scene_reload_pages_at_the_painted_row(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| super::root::Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"display":{"textSize":{"mode":16}}}"#).unwrap();
            shell
        }),
    });
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let spec = crate::visual::contract::Spec::parse(
        &std::process::id().to_string(),
        "g1-01-list-r0",
        r#"{"path":"/other"}"#,
    )
    .unwrap();
    let (answer, _receive) = std::sync::mpsc::sync_channel(1);
    handle
        .update(cx, |root, window, cx| {
            root.shell
                .update(cx, |shell, cx| shell.apply_scene(spec, answer, window, cx));
        })
        .unwrap();
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let host = shell.panes[0].as_ref().unwrap();
            let painted = crate::list::Metrics::for_typeface(&host.list.borrow().typeface).row;
            assert_eq!(host.pane.row_height(), painted);
        })
        .unwrap();
}

#[test]
fn u01_header_element_height_matches_the_slot_for_a_body_override() {
    let face = Fonts::from_sources("[font]\nbase-size=14\nbody=18", "", "monospace".into())
        .typeface(&flea::jsondoc::Json::Null);
    assert_eq!(
        crate::list::header::element_height(&face),
        crate::list::Metrics::for_typeface(&face).header
    );
}

#[gpui::test]
fn u01_second_window_repaints_a_live_snapshot(cx: &mut TestAppContext) {
    let mut open = || {
        let (backend, _) = Backend::test_channel();
        cx.add_window(|_, cx| super::root::Root {
            shell: cx.new(|cx| shell_with(backend, cx)),
        })
    };
    let windows = [open(), open()];
    for handle in windows {
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
    }
    let (theme, parsed) = Theme::from_text_observed("foreground='#cacccc'\naccent='#654321'");
    for handle in windows {
        handle
            .update(cx, |root, window, cx| {
                root.shell.update(cx, |shell, cx| {
                    let fonts = Fonts::from_sources(
                        "[font]\nbase-size=16",
                        "",
                        shell.active_typeface().font.family.to_string(),
                    );
                    shell.apply_presentation(
                        Snapshot {
                            theme,
                            parsed,
                            fonts,
                        },
                        window,
                        cx,
                    );
                });
            })
            .unwrap();
    }
    for handle in windows {
        cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
    }
    windows[1]
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            let host = shell.panes[0].as_ref().unwrap();
            let list = host.list.borrow();
            assert_eq!(list.drawn.palette.map(|p| p.accent), Some(theme.accent));
            assert_eq!(list.drawn.metrics.row, host.pane.row_height());
        })
        .unwrap();
}

#[test]
fn u01_theme_loaded_follows_any_parsed_colour_and_the_frame_the_qml_surface() {
    // ui/js/Palette.js:31-33: a palette without the six keys is still a palette.
    let (_, parsed) = Theme::from_text_observed("cursor='#ffffff'\ncolor1='#ff0000'");
    assert!(parsed.0);
    assert!(!Theme::from_text_observed("").1 .0);
    // ui/js/Palette.js:18 lifts against dark_background, background or selection, else ui/Theme.qml:46's #181825, never color0.
    let theme = Theme::from_text("color0='#f0f0f0'\naccent='#c0c0c0'");
    assert_eq!(theme.accent_frame, theme.accent);
    let light = Theme::from_text("background='#f0f0f0'\naccent='#c0c0c0'");
    assert_ne!(light.accent_frame, light.accent);
}
