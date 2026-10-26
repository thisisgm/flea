//! Each navigation key the table binds changes the path or the listing, and a double click opens the clicked pane's row.
use super::test_shell::{asked, asked_on, keystroke, land, land_on, lists, open, row};
use crate::{
    state::Shell,
    window::{input::event_input, navigate::Step, startup::View, u07_tests::press},
};
use flea::keys::{self, Context, Frontend, Match};
use gpui::{point, px, Entity, KeyDownEvent, Modifiers, TestAppContext, VisualTestContext};

fn home() -> Vec<flea::protocol::RowFacts> {
    vec![
        row("alpha", true),
        row("beta", true),
        row("notes.txt", false),
    ]
}

// The keystrokes a preset binds to an action, each checked to resolve to it in that context.
pub(super) fn strokes(preset: &str, action: &'static str, context: Context) -> Vec<KeyDownEvent> {
    keys::binding_rows(preset, Frontend::Gui)
        .filter(|binding| binding.action == action)
        .map(keystroke)
        .filter(|event| {
            keys::lookup(
                preset,
                context,
                Frontend::Gui,
                event_input(event, Modifiers::none()),
            ) == Match::Action(action)
        })
        .collect()
}

pub(super) fn hit(cx: &mut VisualTestContext, event: &KeyDownEvent, action: &str) {
    let paired = keys::SEQUENCES
        .iter()
        .any(|sequence| sequence.paired && sequence.arm == action);
    for _ in 0..1 + usize::from(paired) {
        cx.simulate_event(event.clone());
        cx.run_until_parked();
    }
}

fn step(shell: &Entity<Shell>, cx: &mut VisualTestContext, step: Step) {
    let done =
        cx.update(|window, cx| shell.update(cx, |shell, cx| shell.step(0, step, window, cx)));
    assert_eq!(done, Ok(true));
}

pub(super) fn close(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
}

// /files, then /files/beta, so Back has a folder to return to.
fn visited<'a>(
    cx: &'a mut TestAppContext,
    preset: &str,
) -> (Entity<Shell>, &'a mut VisualTestContext) {
    let (shell, cx) = open(cx, preset);
    land(&shell, cx, "/files", home(), 1);
    step(&shell, cx, Step::Open("/files/beta".into()));
    land(&shell, cx, "/files/beta", vec![row("inner", true)], 2);
    (shell, cx)
}

#[gpui::test]
fn every_history_page_forward_and_rail_reload_binding_has_its_effect(cx: &mut TestAppContext) {
    for preset in keys::PRESETS {
        let back = strokes(preset, "historyBack", Context::Listing);
        let forward = strokes(preset, "historyForward", Context::Listing);
        let page = strokes(preset, "pageForward", Context::Listing);
        let reload = strokes(preset, "reload", Context::Rail);
        for (action, found) in [("historyBack", &back), ("historyForward", &forward)] {
            assert!(!found.is_empty(), "{preset} binds {action}");
        }
        assert!(
            !page.is_empty() && !reload.is_empty(),
            "{preset} binds both"
        );
        for event in &back {
            let (shell, cx) = visited(cx, preset);
            hit(cx, event, "historyBack");
            assert_eq!(
                asked(&shell, cx),
                "/files",
                "{preset} {:?} goes back",
                event.keystroke
            );
            close(&shell, cx);
        }
        for event in &forward {
            let (shell, cx) = visited(cx, preset);
            step(&shell, cx, Step::Back);
            land(&shell, cx, "/files", home(), 3);
            hit(cx, event, "historyForward");
            assert_eq!(
                asked(&shell, cx),
                "/files/beta",
                "{preset} {:?} goes forward",
                event.keystroke
            );
            close(&shell, cx);
        }
        for event in &page {
            let (shell, cx) = open(cx, preset);
            land(&shell, cx, "/files", home(), 1);
            super::test_shell::set_cursor(&shell, cx, 1);
            hit(cx, event, "pageForward");
            assert_eq!(
                asked(&shell, cx),
                "/files/beta",
                "{preset} {:?} opens the folder row",
                event.keystroke
            );
            close(&shell, cx);
        }
        for event in &reload {
            let (shell, cx) = open(cx, preset);
            land(&shell, cx, "/files", home(), 1);
            cx.update(|window, cx| shell.update(cx, |shell, cx| window.focus(&shell.focus, cx)));
            cx.simulate_keystrokes("tab");
            assert_eq!(
                shell.read_with(cx, |shell, _| shell.keyboard.context),
                Context::Rail
            );
            let out = lists(&shell, cx);
            hit(cx, event, "reload");
            assert_eq!(
                lists(&shell, cx),
                out + 1,
                "{preset} {:?} re-lists from the rail",
                event.keystroke
            );
            close(&shell, cx);
        }
    }
}

// The centre of a drawn row on one side, found from the pane's own hit test.
fn centre(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize, row: usize) -> String {
    shell.read_with(cx, |shell, _| {
        let input = &shell.panes[side].as_ref().unwrap().input;
        let bounds = input.frame.expect("the pane drew its rows").bounds;
        let x = bounds.left() + px(40.);
        let top = f32::from(bounds.top());
        let ys: Vec<f32> = (0..f32::from(bounds.size.height) as usize)
            .map(|dy| top + dy as f32)
            .filter(|&y| input.hit(point(x, px(y))) == Some(row))
            .collect();
        let (first, last) = (ys.first().expect("the row is drawn"), ys.last().unwrap());
        format!("{} {}", f32::from(x), (first + last) / 2.)
    })
}

#[gpui::test]
fn a_double_click_in_the_unfocused_dual_pane_opens_that_panes_row(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Dual, window, cx)));
    let right = asked_on(&shell, cx, 1);
    land_on(
        &shell,
        cx,
        1,
        &right,
        vec![row("gamma", true), row("delta", true)],
        2,
        1,
    );
    assert_eq!(
        shell.read_with(cx, |shell, _| (shell.view, shell.focus_side)),
        (View::Dual, 0),
        "the left pane holds the focus before the click"
    );
    let at = centre(&shell, cx, 1, 1);
    press(cx, &at, 1);
    press(cx, &at, 2);
    assert_eq!(shell.read_with(cx, |shell, _| shell.focus_side), 1);
    assert_eq!(
        asked_on(&shell, cx, 1),
        format!("{}/delta", right.trim_end_matches('/'))
    );
    assert_eq!(asked(&shell, cx), "/files", "the left pane did not move");
    close(&shell, cx);
}

// PR #302: Ctrl with = or 0 alone reaches the listing's text size, as the Ctrl+Shift chords still do.
#[gpui::test]
fn ctrl_equal_and_ctrl_zero_step_the_listing_text_size(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    let stored = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            flea::ui_logic_st1::textsize::parse(
                flea::ui_logic_st1::pages::at(
                    &shell.launch.settings.data,
                    &["display", "textSize"],
                )
                .unwrap_or(&flea::jsondoc::Json::Null),
            )
        })
    };
    let follow = flea::ui_logic_st1::textsize::Stored::Follow;
    assert_eq!(stored(&shell, cx), follow);
    // A text size change re-reads the folder, and a busy pane swallows the next chord, so each re-read is answered.
    let mut listing = 1;
    let mut press = |cx: &mut VisualTestContext, chord: &str| {
        cx.simulate_keystrokes(chord);
        cx.run_until_parked();
        listing += 1;
        land(&shell, cx, "/files", home(), listing);
    };
    for chord in ["ctrl-=", "ctrl-shift-+"] {
        press(cx, chord);
        assert_ne!(
            stored(&shell, cx),
            follow,
            "{chord} overrides the text size"
        );
        press(cx, "ctrl-0");
        assert_eq!(
            stored(&shell, cx),
            follow,
            "Ctrl+0 follows Omarchy again after {chord}"
        );
    }
    close(&shell, cx);
}
