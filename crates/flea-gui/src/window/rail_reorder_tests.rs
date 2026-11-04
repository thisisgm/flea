//! EY-311: a Favorites row dragged above another moves through the store, and a still press still opens it.
use super::*;

// A marked scratch root whose script records the --favourites wire and refuses, so nothing real is written.
struct Recorder(std::path::PathBuf);

impl Recorder {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("flea-ey311-{}-{nanos}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join(".flea-test-sandbox"), "ey311\n").unwrap();
        let script = dir.join("flea");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\n[ \"$1\" = --favourites ] && printf '%s\\n' \"$2\" >> '{}'\nexit 1\n",
                dir.join("wire").display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self(dir)
    }
    fn wire(&self) -> String {
        std::fs::read_to_string(self.0.join("wire")).unwrap_or_default()
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let dir = &self.0;
        if dir.is_absolute()
            && dir.join(".flea-test-sandbox").is_file()
            && dir.parent() == Some(std::env::temp_dir().as_path())
        {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

fn favourites(cx: &mut TestAppContext) -> (Entity<Shell>, &mut VisualTestContext, Recorder) {
    let (shell, cx) = open(false, cx);
    shell.update(cx, |shell, cx| {
        shell.rail_state(
            &jsondoc::parse(
                r#"{"places":{"favourites":[{"label":"Docs","path":"/w/Docs"},{"label":"A","path":"/w/A"}]}}"#,
            )
            .unwrap(),
            cx,
        )
    });
    frame(cx);
    let recorder = Recorder::new();
    let script = recorder.0.join("flea");
    producer(&shell, cx).update(cx, |producer, cx| {
        producer
            .places
            .update(cx, |places, _| places.exe = script.clone())
    });
    (shell, cx, recorder)
}

// The rail row and drawn centre of the favourite labelled `label`.
fn centre(
    shell: &Entity<Shell>,
    label: &str,
    cx: &mut VisualTestContext,
) -> (usize, gpui::Point<gpui::Pixels>, f32) {
    producer(shell, cx).read_with(cx, |producer, _| {
        let index = producer
            .entries()
            .iter()
            .position(|entry| entry.label == label && entry.favourite_index.is_some())
            .expect("the favourite is a rail row");
        let laid = producer.laid.borrow();
        let rect = laid.row(index).expect("the favourite is painted").rect;
        (
            index,
            point(px(rect.x + rect.width / 2.), px(rect.y + rect.height / 2.)),
            rect.height,
        )
    })
}

fn place(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> String {
    shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.place().to_owned()
    })
}

#[gpui::test]
fn ey311_dragging_a_favourite_above_another_moves_it_and_opens_nothing(cx: &mut TestAppContext) {
    let (shell, cx, recorder) = favourites(cx);
    let (docs, _, _) = centre(&shell, "Docs", cx);
    let (_, at, row) = centre(&shell, "A", cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    let lifted = at - point(px(0.), px(12.));
    let over = lifted - point(px(0.), px(row));
    cx.simulate_mouse_move(lifted, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_move(over, Some(MouseButton::Left), Modifiers::none());
    frame(cx);
    let boundary = producer(&shell, cx).read_with(cx, |producer, _| {
        producer
            .model
            .borrow()
            .reorder
            .and_then(|held| held.boundary())
    });
    assert_eq!(
        boundary,
        Some((docs, false)),
        "the bar lies over Docs's top edge while A is held"
    );
    cx.simulate_mouse_up(over, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    frame(cx);
    let wire = recorder.wire();
    assert!(
        wire.contains(r#""op":"move""#)
            && wire.contains(r#""index":1"#)
            && wire.contains(r#""to":0"#),
        "the release moves favourite 1 to 0 through the store: {wire:?} said {:?}",
        producer(&shell, cx).read_with(cx, |producer, cx| producer.places.read(cx).said.clone())
    );
    assert_ne!(
        place(&shell, cx),
        "/w/A",
        "a reorder drag never opens its row"
    );
    assert!(producer(&shell, cx).read_with(cx, |producer, _| producer
        .model
        .borrow()
        .reorder
        .is_none()));
}

#[gpui::test]
fn ey311_a_still_press_on_a_favourite_opens_it_on_release(cx: &mut TestAppContext) {
    let (shell, cx, recorder) = favourites(cx);
    let (_, at, _) = centre(&shell, "A", cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        at - point(px(0.), px(4.)),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        at - point(px(0.), px(4.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    frame(cx);
    assert_eq!(
        place(&shell, cx),
        "/w/A",
        "a press under the drag distance is a click"
    );
    assert_eq!(recorder.wire(), "", "a click writes nothing");
}

// Twice a row's centre x lies past its right edge, so this point is level with `at` and off the rail.
fn past_the_rail(at: gpui::Point<gpui::Pixels>) -> gpui::Point<gpui::Pixels> {
    point(at.x * 2. + px(200.), at.y)
}

#[gpui::test]
fn ey311_a_drag_released_past_the_rail_still_moves_the_favourite(cx: &mut TestAppContext) {
    let (shell, cx, recorder) = favourites(cx);
    let (_, at, row) = centre(&shell, "A", cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    let lifted = at - point(px(0.), px(12.));
    let over = lifted - point(px(0.), px(row));
    cx.simulate_mouse_move(lifted, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_move(over, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_move(
        past_the_rail(over),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    cx.simulate_mouse_up(past_the_rail(over), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    frame(cx);
    let wire = recorder.wire();
    assert!(
        wire.contains(r#""op":"move""#)
            && wire.contains(r#""index":1"#)
            && wire.contains(r#""to":0"#),
        "a drag released past the rail lands where the bar was: {wire:?}"
    );
    assert_ne!(
        place(&shell, cx),
        "/w/A",
        "a reorder drag never opens its row"
    );
    assert!(producer(&shell, cx).read_with(cx, |producer, _| producer
        .model
        .borrow()
        .reorder
        .is_none()));
}

#[gpui::test]
fn ey311_a_still_press_released_past_the_rail_opens_nothing(cx: &mut TestAppContext) {
    let (shell, cx, recorder) = favourites(cx);
    let (_, at, _) = centre(&shell, "A", cx);
    let before = place(&shell, cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        past_the_rail(at),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    cx.simulate_mouse_up(past_the_rail(at), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    frame(cx);
    assert_eq!(
        place(&shell, cx),
        before,
        "a press released off its row is no click"
    );
    assert_eq!(
        recorder.wire(),
        "",
        "a press that never dragged writes nothing"
    );
    assert!(
        producer(&shell, cx).read_with(cx, |producer, _| producer.model.borrow().reorder.is_none()),
        "the held favourite is let go"
    );
}

// EY-364: the drag counts from the press, as DragHandler's translation does, so a short drag down onto the lower half of the next row moves it.
#[gpui::test]
fn ey364_dragging_a_favourite_down_onto_the_next_row_moves_it_below(cx: &mut TestAppContext) {
    let (shell, cx, recorder) = favourites(cx);
    let (_, at, row) = centre(&shell, "Docs", cx);
    let (a, _, _) = centre(&shell, "A", cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    // A step under the drag distance, then the step that starts the drag, then the lower half of A.
    for dy in [9., row * 0.7, row * 1.1] {
        cx.simulate_mouse_move(
            at + point(px(0.), px(dy)),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
    }
    frame(cx);
    let boundary = producer(&shell, cx).read_with(cx, |producer, _| {
        producer
            .model
            .borrow()
            .reorder
            .and_then(|held| held.boundary())
    });
    assert_eq!(
        boundary,
        Some((a, true)),
        "the bar lies under A while Docs is held over A's lower half"
    );
    let over = at + point(px(0.), px(row * 1.1));
    cx.simulate_mouse_up(over, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    frame(cx);
    let wire = recorder.wire();
    assert!(
        wire.contains(r#""op":"move""#)
            && wire.contains(r#""index":0"#)
            && wire.contains(r#""to":1"#),
        "the release moves favourite 0 to 1 through the store: {wire:?}"
    );
    assert_ne!(
        place(&shell, cx),
        "/w/Docs",
        "a reorder drag never opens its row"
    );
}

// EY-364: the next row's live bounds decide the drop despite delivered or coalesced motion events.
#[gpui::test]
fn ey364_the_lower_half_decision_does_not_depend_on_which_motion_events_arrived(
    cx: &mut TestAppContext,
) {
    let (shell, cx, recorder) = favourites(cx);
    let (_, at, _) = centre(&shell, "Docs", cx);
    let (a, a_centre, a_height) = centre(&shell, "A", cx);
    // The lower half of A as A is drawn now, a quarter row under its centre.
    let target = a_centre.y + px(a_height / 4.);
    let gap = f32::from(target - at.y);
    // Delivered, coalesced and late first-motion sequences; the last one starts the drag 1 px short of the target.
    let sequences: [&[f32]; 4] = [&[], &[9.], &[9., 11.], &[9., gap - 1.]];
    for steps in sequences {
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        for dy in steps.iter().copied().chain([gap]) {
            cx.simulate_mouse_move(
                point(at.x, at.y + px(dy)),
                Some(MouseButton::Left),
                Modifiers::none(),
            );
        }
        frame(cx);
        let boundary = producer(&shell, cx).read_with(cx, |producer, _| {
            producer
                .model
                .borrow()
                .reorder
                .and_then(|held| held.boundary())
        });
        assert_eq!(
            boundary,
            Some((a, true)),
            "after motions {steps:?} the bar lies under A"
        );
        // Back to the press and let go: a drag that lands where it started moves nothing.
        cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        frame(cx);
    }
    assert_eq!(recorder.wire(), "", "no sequence wrote a move");
}
