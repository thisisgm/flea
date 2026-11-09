//! Show tabs as icons (0401-tabs-icons-a): 44px tabs of folder marks, no close mark, and a path tip under a resting pointer.
use super::bar::{Frame, Hit, HEIGHT, MENU_GAP, PAD_X};
use super::view::Strip;
use crate::state::Shell;
use flea::jsondoc::Json;
use flea::ui_logic_rail_places::rail::RailEntry;
use gpui::{
    deferred, div, prelude::*, px, Context, Div, MouseMoveEvent, Pixels, Point, SharedString,
    Stateful, Task,
};

/// The row padding either side of the 16px chrome mark.
pub const WIDTH: f32 = 44.;
/// How long the pointer rests on a tab before its tip shows.
pub const REST_MS: u64 = 500;
// tooltip.png's plate: 20px tall with 7px of padding, at caption size.
const TIP_HEIGHT: f32 = 20.;
const TIP_PAD: f32 = 7.;

/// What a strip of marks draws in place of names, one entry per tab in model order.
#[derive(Clone, Debug, PartialEq)]
pub struct Icons {
    pub marks: Vec<&'static str>,
    pub paths: Vec<SharedString>,
}

/// The mark the folder draws in the rail, the house for home, and the plain folder for any place the rail does not list.
pub fn mark_for(path: &str, home: &str, rail: &[RailEntry]) -> &'static str {
    if path == home {
        return "house";
    }
    rail.iter()
        .find(|entry| !entry.path.is_empty() && entry.path == path && !entry.glyph.is_empty())
        .map_or("folder", |entry| entry.glyph)
}

// The devices section keeps its marks as text; a mounted device row draws one of these.
fn device_glyph(glyph: &str) -> &'static str {
    match glyph {
        "camera" => "camera",
        "smartphone" => "smartphone",
        "lock" => "lock",
        _ => "drive",
    }
}

pub fn wanted(data: &Json) -> bool {
    data.get("tabsAsIcons").and_then(Json::as_bool) == Some(true)
}

impl Shell {
    /// With `bar.icons` set before the frame was built, the frame takes each tab's mark and path.
    pub(crate) fn mark_tabs(&self, mut frame: Frame, cx: &gpui::App) -> Frame {
        if !self.strip.bar.icons {
            return frame;
        }
        let side = self.focus_side;
        let current = self.panes[side]
            .as_ref()
            .map_or_else(String::new, |host| host.pane.path().to_owned());
        // Places and network rows, then the devices section, so a mounted device draws its drive.
        let rail = self.rail.as_ref().map_or_else(Vec::new, |join| {
            let mut rows = join.places.read(cx).entries();
            let devices = &join.devices.read(cx).devices.entries;
            rows.extend(devices.iter().map(|device| RailEntry {
                path: device.path.clone(),
                glyph: device_glyph(&device.glyph),
                ..RailEntry::default()
            }));
            rows
        });
        let state = &self.strip.tabs[side];
        let paths: Vec<String> = (0..state.count())
            .map(|i| state.path_at(i, &current))
            .collect();
        frame.close_hover = None;
        frame.icons = Some(Icons {
            marks: paths
                .iter()
                .map(|path| mark_for(path, &self.strip.home, &rail))
                .collect(),
            paths: paths.into_iter().map(SharedString::from).collect(),
        });
        frame
    }
}

/// A tip rests only on a tab of a strip of marks, with no button down, no held tab switch and no tab dragged.
pub(super) fn resting_tab(
    marks: bool,
    pressed: bool,
    holding: bool,
    dragging: bool,
    hit: impl FnOnce() -> Hit,
) -> Option<usize> {
    if !marks || pressed || holding || dragging {
        return None;
    }
    match hit() {
        Hit::Tab(index) => Some(index),
        _ => None,
    }
}

/// The tab under a resting pointer; `shown` once the rest has passed.
#[derive(Default)]
pub(crate) struct Tip {
    at: Option<usize>,
    shown: bool,
    // Held only so the rest timer lives; replacing the tip drops it.
    _rest: Option<Task<()>>,
}

impl Tip {
    pub(crate) fn shown(&self) -> Option<usize> {
        self.at.filter(|_| self.shown)
    }
}

impl Strip {
    /// The tip's listeners and rested tip, added after the strip: the strip's bounds recorder reads its first child.
    pub(super) fn with_tip(&self, root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let tip = self.tip_element(cx);
        root.on_mouse_move(cx.listener(|strip, event: &MouseMoveEvent, _, cx| {
            strip.tip_moved(event.position, event.pressed_button.is_some(), cx)
        }))
        .on_any_mouse_down(cx.listener(|strip, _, _, cx| strip.tip_left(cx)))
        .children(tip)
    }

    pub(super) fn tip_left(&mut self, cx: &mut Context<Self>) {
        if self.tip.at.is_some() {
            self.tip = Tip::default();
            cx.notify();
        }
    }

    fn tip_moved(&mut self, position: Point<Pixels>, pressed: bool, cx: &mut Context<Self>) {
        let marks = self.frame.icons.is_some();
        let index = self.shell.upgrade().and_then(|shell| {
            let shell = shell.read(cx);
            resting_tab(
                marks,
                pressed,
                shell.tab_holding(),
                shell.strip.bar.drag_from.is_some(),
                || shell.tab_hit(position),
            )
        });
        match index {
            Some(index) => self.rest_on(index, cx),
            None => self.tip_left(cx),
        }
    }

    /// The pointer is on tab `index`: its tip shows once REST_MS has passed there, and another tab starts the rest over.
    pub(super) fn rest_on(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.tip.at == Some(index) {
            return;
        }
        let rest = std::time::Duration::from_millis(REST_MS);
        let task = cx.spawn(async move |strip, cx| {
            cx.background_executor().timer(rest).await;
            let _ = strip.update(cx, |strip, cx| {
                if strip.tip.at == Some(index) {
                    strip.tip.shown = true;
                    cx.notify();
                }
            });
        });
        let was_shown = self.tip.shown;
        self.tip = Tip {
            at: Some(index),
            shown: false,
            _rest: Some(task),
        };
        if was_shown {
            cx.notify();
        }
    }

    /// tooltip.png's plate at caption size, its left edge on the tab's and MENU_GAP under the strip, over every pane.
    fn tip_element(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        let index = self.tip.shown()?;
        let icons = self.frame.icons.as_ref()?;
        let path = icons.paths.get(index)?.clone();
        let slot = *self.frame.slots.get(index)?;
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let face = self.labels.borrow().typeface.clone();
        let corner = crate::compositor::corner(cx) / 2.;
        Some(deferred(
            div()
                .id("tab-tip")
                .absolute()
                .left(px(PAD_X + slot as f32 * self.frame.tab_width))
                .top(px(HEIGHT + MENU_GAP))
                .h(px(TIP_HEIGHT))
                .px(px(TIP_PAD))
                .flex()
                .items_center()
                .bg(theme.surface)
                .border_1()
                .border_color(theme.muted_frame)
                .rounded(px(corner))
                .font(face.font)
                .text_size(face.caption)
                .text_color(theme.foreground)
                .whitespace_nowrap()
                .child(path),
        ))
    }
}
