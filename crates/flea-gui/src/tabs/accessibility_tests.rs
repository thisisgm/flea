//! TabBar.qml accessibility: tabs are PageTab nodes named by their titles, New tab a Button, both pressable.
use super::{accessibility, bar, Bar, TabState};
use gpui::{accesskit::Action, point, px, size, Bounds, Element, Role};

fn frame(paths: &[&str], current: usize) -> bar::Frame {
    let mut state = TabState::default();
    if paths.len() > 1 {
        state.tabs = Some(super::state::pack(
            paths.iter().map(|path| super::Tab::at(*path)).collect(),
            current,
            None,
        ));
    }
    Bar::new("test").frame(&state, paths[current], "/home/test", "Home")
}

#[test]
fn pb_tabs_tabs_are_named_page_tabs_and_new_tab_is_a_button() {
    let frame = frame(&["/files", "/home/test"], 1);
    let first = accessibility::node(&frame, 0, "New tab");
    assert_eq!(first.role(), Role::Tab);
    assert_eq!(first.label(), Some("files"));
    assert_eq!(first.is_selected(), Some(false));
    assert!(first.supports_action(Action::Click));
    let home = accessibility::node(&frame, 1, "New tab");
    assert_eq!(home.label(), Some("Home"));
    assert_eq!(home.is_selected(), Some(true));
    let add = accessibility::node(&frame, 2, "New tab");
    assert_eq!(add.role(), Role::Button);
    assert_eq!(add.label(), Some("New tab"));
    assert_eq!(add.is_selected(), None);
    assert!(add.supports_action(Action::Click));
}

#[test]
fn pb_tabs_strip_is_an_identified_tab_list() {
    let labels = std::rc::Rc::new(std::cell::RefCell::new(bar::Labels::new(
        crate::list::Typeface::omarchy_default(),
    )));
    let element = bar::StripElement::new(frame(&["/files"], 0), labels);
    assert_eq!(element.a11y_role(), Some(Role::TabList));
    assert!(
        element.id().is_some(),
        "a role reaches the tree only with an id"
    );
}

#[test]
fn pb_tabs_control_boxes_follow_the_painted_slots_and_device_scale() {
    let mut frame = frame(&["/a", "/b"], 0);
    frame.tab_width = 100.;
    frame.slots = vec![1, 0];
    let strip = Bounds::new(point(px(10.), px(27.)), size(px(400.), px(27.)));
    let boxes: Vec<_> = (0..=2)
        .map(|index| frame.control_bounds(strip, index).unwrap())
        .map(|bounds| (f32::from(bounds.left()), f32::from(bounds.size.width)))
        .collect();
    assert_eq!(boxes, [(124., 100.), (24., 100.), (224., bar::HIT_MIN)]);
    assert!(frame.control_bounds(strip, 3).is_none());
    let one = Bounds::new(point(px(14.), px(27.)), size(px(100.), px(27.)));
    for (scale, expected) in [(1., [14., 27., 114., 54.]), (2., [28., 54., 228., 108.])] {
        let rect = accessibility::device_bounds(one, scale);
        assert_eq!([rect.x0, rect.y0, rect.x1, rect.y1], expected);
    }
}

#[gpui::test]
fn pb_tabs_a_press_on_each_control_taps_that_tab_or_new_tab(cx: &mut gpui::TestAppContext) {
    use crate::window::dispatch::test_shell::{land, open_root};
    let (shell, cx) = open_root(cx, "default");
    let row = flea::protocol::RowFacts {
        name: "a.txt".into(),
        ..Default::default()
    };
    land(&shell, cx, "/files", vec![row], 1);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell.tab_action("tabNew", window, cx));
            assert!(shell.tab_action("tabNew", window, cx));
        })
    });
    cx.run_until_parked();
    let weak = shell.downgrade();
    // (control index, current tab after, tab count after): two tabs by identity, then New tab.
    for (index, current, count) in [(0, 0, 3), (2, 2, 3), (3, 3, 4)] {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                let controls = accessibility::Controls::new(weak.clone(), shell)
                    .expect("a strip of three tabs has controls");
                shell.tab_a11y_press(controls.press(index), window, cx);
            })
        });
        cx.run_until_parked();
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.strip.tabs[0].count(), count, "control {index}");
            assert_eq!(shell.strip.tabs[0].current(), current, "control {index}");
        });
    }
    // Presses built before a close resolve by identity: control 2's tab sits at 1 once tab 0 closes.
    let stale = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let stale = accessibility::Controls::new(weak.clone(), shell)
                .expect("a strip of four tabs has controls");
            shell.tab_a11y_press(stale.press(0), window, cx);
            stale
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert_eq!(
                shell.strip.tabs[0].current(),
                0,
                "control 0 before the close"
            );
            assert!(shell.tab_action("tabClose", window, cx));
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.tab_a11y_press(stale.press(2), window, cx)
        })
    });
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.strip.tabs[0].count(), 3, "the close took one tab");
        assert_eq!(
            shell.strip.tabs[0].current(),
            1,
            "the stale press follows its tab"
        );
    });
}
