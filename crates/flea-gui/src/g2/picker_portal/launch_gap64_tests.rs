//! Real event-loop refusal stays bounded and cannot publish a picker result.
use super::{finish_opening, run, PickLaunch};
use crate::window::locale::Service;
use std::os::unix::fs::PermissionsExt;
use std::{cell::Cell, process::Command, rc::Rc, time::Duration};

#[test]
fn sol_gap64_picker_open_refusal_quits_its_running_loop_without_a_reply() {
    const TEST: &str = "g2::picker_portal::launch::gap64_tests::sol_gap64_picker_open_refusal_quits_its_running_loop_without_a_reply";
    const CHILD: &str = "FLEA_GAP64_PICKER_LOOP_CHILD";
    if let Ok(arm) = std::env::var(CHILD) {
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(5));
            std::process::exit(77);
        });
        if arm == "failure" {
            let reply =
                std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join("reply.json");
            let launch = PickLaunch::resolve(Some("{}"), reply.to_str().unwrap(), None).unwrap();
            let (service, locale) = Service::launch(crate::test_catalogs::english_paths()).unwrap();
            let code = run(
                launch,
                service,
                locale,
                gpui_platform::application().with_windowing(gpui::WindowingRequest::Headless),
                |_, _, _| Err("gap64-picker device=diagnostic-test".into()),
            );
            assert!(
                !reply.exists(),
                "the production launch cannot publish a reply on refusal"
            );
            std::process::exit(code);
        }
        let failed = Rc::new(Cell::new(false));
        let failure = Rc::clone(&failed);
        gpui_platform::application()
            .with_windowing(gpui::WindowingRequest::Headless)
            .run(move |cx| {
                cx.spawn(async move |cx| {
                    cx.update(|cx| {
                        let catalog = crate::test_catalogs::english();
                        let opened = if arm == "success" {
                            Ok("window")
                        } else {
                            Err("gap64-picker device=diagnostic-test".into())
                        };
                        match finish_opening(opened, &catalog, cx) {
                            Ok(window) => {
                                assert_eq!(window, "window");
                                cx.quit();
                            }
                            Err(()) => failure.set(true),
                        }
                    });
                })
                .detach();
            });
        std::process::exit(i32::from(failed.get()));
    }
    let root = std::env::temp_dir().join(format!("flea-gap64-picker-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join(".flea-test-sandbox"), "").unwrap();
    // A stand-in notify-send records the portal launch's desktop notice, so a test run never shows a real one.
    let notices = root.join("notice.log");
    std::fs::create_dir(root.join("bin")).unwrap();
    let stand_in = root.join("bin/notify-send");
    std::fs::write(
        &stand_in,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\n",
            notices.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", TEST, "--nocapture"])
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", root.join("bin").display()),
        )
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("HYPRLAND_INSTANCE_SIGNATURE");
    for (key, name) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ] {
        let directory = root.join(name);
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        command.env(key, directory);
    }
    for (arm, expected) in [("success", 0), ("failure", 1)] {
        let output = command.env(CHILD, arm).output().unwrap();
        assert_eq!(output.status.code(), Some(expected), "{output:?}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            error.matches("gap64-picker device=diagnostic-test").count(),
            usize::from(arm == "failure"),
            "{error}"
        );
        assert!(!root.join("home/reply.json").exists());
        let notice = std::fs::read_to_string(&notices).unwrap_or_default();
        let wanted_notice = if arm == "failure" {
            "--app-name=Flea\n--urgency=critical\n--icon=com.thisisgm.flea\nNo graphics driver could open Flea\n\
             Install or repair vulkan-swrast and mesa, or run flea --tui in a terminal.\n"
        } else {
            ""
        };
        assert_eq!(notice, wanted_notice, "{arm}: {error}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
