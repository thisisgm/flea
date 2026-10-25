//! The FLASH card's window tests: taps, clicks and the dimmed step through the live harness.
use super::draw::StandIn;
use super::draw_flash::{CancelTap, ClickTap, DownTap, FlashFrame, FlashTaps, HoverTap, StepTap, UpTap};
use super::flash::{Flash, SEL_IMAGE_ROW};
use super::tests::english;
use crate::list::{bench::real_cx, Palette, Typeface};
use crate::palette::Theme;
use super::Effect;
use flea::ui_logic_op2::wire;
use flea::i18n::I18n;
use gpui::{prelude::*, px, size, ClickEvent, Context, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Render, VisualTestContext, Window};
use std::rc::Rc;

pub(crate) struct LiveFlash {
    pub(crate) flash: Flash,
    pub(crate) i18n: I18n,
    pub(crate) radius: f32,
    sent: Vec<Effect>,
    steps_only: bool,
    step_taps: usize,
}

impl LiveFlash {
    fn with_targets(rows: Vec<wire::TargetRow>, steps_only: bool) -> Self {
        let i18n = english();
        let mut flash = Flash::closed();
        flash.open_with_image("/scope/iso/debian-13.iso");
        flash.receive_image(
            "/scope/iso/debian-13.iso",
            &wire::FlashImage { kind: "iso".into(), bytes: 4294967296, unpacked: 4294967296, known: true, member: String::new() },
        );
        flash.receive_targets(rows);
        Self { flash, i18n, radius: 8., sent: Vec::new(), steps_only, step_taps: 0 }
    }

    pub(crate) fn two_fitting() -> Self {
        Self::with_targets(
            vec![
                wire::TargetRow {
                    dev: "sda".into(),
                    seq: 2,
                    serial: "2003007157171692".into(),
                    label: "128GB".into(),
                    model: "General USB Flash Disk".into(),
                    bytes: 124623257600,
                    refused: String::new(),
                },
                wire::TargetRow {
                    dev: "sdb".into(),
                    seq: 3,
                    serial: "001122334455".into(),
                    label: "64GB".into(),
                    model: "SanDisk Cruzer Blade".into(),
                    bytes: 64000000000,
                    refused: String::new(),
                },
            ],
            false,
        )
    }

    fn dimmed_cancel() -> Self {
        let i18n = english();
        let mut live = Self::with_targets(
            vec![wire::TargetRow {
                dev: "sda".into(),
                seq: 2,
                serial: "2003007157171692".into(),
                label: "128GB".into(),
                model: "General USB Flash Disk".into(),
                bytes: 124623257600,
                refused: String::new(),
            }],
            true,
        );
        live.flash.press_flash();
        live.flash.start_transfer(&i18n, 1, 4294967296, true, 0);
        live.flash.progress_transfer(
            &i18n,
            &wire::FlashProgress { id: 1, phase: "flush".into(), bytes: 4294967296, total: 4294967296, permille: 1000 },
            10,
        );
        assert!(live.flash.running(), "the flash still runs in its flush for the dimmed Cancel");
        live
    }

    pub(crate) fn live_flash_action() -> Self {
        let mut live = Self::with_targets(
            vec![wire::TargetRow {
                dev: "sda".into(),
                seq: 2,
                serial: "2003007157171692".into(),
                label: "128GB".into(),
                model: "General USB Flash Disk".into(),
                bytes: 124623257600,
                refused: String::new(),
            }],
            true,
        );
        assert!(live.flash.choose_drive(0).is_empty(), "choosing the fitting drive sends nothing");
        assert!(live.flash.live(), "the chosen drive keeps the Flash action live");
        live
    }

    pub(crate) fn live_cancel() -> Self {
        let i18n = english();
        let mut live = Self::with_targets(
            vec![wire::TargetRow {
                dev: "sda".into(),
                seq: 2,
                serial: "2003007157171692".into(),
                label: "128GB".into(),
                model: "General USB Flash Disk".into(),
                bytes: 124623257600,
                refused: String::new(),
            }],
            false,
        );
        assert!(live.flash.choose_drive(0).is_empty(), "choosing the fitting drive sends nothing");
        live.flash.press_flash();
        live.flash.start_transfer(&i18n, 1, 4294967296, true, 0);
        live.flash.progress_transfer(
            &i18n,
            &wire::FlashProgress { id: 1, phase: "write".into(), bytes: 2147483648, total: 4294967296, permille: 500 },
            10,
        );
        assert!(live.flash.running(), "the flash still runs halfway through its write");
        assert!(live.flash.cancel_live(), "Cancel stays live outside the final flush");
        live
    }

    pub(crate) fn failed_end() -> Self {
        let i18n = english();
        let mut live = Self::two_fitting();
        assert!(live.flash.choose_drive(0).is_empty(), "choosing the fitting drive sends nothing");
        live.flash.press_flash();
        live.flash.start_transfer(&i18n, 1, 4294967296, true, 0);
        let done = flea::jsondoc::parse(r#"{"t":"flashdone","id":1,"ok":false,"phase":"write","at":0,"err":"busy","blank":false}"#).unwrap();
        live.flash.finish_transfer(&i18n, &wire::parse_flashdone(&done));
        assert!(live.flash.view(&i18n).step.end_visible, "the failure parks on its end button");
        assert!(live.flash.live(), "the chosen drive keeps the end button available");
        live
    }

    pub(crate) fn done_end() -> Self {
        let mut live = Self::live_cancel();
        let done = flea::jsondoc::parse(r#"{"t":"flashdone","id":1,"ok":true,"phase":"verify","at":4294967296,"err":"","blank":false}"#).unwrap();
        live.flash.finish_transfer(&live.i18n, &wire::parse_flashdone(&done));
        live
    }

    fn picks(&self) -> usize {
        self.sent.iter().filter(|e| matches!(e, Effect::PickImage)).count()
    }

    pub(crate) fn sends(&self) -> Vec<String> {
        self.sent
            .iter()
            .filter_map(|e| match e { Effect::Send(line) => Some(line.clone()), _ => None })
            .collect()
    }
}

impl Render for LiveFlash {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let typeface = Typeface::omarchy_default();
        let seen = self.flash.view(&self.i18n);
        let image: ClickTap = Rc::new(cx.listener(|live: &mut Self, _: &ClickEvent, _, cx| {
            let fx = live.flash.choose_image();
            live.sent.extend(fx);
            cx.notify();
        }));
        let drives: Vec<ClickTap> = seen
            .drives
            .iter()
            .enumerate()
            .map(|(at, _)| {
                Rc::new(cx.listener(move |live: &mut Self, _: &ClickEvent, _, cx| {
                    let fx = live.flash.choose_drive(at);
                    live.sent.extend(fx);
                    cx.notify();
                })) as ClickTap
            })
            .collect();
        let hover: HoverTap = Rc::new(cx.listener(|live: &mut Self, over: &bool, _, cx| {
            live.flash.set_action_hover(*over, cx.background_executor().now());
            cx.notify();
        }));
        let down: DownTap = Rc::new(cx.listener(|live: &mut Self, _: &MouseDownEvent, _, cx| {
            live.flash.set_action_pressed(true, cx.background_executor().now());
            cx.notify();
        }));
        let up: UpTap = Rc::new(cx.listener(|live: &mut Self, _: &MouseUpEvent, _, cx| {
            live.flash.set_action_pressed(false, cx.background_executor().now());
            cx.notify();
        }));
        let cancelled = cx.listener(|live: &mut Self, _: &(), _, cx| {
            live.flash.set_action_pressed(false, cx.background_executor().now());
            cx.notify();
        });
        let cancel: CancelTap = Rc::new(move |window, cx| cancelled(&(), window, cx));
        let now = cx.background_executor().now();
        let reduced = cx.reduce_motion();
        if self.steps_only {
            let step: StepTap = Rc::new(cx.listener(|live: &mut Self, _: &MouseUpEvent, _, cx| {
                live.step_taps += 1;
                let fx = live.flash.press_step();
                live.sent.extend(fx);
                cx.notify();
            }));
            let taps = FlashTaps {
                step: Some(step),
                hover: Some(hover),
                down: Some(down),
                up: Some(up),
                cancel: Some(cancel),
                ..FlashTaps::default()
            };
            return StandIn.present_flash(FlashFrame {
                view: &seen,
                i18n: &self.i18n,
                palette: &palette,
                typeface: &typeface,
                radius: self.radius,
                taps: &taps,
                press: self.flash.press_anim(now),
                now,
                reduced,
                window,
            });
        }
        let step: StepTap = Rc::new(cx.listener(|live: &mut Self, _: &MouseUpEvent, _, cx| {
            let fx = live.flash.press_step();
            live.sent.extend(fx);
            cx.notify();
        }));
        let taps =
            FlashTaps { image: Some(image), drives, step: Some(step), hover: Some(hover), down: Some(down), up: Some(up), cancel: Some(cancel) };
        StandIn.present_flash(FlashFrame {
            view: &seen,
            i18n: &self.i18n,
            palette: &palette,
            typeface: &typeface,
            radius: self.radius,
            taps: &taps,
            press: self.flash.press_anim(now),
            now,
            reduced,
            window,
        })
    }
}

#[test]
fn clicks_on_flash_rows_reach_the_oracles_entries() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::two_fitting());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let image = visual.debug_bounds(SEL_IMAGE_ROW).expect("the image row carries its selector");
    let first = visual.debug_bounds("op2-flash-drive-0").expect("drive rows carry their selectors");
    let second = visual.debug_bounds("op2-flash-drive-1").expect("drive rows carry their selectors");
    visual.simulate_click(image.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert_eq!(live.picks(), 1, "a left release on the image row asks for the picker");
            assert!(live.flash.view(&live.i18n).image.cursor, "the cursor moves to the image row first");
        })
        .unwrap();
    visual.simulate_click(second.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert_eq!(live.flash.selected(), Some(1), "a left release on a drive row chooses it");
        })
        .unwrap();
    visual.simulate_click(first.center(), Modifiers::none());
    visual.run_until_parked();
    let action = visual.debug_bounds("op2-flash-action").expect("rest draws the Flash action");
    assert!(visual.debug_bounds("op2-flash-wash-hover").is_none(), "rest lays no hover wash");
    assert!(visual.debug_bounds("op2-flash-wash-press").is_none(), "rest lays no press wash");
    visual.simulate_mouse_move(action.center(), None, Modifiers::none());
    visual.run_until_parked();
    window.update(&mut cx, |live, _, _| assert!(live.flash.view(&live.i18n).step.action_hover)).unwrap();
    assert!(visual.debug_bounds("op2-flash-wash-hover").is_some(), "hover lays the 0.08 ink wash");
    assert!(visual.debug_bounds("op2-flash-wash-press").is_none(), "hover lays no press wash");
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window.update(&mut cx, |live, _, _| assert!(live.flash.view(&live.i18n).step.action_pressed)).unwrap();
    assert!(visual.debug_bounds("op2-flash-wash-press").is_some(), "press lays the 0.14 ink wash");
    visual.simulate_mouse_up(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(!live.flash.view(&live.i18n).step.action_pressed, "a release clears the press");
        })
        .unwrap();
    assert!(visual.debug_bounds("op2-flash-wash-press").is_none(), "a release lifts the press wash");
    visual.simulate_mouse_down(action.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    visual.simulate_mouse_up(image.center(), MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(!live.flash.view(&live.i18n).step.action_pressed, "a release outside the action clears the press");
        })
        .unwrap();
    visual.simulate_click(action.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert!(
                live.sends().iter().any(|line| line.contains(r#""c":"flash""#) && line.contains(r#""dev":"sda""#)),
                "a left release on the action flashes the chosen drive"
            );
            live.flash.start_transfer(&live.i18n, 1, 4294967296, true, 0);
            assert!(live.flash.running(), "the backend answer starts the transfer");
        })
        .unwrap();
    let second = visual.debug_bounds("op2-flash-drive-1").expect("drive rows keep their selectors");
    visual.simulate_click(second.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert_eq!(live.flash.selected(), Some(0), "no choice moves while a flash runs");
        })
        .unwrap();
}

#[test]
fn a_click_on_the_dimmed_flush_cancel_in_a_real_window_sends_nothing() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::dimmed_cancel());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let cancel = visual.debug_bounds("op2-flash-cancel").expect("the running step draws Cancel");
    visual.simulate_mouse_move(cancel.center(), None, Modifiers::none());
    visual.run_until_parked();
    assert!(visual.debug_bounds("op2-flash-wash-hover").is_none(), "a dimmed hover lays no wash");
    assert!(visual.debug_bounds("op2-flash-wash-press").is_none(), "a dimmed hover lays no press wash");
    visual.simulate_click(cancel.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert_eq!(live.step_taps, 0, "a click on the dimmed Cancel reaches no tap");
            assert!(
                live.sends().iter().all(|line| !line.contains("flashcancel")),
                "a click on the dimmed Cancel sends no flashcancel"
            );
        })
        .unwrap();
}

#[test]
fn a_click_on_the_live_flash_action_reaches_only_its_tap() {
    let mut cx = real_cx();
    let window = cx.open_window(size(px(1100.), px(700.)), |_, _| LiveFlash::live_flash_action());
    let mut visual = VisualTestContext::from_window(window.into(), &cx);
    visual.run_until_parked();
    let action = visual.debug_bounds("op2-flash-action").expect("the live step draws its Flash action");
    visual.simulate_click(action.center(), Modifiers::none());
    visual.run_until_parked();
    window
        .update(&mut cx, |live, _, _| {
            assert_eq!(live.step_taps, 1, "a click on the live action reaches its tap once");
            let sends = live.sends();
            assert!(
                matches!(sends.as_slice(), [line] if line.contains(r#""c":"flash""#)),
                "the one tap sends exactly one flash request: {sends:?}"
            );
        })
        .unwrap();
}
