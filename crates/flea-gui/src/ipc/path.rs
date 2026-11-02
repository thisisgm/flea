//! The path bar's IPC readers: whether it is open, what it holds, and where each crumb a test presses lies.
use super::Adapter;
use crate::path::view::PathView;
use flea::ipc::wire::Reply;
use gpui::{Bounds, Pixels};

pub struct PathAdapter<'a> {
    pub view: &'a PathView,
}

// JavaScript's Math.round, which rounds a half up and never away from zero.
fn round(value: f32) -> i64 {
    (value + 0.5).floor() as i64
}

fn centre(x: f32, y: f32, width: f32, height: f32) -> String {
    format!("{} {}", round(x + width / 2.), round(y + height / 2.))
}

fn rect(bounds: Bounds<Pixels>) -> String {
    let (x, y) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let (left, top) = (round(x), round(y));
    let width = round(x + f32::from(bounds.size.width)) - left;
    let height = round(y + f32::from(bounds.size.height)) - top;
    format!("{left} {top} {width} {height}")
}

impl PathAdapter<'_> {
    /// A crumb's centre, or "" while its strip is hidden or the crumb lies outside it.
    fn crumb_centre(&self, slot: usize, index: Option<usize>) -> String {
        index
            .and_then(|index| self.crumb_box(slot, index))
            .map_or_else(String::new, |(x, y, width, height)| {
                centre(x, y, width, height)
            })
    }

    /// A crumb's painted box as x, y, width and height, None while its strip is hidden or the crumb lies outside it.
    pub(crate) fn crumb_box(&self, slot: usize, index: usize) -> Option<(f32, f32, f32, f32)> {
        let paint = self.view.paint.borrow();
        let bounds = paint.strips[slot]?;
        let (left, width) = self.view.strips[slot].extent(index)?;
        let pad = crate::path::paint::strip_pad(slot != 0, &paint.scale());
        let x = f32::from(bounds.origin.x) + pad + left;
        if x < f32::from(bounds.origin.x) || x + width > f32::from(bounds.right()) {
            return None;
        }
        Some((
            x,
            f32::from(bounds.origin.y),
            width,
            f32::from(bounds.size.height),
        ))
    }

    fn crumb_text(&self, slot: usize) -> String {
        self.view.strips[slot]
            .crumbs()
            .iter()
            .map(|crumb| crumb.text.as_str())
            .collect()
    }

    fn pane(&self, arguments: &[String]) -> Option<usize> {
        let side = arguments.first()?.parse::<usize>().ok()?;
        (side < 2 && self.view.inputs.dual).then_some(1 + side)
    }
}

impl Adapter for PathAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let view = self.view;
        let index = |at: usize| {
            arguments
                .get(at)
                .and_then(|value| value.parse::<usize>().ok())
        };
        let value = match function {
            "pathBarOpen" => view.editing().to_string(),
            "pathBarText" => view.entry.text.clone(),
            "pathSuggestionRows" => view.paint.borrow().drawn.to_string(),
            "pathSuggestionCursor" => view
                .jump
                .cursor
                .map_or_else(|| "-1".to_string(), |at| at.to_string()),
            "pathSuggestionPointer" => {
                let paint = view.paint.borrow();
                match (view.jump.shown(), paint.popup, view.pointer.last_seen()) {
                    (true, Some(popup), Some((x, y))) => {
                        format!("{} {}", round(x - popup.x), round(y - popup.y))
                    }
                    _ => "none".to_string(),
                }
            }
            "pathCentre" => view.paint.borrow().area.map_or_else(String::new, |area| {
                centre(
                    f32::from(area.origin.x),
                    f32::from(area.origin.y),
                    f32::from(area.size.width),
                    f32::from(area.size.height),
                )
            }),
            "pathFrameRect" => view
                .paint
                .borrow()
                .frame
                .filter(|_| view.built)
                .map_or_else(String::new, rect),
            "crumbCount" => view.strips[0].crumbs().len().to_string(),
            "crumbText" => self.crumb_text(0),
            "crumbCentre" if view.editing() || view.inputs.dual => String::new(),
            "crumbCentre" => self.crumb_centre(0, index(0)),
            // The nearest crumb drawn as that folder name ("~" is home), so a scene aims at a folder without counting crumbs.
            "crumbCentreOf" if view.editing() || view.inputs.dual => String::new(),
            "crumbCentreOf" => self.crumb_centre(
                0,
                arguments.first().and_then(|label| {
                    view.strips[0].crumbs().iter().rposition(|crumb| {
                        !crumb.elided
                            && (crumb.text == *label
                                || crumb.text.strip_suffix('/') == Some(label.as_str()))
                    })
                }),
            ),
            "elisionCentre" if view.editing() || view.inputs.dual => String::new(),
            "elisionCentre" => self.crumb_centre(
                0,
                view.strips[0]
                    .crumbs()
                    .iter()
                    .position(|crumb| crumb.elided),
            ),
            "paneCrumbCount" => self
                .pane(arguments)
                .map_or(0, |slot| view.strips[slot].crumbs().len())
                .to_string(),
            "paneCrumbText" => self
                .pane(arguments)
                .map_or_else(String::new, |slot| self.crumb_text(slot)),
            "paneCrumbCentre" => self
                .pane(arguments)
                .map_or_else(String::new, |slot| self.crumb_centre(slot, index(1))),
            "panePathRect" => self
                .pane(arguments)
                .and_then(|slot| view.paint.borrow().strips[slot])
                .map_or_else(String::new, rect),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
