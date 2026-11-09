//! The flash view covers its pane's slot as Trash does: the rail, the status bar and its count stay drawn.
use super::*;

/// Flash opened on the left pane over an image, a fitting drive and a too-small one.
fn opened(cx: &mut TestAppContext, view: View) -> Handle {
    // Catppuccin Latte's colours, so the accent and muted frames differ in the painted scene.
    cx.update(|cx| {
        cx.set_global(crate::palette::Theme::from_text(
            "background='#eff1f5'\nforeground='#4c4f69'\naccent='#1e66f5'\nmuted='#9ca0b0'",
        ))
    });
    let (handle, mut requests, _) = start(cx);
    edit(cx, handle, |shell, _, cx| {
        shell.view = view;
        shell.launch.settings.rail_width = 192.;
        shell.mount_rail(cx);
    });
    frame(cx, handle);
    choose(cx, handle, 44, "flashDrive", "", &["/files/disk.img"]);
    drain(&mut requests);
    receive(
        cx,
        handle,
        0,
        r#"{"t":"flashimage","path":"/files/disk.img","kind":"img","bytes":4294967296,"unpacked":4294967296,"known":true,"member":""}"#,
    );
    receive(
        cx,
        handle,
        0,
        r#"{"t":"flashtargets","drives":[{"dev":"sda","seq":2,"serial":"S1","label":"128GB","model":"General USB Flash Disk","bytes":124623257600,"volumes":[],"refused":""},{"dev":"sdb","seq":3,"serial":"S2","label":"SanDisk","model":"SanDisk Cruzer Blade","bytes":2000000000,"volumes":[],"refused":"small"}]}"#,
    );
    frame(cx, handle);
    handle
}

#[test]
fn flash_covers_the_pane_slot_and_keeps_the_rail_and_the_status_bar() {
    let mut cx = crate::list::bench::real_cx();
    let handle = opened(&mut cx, View::List);
    let geometry = edit(&mut cx, handle, |shell, window, _| shell.geometry(window));
    // The window chrome gives its row to the flash strip; the rail runs from the top down to the status bar.
    assert_eq!(geometry.chrome.height, 0.);
    assert_eq!((geometry.rail.y, geometry.rail.width), (0., 192.));
    assert_eq!(geometry.rail.height, geometry.status.y);
    assert!(geometry.status.height > 0., "the status bar stays drawn");
    let drawn = VisualTestContext::from_window(handle.into(), &cx)
        .debug_bounds("op2-flash")
        .expect("the flash view is drawn");
    let slot = geometry.panes[0];
    assert_eq!(
        (
            f32::from(drawn.origin.x),
            f32::from(drawn.origin.y),
            f32::from(drawn.size.width),
            f32::from(drawn.size.height),
        ),
        (slot.x, 0., slot.width, geometry.status.y),
        "the flash view fills the pane's slot and nothing else"
    );
    let status = footer(&mut cx, handle);
    assert!(status.contains("2 drives"), "{status}");
    // A rail place leaves Flash, as Pane.qml open does.
    edit(&mut cx, handle, |shell, window, cx| {
        let _ = shell.step(
            0,
            crate::window::navigate::Step::OpenPlace("/right".into()),
            window,
            cx,
        );
        assert!(
            !shell.operations.archive.flash.shown(),
            "a rail place closes Flash"
        );
    });
}

/// The flash cursor (0 image, 1 first drive, 9 the step) and the window's focused stop.
fn seat(cx: &mut TestAppContext, handle: Handle) -> (usize, &'static str) {
    edit(cx, handle, |shell, window, cx| {
        let view = shell.operations.archive.flash.view(&shell.locale.catalog);
        let at = if view.image.cursor {
            0
        } else if view.step.cursor {
            9
        } else {
            1
        };
        (at, shell.focus_view(window, cx))
    })
}

/// The listing cursor of one pane.
fn cursor(cx: &mut TestAppContext, handle: Handle, side: usize) -> usize {
    edit(cx, handle, |shell, _, _| {
        shell.panes[side].as_ref().unwrap().pane.cursor
    })
}

/// A real left click on the flash pane's empty foot.
fn press_flash(cx: &mut TestAppContext, handle: Handle) {
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let pane = visual
        .debug_bounds("op2-flash")
        .expect("the flash view is drawn");
    let at = gpui::point(
        pane.origin.x + pane.size.width / 2.,
        pane.origin.y + pane.size.height - gpui::px(20.),
    );
    visual.simulate_click(at, gpui::Modifiers::none());
    frame(cx, handle);
}

/// The painted ground of the flash cursor edge, with the accent and muted frames it chooses between.
fn edge_ink(
    cx: &mut TestAppContext,
    handle: Handle,
) -> (Option<gpui::Hsla>, gpui::Hsla, gpui::Hsla) {
    let edge = VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds(crate::g2::archive_flash::flash::SEL_CURSOR_EDGE)
        .expect("the cursor edge is drawn");
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scale = window.scale_factor();
        let theme = *cx.global::<crate::palette::Theme>();
        let palette = crate::list::Palette::inherited(&window.text_style(), theme);
        let ink = window
            .painted_quads()
            .iter()
            .find(|q| {
                (q.bounds.origin.x.as_f32() - f32::from(edge.origin.x) * scale).abs() <= 1.
                    && (q.bounds.origin.y.as_f32() - f32::from(edge.origin.y) * scale).abs() <= 1.
                    && (q.bounds.size.width.as_f32() - f32::from(edge.size.width) * scale).abs()
                        <= 1.
                    && q.background.as_solid().is_some_and(|c| c.a > 0.)
            })
            .and_then(|q| q.background.as_solid());
        (ink, palette.accent_frame, palette.muted_frame)
    })
    .unwrap()
}

#[test]
fn tab_hands_the_keys_to_the_rail_and_a_press_on_flash_takes_them_back() {
    let mut cx = crate::list::bench::real_cx();
    let handle = opened(&mut cx, View::List);
    assert_eq!(
        seat(&mut cx, handle),
        (1, "list"),
        "Flash opens holding the keys"
    );
    let (edge, accent, muted) = edge_ink(&mut cx, handle);
    assert_ne!(accent, muted);
    assert_eq!(edge, Some(accent));
    // FlashView.qml focusNext asks FlashHost for the rail; the rail's own keys then leave the flash cursor alone.
    keys(&mut cx, handle, "tab");
    assert_eq!(seat(&mut cx, handle), (1, "rail"));
    assert!(
        !edit(&mut cx, handle, |shell, window, cx| shell
            .flash_has_keys(window, cx)),
        "the flash layer passes the rail's keys on"
    );
    assert_eq!(
        edge_ink(&mut cx, handle).0,
        Some(muted),
        "FlashDriveRows.qml:55: mutedFrame off focus"
    );
    keys(&mut cx, handle, "up");
    assert_eq!(
        seat(&mut cx, handle).0,
        1,
        "the rail's Up never moves the flash cursor"
    );
    press_flash(&mut cx, handle);
    assert_eq!(seat(&mut cx, handle), (1, "list"));
    assert_eq!(
        edge_ink(&mut cx, handle).0,
        Some(accent),
        "the cursor edge is accent again"
    );
    keys(&mut cx, handle, "up");
    assert_eq!(seat(&mut cx, handle).0, 0, "Flash has the keys back");
}

#[test]
fn the_other_dual_pane_keeps_its_keys_while_flash_covers_this_one() {
    let mut cx = crate::list::bench::real_cx();
    let handle = opened(&mut cx, View::Dual);
    let (edge, accent, muted) = edge_ink(&mut cx, handle);
    assert_eq!(edge, Some(accent));
    edit(&mut cx, handle, |shell, _, _| shell.focus_side = 1);
    let before = cursor(&mut cx, handle, 1);
    keys(&mut cx, handle, "down");
    assert_eq!(
        cursor(&mut cx, handle, 1),
        before + 1,
        "the right pane takes its own Down"
    );
    assert_eq!(
        seat(&mut cx, handle).0,
        1,
        "the right pane's Down never moves the flash cursor"
    );
    assert_eq!(
        edge_ink(&mut cx, handle).0,
        Some(muted),
        "focus on the right pane leaves Flash unfocused"
    );
}

#[test]
fn a_flash_on_the_second_pane_lets_go_of_the_keys_while_one_pane_shows() {
    let mut cx = crate::list::bench::real_cx();
    let handle = opened(&mut cx, View::Dual);
    // FlashHost.qml rides its pane: the second pane's flash hides with it and comes back with Dual.
    edit(&mut cx, handle, |shell, window, cx| {
        shell.operations.archive.flash_side = 1;
        shell.set_view(View::List, window, cx);
        assert!(shell.view == View::List && shell.focus_side == 0);
    });
    frame(&mut cx, handle);
    let before = cursor(&mut cx, handle, 0);
    keys(&mut cx, handle, "down");
    assert_eq!(
        seat(&mut cx, handle).0,
        1,
        "the unseen flash never takes the Down"
    );
    assert_eq!(
        cursor(&mut cx, handle, 0),
        before + 1,
        "the left pane takes its own Down"
    );
    assert!(edit(&mut cx, handle, |shell, _, _| shell
        .operations
        .archive
        .flash
        .shown()));
    assert!(
        VisualTestContext::from_window(handle.into(), &cx)
            .debug_bounds("op2-flash")
            .is_none(),
        "nothing draws the flash while its pane is out of view"
    );
    edit(&mut cx, handle, |shell, _, _| {
        shell.view = View::Dual;
        shell.focus_side = 1;
    });
    frame(&mut cx, handle);
    assert!(
        VisualTestContext::from_window(handle.into(), &cx)
            .debug_bounds("op2-flash")
            .is_some(),
        "the flash returns with its pane"
    );
}

#[test]
fn back_and_up_leave_flash_and_stop_while_forward_is_refused() {
    use crate::window::navigate::Step;
    for stop in [Step::Back, Step::MouseBack, Step::Up] {
        let mut cx = crate::list::bench::real_cx();
        let handle = opened(&mut cx, View::List);
        edit(&mut cx, handle, |shell, window, cx| {
            assert!(matches!(
                shell.covered_step(0, &Step::Forward, window, cx),
                Some(Ok(false))
            ));
            assert!(
                shell.operations.archive.flash.shown(),
                "Forward leaves Flash open"
            );
            // Pane.qml goBack and openParent: the step ends on the folder under Flash.
            assert!(matches!(
                shell.covered_step(0, &stop, window, cx),
                Some(Ok(true))
            ));
            assert!(!shell.operations.archive.flash.shown());
        });
    }
}

#[test]
fn a_place_and_the_strip_back_leave_flash() {
    use crate::window::navigate::Step;
    let mut cx = crate::list::bench::real_cx();
    let handle = opened(&mut cx, View::List);
    edit(&mut cx, handle, |shell, window, cx| {
        assert!(
            shell
                .covered_step(0, &Step::OpenPlace("/right".into()), window, cx)
                .is_none(),
            "a place leaves Flash and goes on to the folder"
        );
        assert!(!shell.operations.archive.flash.shown());
    });
    let mut cx = crate::list::bench::real_cx();
    let handle = opened(&mut cx, View::List);
    let mut visual = VisualTestContext::from_window(handle.into(), &cx);
    let back = visual
        .debug_bounds("op2-flash-back")
        .expect("the strip's Back is drawn");
    visual.simulate_click(back.center(), gpui::Modifiers::none());
    frame(&mut cx, handle);
    assert!(
        !edit(&mut cx, handle, |shell, _, _| shell
            .operations
            .archive
            .flash
            .shown()),
        "the strip's Back leaves Flash"
    );
}
