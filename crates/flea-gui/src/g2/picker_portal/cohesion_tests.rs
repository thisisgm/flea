//! CTL2's picker-open review: real crumb geometry, quiet footer and caller dropdown.
use super::{process::Line, tests::fixture::fixture};
use gpui::{px, size, Modifiers, TestAppContext};

#[gpui::test]
fn picker_toolbar_gives_the_crumb_path_its_full_height(cx: &mut TestAppContext) {
    let f = fixture(cx, r#"{"folder":"/fx/v/work"}"#, None);
    f.listing.say(Line::Reply(
        r#"{"t":"listed","n":0,"path":"/fx/v/work"}"#.into(),
    ));
    f.cx.run_until_parked();
    let picker = f.picker.clone();
    f.cx.update(|_, cx| {
        let path = picker.read(cx).browse.path_view(cx);
        let path = path.read(cx);
        let paint = path.paint.borrow();
        let bounds = paint.area.expect("the shared path strip was laid out");
        let height = f32::from(bounds.size.height);
        assert!(
            (26. ..=27.).contains(&height),
            "crumbs fill the 27 px toolbar above its 1 px rule, got {height}"
        );
        assert!(!paint.runs[0].is_empty(), "the current folder is shaped");
        assert_eq!(path.inputs.paths[0], "/fx/v/work");
    });
}

#[gpui::test]
fn picker_footer_draws_only_the_right_hint_when_quiet(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    f.cx.run_until_parked();
    assert!(
        f.cx.debug_bounds("picker-selection-status").is_none(),
        "picker-open leaves the footer's left side empty"
    );
    assert_eq!(
        f.read(|picker| picker.hints()).as_ref(),
        "enter open · esc cancel"
    );
    drop(f);
    let f = fixture(cx, r#"{"multiple":true}"#, None);
    f.cx.run_until_parked();
    assert!(
        f.cx.debug_bounds("picker-selection-status").is_some(),
        "a chooser that marks says what is checked, as v0.3.12 does"
    );
}

#[gpui::test]
fn picker_caller_filter_has_dropdown_width_and_chevron(cx: &mut TestAppContext) {
    let f = fixture(
        cx,
        r#"{"folder":"/fx/v/work","filters":[{"label":"Images","globs":["*.png"]}]}"#,
        None,
    );
    f.cx.simulate_resize(size(px(1100.), px(700.)));
    f.cx.run_until_parked();
    let trigger =
        f.cx.debug_bounds("picker-filter")
            .expect("caller filter trigger");
    assert_eq!(
        f32::from(trigger.size.width),
        257.,
        "the dropdown shares v0.3.12's menu width"
    );
    let chevron =
        f.cx.debug_bounds("picker-filter-chevron")
            .expect("the trigger draws a chevron");
    assert_eq!(
        (
            f32::from(chevron.size.width),
            f32::from(chevron.size.height)
        ),
        (16., 16.)
    );
    // The slot alone stayed green while it painted nothing; the glyph itself must resolve and lay out.
    assert!(crate::g2::settings_pages::glyphs::svg("chevron-down").is_some());
    let glyph =
        f.cx.debug_bounds("picker-filter-chevron-glyph")
            .expect("the chevron svg is painted inside its slot");
    assert_eq!(
        (f32::from(glyph.size.width), f32::from(glyph.size.height)),
        (16., 16.)
    );
    assert!(trigger.contains(&chevron.center()));
    assert_eq!(trigger.right() - chevron.right(), px(9.));
}

#[gpui::test]
fn picker_caller_filter_frame_follows_the_hyprland_corner(cx: &mut TestAppContext) {
    let f = fixture(
        cx,
        r#"{"folder":"/fx/v/work","filters":[{"label":"Images","globs":["*.png"]}]}"#,
        None,
    );
    f.cx.simulate_resize(size(px(1100.), px(700.)));
    for radius in [0., 8., 13.] {
        f.cx.update(|window, cx| {
            cx.set_global(crate::compositor::Corner(radius));
            window.refresh();
        });
        f.cx.run_until_parked();
        let trigger =
            f.cx.debug_bounds("picker-filter")
                .expect("caller filter trigger");
        f.cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let scale = window.scale_factor();
            let quads = window.painted_quads();
            let frame = quads
                .iter()
                .find(|q| q.bounds == trigger.scale(scale) && q.border_widths.top.0 > 0.)
                .expect("the trigger paints its frame");
            for corner in [
                frame.corner_radii.top_left,
                frame.corner_radii.top_right,
                frame.corner_radii.bottom_left,
                frame.corner_radii.bottom_right,
            ] {
                assert_eq!(
                    corner.0,
                    radius * scale,
                    "the filter frame takes the Hyprland rounding"
                );
            }
        });
    }
}

#[gpui::test]
fn picker_dropdown_opens_flush_above_its_trigger_on_the_active_filter(cx: &mut TestAppContext) {
    let mut f = fixture(
        cx,
        r#"{"folder":"/d","filters":[{"label":"Images","globs":["*.png"]},{"label":"Text files","globs":["*.txt"]}],"current":"Text files"}"#,
        None,
    );
    f.cx.simulate_resize(size(px(1100.), px(700.)));
    f.cx.run_until_parked();
    let trigger = f.cx.debug_bounds("picker-filter").unwrap();
    f.cx.simulate_click(trigger.center(), Modifiers::none());
    f.cx.run_until_parked();
    let menu = f.read(|picker| picker.chrome.menu_view.clone().unwrap());
    let card = f.cx.update(|_, cx| menu.read(cx).card_frame().unwrap());
    assert_eq!(card.x, f32::from(trigger.left()));
    assert_eq!(card.y + card.h, f32::from(trigger.top()));
    assert_eq!(card.w, 257.);
    f.cx.simulate_keystrokes("enter");
    f.cx.run_until_parked();
    assert!(
        f.read(|picker| picker.chrome.menu_view.is_none()),
        "Enter chose from the open menu"
    );
    assert_eq!(
        f.read(|picker| picker.listing.chip),
        1,
        "Enter keeps the caller's current filter"
    );
}
