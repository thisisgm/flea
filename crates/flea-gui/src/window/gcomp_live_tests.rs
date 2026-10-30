//! OEM files reach the real subscription before backend deliveries mount each state surface.
use super::*;
use crate::presentation::{fixture::Fixture, Service};
use gpui::{px, VisualTestContext};

#[path = "gcomp_live_fixture.rs"]
mod mounted;
use mounted::{assert_rows, populate, wait_for_base};
use std::{
    process::{Command, Stdio},
    time::Duration,
};

const TEST: &str =
    "window::gcomp_tests::live::gcomp_oem_delivery_scales_received_empty_locked_and_copy_surfaces";

fn settle(cx: &mut TestAppContext, handle: Handle) {
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    frame(cx, handle);
}

fn assert_received_surfaces(
    cx: &mut TestAppContext,
    handle: Handle,
    base: u32,
    hero_mark: f32,
    lock_mark: f32,
    card_width: f32,
    listing: usize,
) {
    assert_rows(cx, handle, base);
    edit(cx, handle, |shell, _, _| {
        for host in shell.panes.iter().flatten() {
            assert_eq!(f32::from(host.list.borrow().typeface.base), base as f32);
        }
    });
    edit(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        assert!(pane.begin("/empty".into(), Default::default(), pane.now()));
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/empty".into(),
                total: 0,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                rows: vec![],
                listing,
            },
        ] {
            shell.receive(0, BackendEvent::Reply(reply), window, cx);
        }
    });
    settle(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let pane = &shell.panes[0].as_ref().unwrap().pane;
        assert_eq!(
            pane.state.kind,
            crate::pane::states::Kind::Empty,
            "base={base}, view={:?}, place={}, pending={}",
            shell.view,
            pane.place(),
            pane.pending(),
        );
    });
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    assert_eq!(
        visual.debug_bounds("fb1-hero-mark").unwrap().size.width,
        px(hero_mark)
    );
    // Every beat draws the caption size's 4/3 line: 10, 12 and 13 px captions at bases 12, 14 and 16.
    let caption_size = match base {
        12 => 10.,
        14 => 12.,
        _ => 13.,
    };
    let caption = visual.debug_bounds("fb1-hero-caption").unwrap().size.height;
    assert_eq!(
        caption,
        edit(cx, handle, |_, window, _| window
            .pixel_snap(px(caption_size * 4. / 3.)))
    );
    edit(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        assert!(pane.begin("/locked".into(), Default::default(), pane.now()));
        shell.receive(
            0,
            BackendEvent::Refused {
                operation: "scan".into(),
                path: "/locked".into(),
                message: "Permission denied".into(),
                mode: 0o40000,
            },
            window,
            cx,
        );
    });
    settle(cx, handle);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    assert_eq!(
        visual.debug_bounds("fb1-state-mark").unwrap().size.width,
        px(lock_mark)
    );
    assert_eq!(
        visual
            .debug_bounds("fb1-state-sentence")
            .unwrap()
            .size
            .height,
        edit(cx, handle, |_, window, _| window
            .pixel_snap(px(base as f32 * 1.2)))
    );
    edit(cx, handle, |shell, window, cx| {
        shell.open_overlay_copy_to(false, "/files", window, cx)
    });
    frame(cx, handle);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    assert_eq!(
        visual.debug_bounds("op1-card").unwrap().size.width,
        px(card_width)
    );
    edit(cx, handle, |shell, window, cx| {
        shell.close_overlay(window, cx)
    });
}

#[test]
fn gcomp_oem_delivery_scales_received_empty_locked_and_copy_surfaces() {
    let Ok(root) = std::env::var("GCOMP_FONT_FIXTURE") else {
        let fixture = Fixture::new("gcomp-font");
        let home = fixture.path().join("home");
        fixture.mkdir(&home);
        let output = Command::new("/usr/bin/setsid")
            .args([
                "-w",
                std::env::current_exe().unwrap().to_str().unwrap(),
                "--exact",
                TEST,
                "--nocapture",
                "--test-threads=1",
            ])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &home)
            .env("GCOMP_FONT_FIXTURE", fixture.path())
            .stdin(Stdio::null())
            .output()
            .unwrap();
        fixture.cleanup();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    let fixture = Fixture::open(root.into()).unwrap();
    let home = fixture.path().join("home");
    let font_file = home.join(".config/omarchy/shell.toml");
    fixture.write(&font_file, "[font]\nbase-size=12\n");
    let (service, initial) = Service::start();
    let mut cx = crate::list::bench::real_cx();
    cx.update(|cx| {
        initial.install(cx);
        cx.set_reduce_motion(true);
    });
    let handle = open(&mut cx);
    let (backend, _requests) = Backend::test_lines();
    edit(&mut cx, handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
        shell.launch.settings =
            crate::window::startup::Settings::from_json(flea::uistate::from_file("{}"));
        shell.set_view(View::List, window, cx);
        shell.follow_presentation(&service, window, cx);
        shell.follow_settings(window, cx);
    });
    for (base, hero_mark, lock_mark, card_width) in [
        (12, 66., 34., 473.),
        (14, 66., 40., 552.),
        (16, 66., 47., 630.),
    ] {
        populate(&mut cx, handle, base as usize + 100);
        fixture.write(&font_file, format!("[font]\nbase-size={base}\n"));
        wait_for_base(&mut cx, handle, base, "OEM");
        assert_received_surfaces(
            &mut cx,
            handle,
            base,
            hero_mark,
            lock_mark,
            card_width,
            base as usize + 101,
        );
    }
    for (base, hero_mark, lock_mark, card_width) in [
        (12, 66., 34., 473.),
        (14, 66., 40., 552.),
        (16, 66., 47., 630.),
    ] {
        populate(&mut cx, handle, base as usize + 200);
        let document = edit(&mut cx, handle, |shell, _, _| {
            let patch = flea::jsondoc::parse(&format!(
                r#"{{"display":{{"textSize":{{"mode":{base}}}}}}}"#
            ))
            .unwrap();
            flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap()
        });
        fixture.write(
            &home.join(".local/state/flea/ui.json"),
            flea::jsondoc::render(&document),
        );
        wait_for_base(&mut cx, handle, base, "settings textSize");
        assert_received_surfaces(
            &mut cx,
            handle,
            base,
            hero_mark,
            lock_mark,
            card_width,
            base as usize + 201,
        );
        assert_eq!(call(&mut cx, handle, "uiSettings").code, 0);
    }
}
