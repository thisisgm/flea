// FIX-PICKER-2A: chooser keys, rail and scroll bar, each held to what the v0.3.12 chooser does in the same steps.
use super::ey313_tests::draw;
use super::pointer_tests::{grid, notch, seed};
use super::tests::fixture::{fixture, Fixture};
use gpui::{KeyUpEvent, Keystroke, Modifiers, MouseButton, TestAppContext};

// A real key is pressed and released; gpui's keyboard click fires on the release of Enter or Space.
fn press(f: &mut Fixture, key: &str) {
    f.cx.simulate_keystrokes(key);
    f.cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).unwrap(),
    });
    f.cx.run_until_parked();
}

// EY-355: PickerWindow.qml's Parent folder button goes up one folder per press, Space or Enter alike.
#[gpui::test]
fn fix_picker_2a_ey355_space_on_parent_folder_goes_up_one_folder(cx: &mut TestAppContext) {
    for key in ["space", "enter"] {
        let mut f = fixture(cx, r#"{"folder":"/d/sub/deep"}"#, None);
        f.cx.run_until_parked();
        let up = f.read(|picker| picker.chrome.up.clone());
        f.cx.update(|window, cx| window.focus(&up, cx));
        f.cx.run_until_parked();
        press(&mut f, key);
        assert_eq!(
            f.read(|picker| picker.listing.location.clone()),
            "/d/sub",
            "{key} on Parent folder goes one folder up"
        );
    }
}

// Back is the other focusable chrome mark, so one Space or Enter on it steps back one folder too.
#[gpui::test]
fn fix_picker_2a_ey355_space_on_back_steps_back_one_folder(cx: &mut TestAppContext) {
    for key in ["space", "enter"] {
        let mut f = fixture(cx, r#"{"folder":"/d/sub/deep"}"#, None);
        super::side_button_tests::landed(&mut f, "/d/sub/deep");
        f.update(|picker, cx| {
            picker.chrome.history = vec!["/a".into(), "/a/b".into()];
            cx.notify();
        });
        f.cx.run_until_parked();
        let back = f.read(|picker| picker.chrome.back.clone());
        f.cx.update(|window, cx| window.focus(&back, cx));
        f.cx.run_until_parked();
        press(&mut f, key);
        assert_eq!(
            f.read(|picker| picker.listing.location.clone()),
            "/a/b",
            "{key} on Back steps back one folder"
        );
    }
}

// EY-543: keys.toml binds ":" to pathBar, so the location field opens again after Escape closed it.
#[gpui::test]
fn fix_picker_2a_ey543_colon_after_escape_reopens_the_location_field(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    seed(&mut f);
    draw(&mut f);
    let files = f.read(|picker| picker.focus.clone());
    f.cx.update(|window, cx| window.focus(&files, cx));
    f.cx.run_until_parked();
    let path = f.update(|picker, cx| picker.browse.path_view(cx));
    f.cx.simulate_keystrokes("ctrl-l");
    f.cx.run_until_parked();
    assert!(f.cx.update(|_, cx| path.read(cx).editing()), "Ctrl+L opens");
    press(&mut f, "escape");
    assert!(
        !f.cx.update(|_, cx| path.read(cx).editing()),
        "Escape closes"
    );
    press(&mut f, "shift-;->:");
    assert!(
        f.cx.update(|_, cx| path.read(cx).editing()),
        "a colon after Escape opens the location field again"
    );
}

// EY-377: the wheel shows PickerList's scroll bar in the right lane, and a press in that lane moves the list.
#[gpui::test]
fn fix_picker_2a_ey377_wheel_shows_the_list_scroll_bar(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    seed(&mut f);
    draw(&mut f);
    assert!(
        !f.update(|picker, cx| picker.browse.listing.read(cx).bar_shown()),
        "no bar before the list moves"
    );
    for _ in 0..4 {
        notch(&mut f, -3., false);
    }
    draw(&mut f);
    assert!(
        f.update(|picker, cx| picker.browse.listing.read(cx).bar_shown()),
        "the wheel shows the bar"
    );
}

// PickerGrid carries the same bar over its tiles.
#[gpui::test]
fn fix_picker_2a_ey377_wheel_shows_the_grid_scroll_bar(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    draw(&mut f);
    assert_eq!(
        f.update(|picker, cx| picker.browse.listing.read(cx).view_mode()),
        "grid"
    );
    assert!(
        !f.update(|picker, cx| picker.browse.listing.read(cx).bar_shown()),
        "no bar before the grid moves"
    );
    for _ in 0..4 {
        notch(&mut f, -3., false);
    }
    draw(&mut f);
    assert!(
        f.update(|picker, cx| picker.browse.listing.read(cx).scroll_y()) > 0.,
        "the wheel moved the grid"
    );
    assert!(
        f.update(|picker, cx| picker.browse.listing.read(cx).bar_shown()),
        "the wheel shows the grid bar"
    );
}

#[gpui::test]
fn fix_picker_2a_ey377_a_press_in_the_bar_lane_moves_the_list(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    seed(&mut f);
    draw(&mut f);
    let asked = f.listing.sent().len();
    let body = f.cx.debug_bounds("pk2-listing").unwrap();
    let at = gpui::point(body.right() - gpui::px(2.), body.bottom() - gpui::px(4.));
    f.cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    f.cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    f.cx.run_until_parked();
    // A row click could only reveal its own partly drawn row; the lane jumps the knob toward the end of 1000 rows.
    let (scroll, row, cursor) = f.update(|picker, cx| {
        let view = picker.browse.listing.read(cx);
        (
            view.scroll_y(),
            view.drawn().file_metrics().row,
            view.listing().cursor(),
        )
    });
    assert!(
        scroll > 500. * row,
        "a press low in the lane scrolls far down the list, not one row"
    );
    assert_eq!(cursor, 0, "the lane press never moves the cursor");
    assert!(
        f.listing.sent()[asked..]
            .iter()
            .any(|line| line.contains(r#""c":"window""#)),
        "the rows the lane press shows are asked for"
    );
}

// A tile reveal moves at most one body height; the grid lane jumps toward the end of 1000 tiles.
#[gpui::test]
fn fix_picker_2a_ey377_a_press_in_the_bar_lane_moves_the_grid(cx: &mut TestAppContext) {
    let mut f = fixture(cx, r#"{"folder":"/d/sub"}"#, None);
    grid(&mut f);
    draw(&mut f);
    let asked = f.listing.sent().len();
    let body = f.cx.debug_bounds("pk2-listing").unwrap();
    let at = gpui::point(body.right() - gpui::px(2.), body.bottom() - gpui::px(4.));
    f.cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    f.cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    f.cx.run_until_parked();
    let (mode, scroll, cursor) = f.update(|picker, cx| {
        let view = picker.browse.listing.read(cx);
        (view.view_mode(), view.scroll_y(), view.listing().cursor())
    });
    assert_eq!(mode, "grid", "the press lands on the grid");
    assert!(
        scroll > 2. * f32::from(body.size.height),
        "a press low in the lane scrolls far down the grid, not one tile"
    );
    assert_eq!(cursor, 0, "the lane press never moves the cursor");
    // Only the grid branch asks for tiles in grid mode; a lane press taking the list branch sends nothing.
    assert!(
        f.listing.sent()[asked..]
            .iter()
            .any(|line| line.contains(r#""c":"window""#)),
        "the tiles the lane press shows are asked for"
    );
}

// EY-506: PickerPlaces lists the gvfs share and a kernel share, never the gvfs FUSE root the share sits under.
#[test]
fn fix_picker_2a_ey506_the_rail_lists_no_gvfs_root() {
    let gio = "Mount(0): data on nas -> smb://nas/data/\n";
    let mountinfo = concat!(
        "36 25 0:45 / /home/u/.r/gvfs rw,nosuid,nodev,relatime shared:1 - fuse.gvfsd-fuse gvfsd-fuse rw,user_id=1000\n",
        "37 25 0:46 / /mnt/nas rw,relatime shared:2 - cifs //nas/data rw\n",
    );
    let rows: Vec<_> = super::network::share_rows(gio, "", mountinfo)
        .into_iter()
        .flat_map(|section| section.rows)
        .collect();
    assert!(
        rows.iter()
            .any(|row| row.uri.starts_with("smb://nas/data") && row.path != "/mnt/nas"),
        "the gvfs share stays as its own row"
    );
    assert!(
        rows.iter().any(|row| row.path == "/mnt/nas"),
        "the kernel share stays"
    );
    assert!(
        rows.iter()
            .all(|row| row.path != "/home/u/.r/gvfs" && row.label != "gvfs"),
        "the gvfs root does not"
    );
    assert_eq!(rows.len(), 2);
}
