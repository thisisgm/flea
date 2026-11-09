//! The delete card's lifetime against TrashView.qml: re-review on change, the 2500 ms check, keys during a review, Escape and the button's press.
use super::board::HOVER;
use super::monitor::{Heard, Monitor, Started};
use super::pane::{Button, TrashPane};
use super::tests::{answer, ink, listed, open, opened, row, sent, Call};
use crate::list::bench::real_cx;
use flea::ui_logic_trash_browser::view::CHECK_MS;
use gpui::{Modifiers, MouseButton, Point, TestAppContext, VisualTestContext, WindowHandle};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

type Calls = Rc<RefCell<Vec<Call>>>;

fn cards(calls: &Calls) -> Vec<Option<(String, String, u64)>> {
    calls
        .borrow()
        .iter()
        .filter_map(|call| match call {
            Call::Card(card) => Some(
                card.as_ref()
                    .map(|c| (c.title.clone(), c.body.clone(), c.token)),
            ),
            _ => None,
        })
        .collect()
}

fn reviewed(cx: &mut TestAppContext) -> (WindowHandle<TrashPane>, Calls) {
    let (window, calls) = opened(cx, &[row("a.txt", None)]);
    window
        .update(cx, |pane, _, cx| pane.empty_pressed(cx))
        .unwrap();
    answer(
        cx,
        window,
        &calls,
        "prepare",
        r#","token":4,"all":true,"count":1,"bytes":13,"partial":false"#,
    );
    (window, calls)
}

fn changed(cx: &mut TestAppContext, window: WindowHandle<TrashPane>) {
    window
        .update(cx, |pane, _, cx| pane.heard(Heard::Changed, cx))
        .unwrap();
}

#[test]
fn a_trash_change_closes_the_card_and_reopens_it_with_the_new_review() {
    let mut cx = real_cx();
    let (window, calls) = reviewed(&mut cx);
    changed(&mut cx, window);
    let again = sent(&calls).last().unwrap().clone();
    assert!(
        again.contains(r#""op":"prepare""#) && again.contains(r#""refreshToken":4"#),
        "the open review is asked again: {again}"
    );
    assert_eq!(
        cards(&calls).last(),
        Some(&None),
        "the card closes at once, as sourceChanged closes TrashConfirm"
    );
    answer(
        &mut cx,
        window,
        &calls,
        "prepare",
        r#","token":5,"all":true,"count":2,"bytes":26,"partial":false"#,
    );
    let first = Some((
        "Empty Trash?".to_string(),
        "1 item, 13 B. This deletes it from disk. This cannot be undone.".to_string(),
        4,
    ));
    let second = Some((
        "Empty Trash?".to_string(),
        "2 items, 26 B. This deletes them from disk. This cannot be undone.".to_string(),
        5,
    ));
    assert_eq!(
        cards(&calls),
        [first, None, second],
        "the new review opens with its own count, bytes and token"
    );
    window
        .update(&mut cx, |pane, _, cx| pane.card_answered(Some(5), cx))
        .unwrap();
    let delete = sent(&calls).last().unwrap().clone();
    assert!(
        delete.contains(r#""op":"delete""#) && delete.contains(r#""token":5"#),
        "the destructive press sends the token on screen: {delete}"
    );
}

#[test]
fn an_answer_from_a_card_already_closed_sends_nothing() {
    let mut cx = real_cx();
    let (window, calls) = reviewed(&mut cx);
    changed(&mut cx, window);
    let before = sent(&calls).len();
    window
        .update(&mut cx, |pane, _, cx| {
            pane.card_answered(Some(4), cx);
            pane.card_answered(None, cx);
            assert!(pane.view().confirming, "the re-review still runs");
        })
        .unwrap();
    assert_eq!(
        sent(&calls).len(),
        before,
        "a stale token never reaches the backend, and a stale cancel does not end the re-review"
    );
}

#[test]
fn an_open_card_checks_every_2500_ms_until_it_closes() {
    let mut cx = real_cx();
    let (window, calls) = reviewed(&mut cx);
    let checks = |calls: &Calls| {
        sent(calls)
            .iter()
            .filter(|line| line.contains(r#""op":"check""#) && line.contains(r#""token":4"#))
            .count()
    };
    cx.executor()
        .advance_clock(Duration::from_millis(CHECK_MS - 1));
    cx.run_until_parked();
    assert_eq!(checks(&calls), 0, "nothing before the first period");
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(checks(&calls), 1, "one check per 2500 ms");
    cx.executor()
        .advance_clock(Duration::from_millis(2 * CHECK_MS));
    cx.run_until_parked();
    assert_eq!(
        checks(&calls),
        1,
        "a check in flight stops the timer, as running needs !busy"
    );
    answer(&mut cx, window, &calls, "check", r#","valid":true"#);
    cx.executor()
        .advance_clock(Duration::from_millis(CHECK_MS - 1));
    cx.run_until_parked();
    assert_eq!(checks(&calls), 1, "the timer restarts from the reply");
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(checks(&calls), 2);
    answer(&mut cx, window, &calls, "check", r#","valid":false"#);
    assert!(
        sent(&calls).last().unwrap().contains(r#""refreshToken":4"#),
        "an invalid check re-reviews"
    );
    assert_eq!(cards(&calls).last(), Some(&None), "and closes the card");
    cx.executor()
        .advance_clock(Duration::from_millis(3 * CHECK_MS));
    cx.run_until_parked();
    assert_eq!(checks(&calls), 2, "no check runs while no card is open");
}

fn burst() -> Started {
    let sh = |script: &str| vec!["sh".to_string(), "-c".to_string(), script.to_string()];
    Monitor::start_with(
        sh("sleep 0.3; echo change; sleep 5"),
        sh("echo 'trash::item-count: 1'"),
    )
}

#[test]
fn an_open_trash_starts_its_watch_and_re_reads_after_each_settled_burst() {
    let mut cx = real_cx();
    let (window, calls) = open(&mut cx);
    window
        .update(&mut cx, |pane, _, cx| {
            pane.monitor_start = Some(burst);
            pane.open("", cx);
        })
        .unwrap();
    answer(
        &mut cx,
        window,
        &calls,
        "list",
        &listed(&[row("a.txt", None)]),
    );
    answer(
        &mut cx,
        window,
        &calls,
        "summary",
        r#","bytes":13,"partial":false"#,
    );
    let lists = |calls: &Calls| {
        sent(calls)
            .iter()
            .filter(|line| line.contains(r#""op":"list""#))
            .count()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while lists(&calls) < 2 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        cx.run_until_parked();
    }
    assert_eq!(
        lists(&calls),
        2,
        "the settled burst re-reads the open listing"
    );
    assert!(
        calls.borrow().contains(&Call::Count(1)),
        "the watch's count reaches the rail"
    );
}

#[test]
fn escape_backspace_and_the_back_button_close_while_a_review_runs() {
    let mut cx = real_cx();
    for press in ["escape", "back"] {
        let (window, calls) = opened(&mut cx, &[row("a.txt", None)]);
        window
            .update(&mut cx, |pane, _, cx| {
                pane.empty_pressed(cx);
                pane.action("quit", false, cx);
                pane.action(press, false, cx);
            })
            .unwrap();
        assert!(
            calls.borrow().contains(&Call::Back),
            "{press} before the card opens closes Trash"
        );
        assert!(
            sent(&calls).last().unwrap().contains(r#""op":"cancel""#),
            "and cancels the review"
        );
        assert!(
            !calls.borrow().contains(&Call::Action("quit".into())),
            "TrashHost.qml drops forwarded actions while confirming"
        );
    }
    let (window, calls) = opened(&mut cx, &[row("a.txt", None)]);
    window
        .update(&mut cx, |pane, _, cx| pane.empty_pressed(cx))
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let back = visual
        .debug_bounds("trash-back")
        .expect("the back button is drawn");
    visual.simulate_click(back.center(), Modifiers::none());
    assert!(
        calls.borrow().contains(&Call::Back),
        "the back button closes during a review, as backButton calls root.close()"
    );
    let (window, calls) = reviewed(&mut cx);
    window
        .update(&mut cx, |pane, _, cx| pane.action("escape", false, cx))
        .unwrap();
    assert!(
        !calls.borrow().contains(&Call::Back),
        "once the card is open it holds the keyboard"
    );
}

#[test]
fn a_review_without_a_count_opens_an_empty_card_that_cancels() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("a.txt", None)]);
    window
        .update(&mut cx, |pane, _, cx| pane.empty_pressed(cx))
        .unwrap();
    answer(
        &mut cx,
        window,
        &calls,
        "prepare",
        r#","token":4,"all":true"#,
    );
    assert_eq!(
        cards(&calls),
        [Some((String::new(), String::new(), 4))],
        "the card opens with no words, as Trash.js's filled guard leaves it"
    );
    window
        .update(&mut cx, |pane, _, cx| {
            pane.card_answered(None, cx);
            assert!(!pane.view().confirming, "Cancel ends the review");
        })
        .unwrap();
    assert!(sent(&calls).last().unwrap().contains(r#""op":"cancel""#));
}

#[test]
fn escape_dismisses_a_status_message_before_it_closes_trash() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("a.txt", None)]);
    window
        .update(&mut cx, |pane, _, cx| {
            pane.action("trashArm", false, cx);
            pane.action("escape", false, cx);
        })
        .unwrap();
    assert!(
        calls.borrow().contains(&Call::Escape) && !calls.borrow().contains(&Call::Back),
        "the first Escape dismisses the prompt and Trash stays"
    );
    window
        .update(&mut cx, |pane, _, cx| pane.action("escape", false, cx))
        .unwrap();
    assert!(
        calls.borrow().contains(&Call::Back),
        "the next Escape closes Trash"
    );
}

#[test]
fn a_press_released_outside_empty_trash_leaves_no_press_wash() {
    let mut cx = real_cx();
    let (window, calls) = opened(&mut cx, &[row("a.txt", None)]);
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let button = visual
        .debug_bounds("trash-empty")
        .expect("Empty Trash… is drawn");
    let away = Point::new(button.center().x, button.center().y + gpui::px(300.));
    visual.simulate_mouse_move(button.center(), None, Modifiers::none());
    visual.simulate_mouse_down(button.center(), MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(away, Some(MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(away, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(button.center(), None, Modifiers::none());
    window
        .update(&mut cx, |pane, _, _| {
            assert_eq!(
                pane.button,
                Button {
                    hover: true,
                    pressed: false
                },
                "the release outside ends the press"
            );
            assert_eq!(
                pane.button_look().wash,
                Some(ink().foreground.opacity(HOVER)),
                "the next hover draws the 8% wash, not the press"
            );
        })
        .unwrap();
    assert!(
        !sent(&calls)
            .iter()
            .any(|line| line.contains(r#""op":"prepare""#)),
        "a release outside is no click"
    );
}
