//! The toolbar's delivery, ink, press and refusal contracts, driven through the live shell.
use super::{
    input::ActionEvent,
    u07_tests::{open, press, read},
};
use crate::path::{join::NetworkPath, Answer, Peeked};
use gpui::{point, px, Modifiers, TestAppContext};
use std::{cell::RefCell, rc::Rc, time::Duration};

fn drawn(centre: &str) -> gpui::Point<gpui::Pixels> {
    let mut values = centre.split_whitespace().map(|v| v.parse::<f32>().unwrap());
    point(px(values.next().unwrap()), px(values.next().unwrap()))
}

#[gpui::test]
fn u07_each_visible_toolbar_control_delivers_the_action_its_owner_knows(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let seen = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let emitted = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    cx.run_until_parked();
    let chrome = shell.read_with(cx, |shell, _| shell.chrome.view.clone().expect("mounted"));
    let _actions = cx.update(|_, cx| {
        let seen = seen.clone();
        cx.subscribe(&shell, move |_, event: &ActionEvent, _| {
            seen.borrow_mut().push(event.action)
        })
    });
    let _controls = cx.update(|_, cx| {
        let emitted = emitted.clone();
        cx.subscribe(&chrome, move |_, action: &&'static str, _| {
            emitted.borrow_mut().push(*action)
        })
    });
    let back = read(&shell, cx, "chromeButtonCentre", &["arrow-left"]);
    press(cx, &back, 1);
    assert!(
        emitted.borrow().is_empty(),
        "a disabled Back never emits: {:?}",
        emitted.borrow()
    );
    let up = read(&shell, cx, "chromeButtonCentre", &["arrow-up"]);
    press(cx, &up, 1);
    assert_eq!(emitted.borrow().as_slice(), ["up"], "an enabled Up emits");
    for (name, action) in [("search", "search"), ("sliders", "settings")] {
        let centre = read(&shell, cx, "chromeButtonCentre", &[name]);
        press(cx, &centre, 1);
        assert_eq!(emitted.borrow().last(), Some(&action));
        if action == "settings" {
            assert!(
                shell.read_with(cx, |shell, _| shell.settings.opened),
                "real U11 owner consumed the toolbar action"
            );
            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
            assert!(!shell.read_with(cx, |shell, _| shell.settings.opened));
        } else {
            assert_eq!(seen.borrow().last(), Some(&action));
        }
    }
    let centre = read(&shell, cx, "chromeButtonCentre", &["grid"]);
    press(cx, &centre, 1);
    for (name, action) in [("filter", "filter"), ("sort", "sortNext")] {
        let centre = read(&shell, cx, "chromeButtonCentre", &[name]);
        press(cx, &centre, 1);
        assert_eq!(
            seen.borrow().last(),
            Some(&action),
            "the {name} control must deliver the action keys.toml names"
        );
    }
}

#[gpui::test]
fn u07_a_press_scales_the_mark_and_hover_is_read_from_the_pointer(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    let centre = read(&shell, cx, "chromeButtonCentre", &["search"]);
    let at = drawn(&centre);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.run_until_parked();
    let state = flea::jsondoc::parse(&read(&shell, cx, "chromeButtonState", &["search"])).unwrap();
    assert_eq!(
        state.get("hovered").and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    cx.simulate_event(gpui::MouseDownEvent {
        position: at,
        button: gpui::MouseButton::Left,
        click_count: 1,
        modifiers: Modifiers::none(),
        first_mouse: false,
    });
    cx.run_until_parked();
    let chrome = shell.read_with(cx, |shell, _| shell.chrome.view.clone().unwrap());
    let down = chrome.read_with(cx, |view, _| view.frame.borrow().controls[2].press);
    assert!(
        matches!(down, Some(crate::path::mark::Press { down: true, .. })),
        "a live press starts the 0.96 tween"
    );
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    let up = chrome.read_with(cx, |view, _| view.frame.borrow().controls[2].press);
    assert!(
        matches!(up, Some(crate::path::mark::Press { down: false, .. })),
        "the release eases back"
    );
}

#[test]
fn u07_the_press_tween_is_ninety_six_percent_over_150_ms_and_reduced_motion_stays_unscaled() {
    use crate::path::mark::{press_scale, Press};
    let start = std::time::Instant::now();
    assert_eq!(press_scale(None, start, false), (1., false));
    assert_eq!(
        press_scale(
            Some(Press {
                down: true,
                since: start,
                from: 1.
            }),
            start,
            false
        ),
        (1., true)
    );
    let (mid, moving) = press_scale(
        Some(Press {
            down: true,
            since: start,
            from: 1.,
        }),
        start + Duration::from_millis(40),
        false,
    );
    assert!(moving && mid < 1. && mid > 0.96, "{mid}");
    let (end, moving) = press_scale(
        Some(Press {
            down: true,
            since: start,
            from: 1.,
        }),
        start + Duration::from_millis(150),
        false,
    );
    assert!(!moving && (end - 0.96).abs() < 1e-6, "{end}");
    assert_eq!(
        press_scale(
            Some(Press {
                down: true,
                since: start,
                from: 1.
            }),
            start,
            true
        ),
        (1., false)
    );
    let (back, moving) = press_scale(
        Some(Press {
            down: false,
            since: start,
            from: 0.96,
        }),
        start + Duration::from_millis(150),
        false,
    );
    assert!(!moving && (back - 1.).abs() < 1e-6, "{back}");
}

#[test]
fn u07_a_focused_live_mark_inks_in_the_foreground_like_its_ring() {
    use crate::path::mark::mark_ink;
    let theme = crate::palette::Theme::default();
    let (muted, foreground) = (theme.background, theme.foreground);
    assert_eq!(mark_ink(true, true, muted, foreground), foreground);
    assert_eq!(mark_ink(true, false, muted, foreground), muted);
    assert_eq!(mark_ink(false, true, muted, foreground), muted);
}

#[gpui::test]
fn u07_a_typed_network_address_is_refused_until_the_network_owner_subscribes(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx);
    let owner = Rc::new(RefCell::new(Vec::<(usize, String)>::new()));
    let _network = cx.update(|_, cx| {
        let owner = owner.clone();
        cx.subscribe(&shell, move |_, event: &NetworkPath, _| {
            owner.borrow_mut().push((event.side, event.uri.clone()))
        })
    });
    let status = |shell: &gpui::Entity<crate::state::Shell>, cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            shell.panes[0].as_ref().unwrap().status.to_string()
        })
    };
    cx.simulate_keystrokes("ctrl-l");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("smb://host/share");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        status(&shell, cx),
        crate::path::Said::NetworkUnavailable
            .text(&shell.read_with(cx, |s, _| s.locale.catalog.clone()))
    );
    assert!(owner.borrow().is_empty());
    shell.update(cx, |shell, _| shell.chrome.network = true);
    cx.simulate_keystrokes("ctrl-l");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("smb://host/share");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        owner.borrow().as_slice(),
        [(0, "smb://host/share".to_owned())]
    );
}

#[gpui::test]
fn u07_a_peeked_answer_the_path_never_asked_falls_through_to_the_next_consumer(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open(cx);
    cx.run_until_parked();
    let foreign = Answer::Peeked(Peeked {
        path: "/a".into(),
        hidden: false,
        dirs: vec!["b".into()],
        ..Default::default()
    });
    let back = cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.path_answer(0, foreign.clone(), window, cx)
        })
    });
    assert_eq!(
        back,
        Some(foreign),
        "the columns join takes what the bar never asked"
    );
}
