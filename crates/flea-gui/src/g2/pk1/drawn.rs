//! The chooser's drawn geometry: the boxes its last frame painted, read by the fleapicker target.
use super::ipc::PickerGeometry;
use flea::ipc::wire::Request;
use flea::json::escape;
use gpui::{
    canvas, point, px, size, App, Bounds, Hsla, IntoElement, Pixels, Rgba, SharedString, Styled,
};
use std::{cell::RefCell, rc::Rc};

// A reader answered once the next probed frame has painted.
type Waiting = Box<dyn FnOnce(&mut App)>;

// A button the frame painted: its words, its box, and whether its press does anything.
#[derive(Clone, Debug)]
struct Button {
    name: SharedString,
    bounds: Bounds<Pixels>,
    enabled: bool,
    hovered: bool,
    pressed: bool,
}

#[derive(Default)]
struct Frame {
    // Set by the first reader that asks for a frame; until then no frame records anything.
    armed: bool,
    // A reader that came before any probed frame painted, answered once the frame has.
    waiting: Vec<Waiting>,
    window: Option<Bounds<Pixels>>,
    title: Option<Bounds<Pixels>>,
    body: Option<Bounds<Pixels>>,
    footer: Option<Bounds<Pixels>>,
    cancel: Option<Bounds<Pixels>>,
    answer: Option<Bounds<Pixels>>,
    cancel_name: SharedString,
    answer_name: SharedString,
    answer_enabled: bool,
    foreground: Option<Hsla>,
    themed: bool,
    sizes: (f32, f32),
    // The family the title's text inherits from the window, so every chrome run draws in it.
    family: SharedString,
    // The ink the answer's label inherits from its button, read only by the C2 tests.
    #[cfg(test)]
    answer_ink: Option<Hsla>,
    pointers: [(bool, bool); 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Slot {
    Window,
    Title,
    Body,
    Footer,
    Cancel,
    Answer,
}

// What the render pass knows and the paint pass cannot: the words and states the boxes belong to.
pub(super) struct Painted {
    pub cancel: SharedString,
    pub answer: SharedString,
    pub answer_enabled: bool,
    pub foreground: Hsla,
    pub themed: bool,
    pub sizes: (f32, f32),
}

#[derive(Clone, Default)]
pub struct Drawn(Rc<RefCell<Frame>>);

impl Drawn {
    pub(super) fn armed(&self) -> bool {
        self.0.borrow().armed
    }

    pub(super) fn arm(&self) {
        self.0.borrow_mut().armed = true;
    }

    // Only these reads wait for a painted frame.
    pub(super) fn reads_frame(request: &Request) -> bool {
        matches!(request, Request::Call { target, function, .. } if target == "fleapicker" && matches!(function.as_str(), "snapshot" | "controls" | "rowCentre" | "cohesionControls" | "saveState" | "pathBarOpen"))
    }

    // Runs `answer` after the next frame whose probes have all recorded.
    pub(super) fn when_painted(&self, answer: impl FnOnce(&mut App) + 'static) {
        self.0.borrow_mut().waiting.push(Box::new(answer));
    }

    pub(super) fn render(&self, painted: Painted) {
        let mut frame = self.0.borrow_mut();
        frame.cancel_name = painted.cancel;
        frame.answer_name = painted.answer;
        frame.answer_enabled = painted.answer_enabled;
        frame.foreground = Some(painted.foreground);
        frame.themed = painted.themed;
        frame.sizes = painted.sizes;
    }

    // Armed probes record outer boxes; the final window probe releases readers after every control prepaints.
    pub(super) fn probe(&self, slot: Slot, border: [f32; 4]) -> Option<impl IntoElement> {
        if !self.armed() {
            return None;
        }
        let drawn = self.clone();
        Some(
            canvas(
                move |bounds, window, cx| {
                    let [top, right, bottom, left] = border.map(px);
                    let outer = Bounds::new(
                        point(bounds.origin.x - left, bounds.origin.y - top),
                        size(
                            bounds.size.width + left + right,
                            bounds.size.height + top + bottom,
                        ),
                    );
                    drawn.record(slot, outer);
                    if slot == Slot::Title {
                        drawn.0.borrow_mut().family = window.text_style().font_family;
                    }
                    #[cfg(test)]
                    if slot == Slot::Answer {
                        drawn.0.borrow_mut().answer_ink = Some(window.text_style().color);
                    }
                    if slot == Slot::Window {
                        let waiting = std::mem::take(&mut drawn.0.borrow_mut().waiting);
                        for answer in waiting {
                            cx.defer(answer);
                        }
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .left(px(0.))
            .top(px(0.))
            .size_full(),
        )
    }

    #[cfg(test)]
    pub(super) fn answer_ink(&self) -> Option<Hsla> {
        self.0.borrow().answer_ink
    }

    fn record(&self, slot: Slot, bounds: Bounds<Pixels>) {
        let mut frame = self.0.borrow_mut();
        let target = match slot {
            Slot::Window => &mut frame.window,
            Slot::Title => &mut frame.title,
            Slot::Body => &mut frame.body,
            Slot::Footer => &mut frame.footer,
            Slot::Cancel => &mut frame.cancel,
            Slot::Answer => &mut frame.answer,
        };
        *target = Some(bounds);
    }

    pub(super) fn pointer(&self, slot: Slot, hovered: Option<bool>, pressed: Option<bool>) {
        let index = match slot {
            Slot::Cancel => 0,
            Slot::Answer => 1,
            _ => return,
        };
        let mut frame = self.0.borrow_mut();
        let pointer = &mut frame.pointers[index];
        if let Some(hovered) = hovered {
            pointer.0 = hovered;
        }
        if let Some(pressed) = pressed {
            pointer.1 = pressed;
        }
    }

    fn buttons(&self) -> Vec<Button> {
        let frame = self.0.borrow();
        [
            (frame.cancel, &frame.cancel_name, true),
            (frame.answer, &frame.answer_name, frame.answer_enabled),
        ]
        .into_iter()
        .enumerate()
        .filter_map(|(index, (bounds, name, enabled))| {
            Some(Button {
                name: name.clone(),
                bounds: bounds?,
                enabled,
                hovered: frame.pointers[index].0,
                pressed: frame.pointers[index].1,
            })
        })
        .collect()
    }

    pub(super) fn controls_json(&self) -> String {
        let controls: Vec<String> = self
            .buttons()
            .iter()
            .map(|button| {
                let (x, y, w, h) = rect(button.bounds);
                format!(
                    r#"{{"name":"{}","visible":true,"enabled":{},"focused":false,"hovered":{},"pressed":{},"centre":"{} {}","bounds":[{x},{y},{w},{h}]}}"#,
                    escape(&button.name),
                    button.enabled,
                    button.hovered,
                    button.pressed,
                    (x + w / 2.).round(),
                    (y + h / 2.).round(),
                )
            })
            .collect();
        format!("[{}]", controls.join(","))
    }
}

fn rect(bounds: Bounds<Pixels>) -> (f32, f32, f32, f32) {
    (
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    )
}

fn hex(colour: Hsla) -> String {
    let Rgba { r, g, b, .. } = Rgba::from(colour);
    let byte = |channel: f32| (channel * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

impl PickerGeometry for Drawn {
    // The live adapter adds the listing and field readers to the shared snapshot probe.
    fn answers(&self, function: &str) -> bool {
        matches!(
            function,
            "snapshot" | "controls" | "rowCentre" | "cohesionControls" | "saveState"
        )
    }
    fn controls(&self) -> String {
        self.controls_json()
    }
    fn row_centre(&self, _: usize) -> String {
        String::new()
    }
    fn cohesion_controls(&self) -> String {
        "[]".into()
    }
    fn save_field_centre(&self) -> String {
        String::new()
    }
    fn path_bar_open(&self) -> bool {
        false
    }
    // Only the boxes a frame painted: a surface the chooser has not mounted reports nothing.
    fn snapshot_members(&self) -> String {
        let frame = self.0.borrow();
        let Some(window) = frame.window else {
            return String::new();
        };
        let (_, _, width, height) = rect(window);
        let high = |bounds: Option<Bounds<Pixels>>| bounds.map_or(0., |bounds| rect(bounds).3);
        let (body, small) = frame.sizes;
        let foreground = frame.foreground.map(hex).unwrap_or_default();
        let (title, list, footer) = (high(frame.title), high(frame.body), high(frame.footer));
        drop(frame);
        format!(
            concat!(
                r#""controls":{},"bodySmall":{},"body":{},"width":{},"height":{},"#,
                r#""themeLoaded":{},"themeForeground":"{}","family":"{}","#,
                r#""geometry":{{"chrome":{},"footer":{},"list":{}}}"#
            ),
            self.controls_json(),
            small,
            body,
            width,
            height,
            self.0.borrow().themed,
            foreground,
            escape(&self.0.borrow().family),
            title,
            footer,
            list,
        )
    }
}
