//! Toolbar readers observe the geometry and pointer state of the controls the live view drew.
use super::Adapter;
use crate::path::chrome::{Frame, GLYPHS};
use flea::{ipc::wire::Reply, json::escape};
use gpui::{Bounds, Pixels};

pub struct ChromeAdapter<'a> {
    pub frame: &'a Frame,
}

fn round(value: f32) -> i64 {
    (value + 0.5).floor() as i64
}

fn centre(bounds: Bounds<Pixels>) -> String {
    format!(
        "{} {}",
        round(f32::from(bounds.origin.x + bounds.size.width / 2.)),
        round(f32::from(bounds.origin.y + bounds.size.height / 2.))
    )
}

fn rect(bounds: Bounds<Pixels>) -> String {
    let x = round(f32::from(bounds.origin.x));
    let y = round(f32::from(bounds.origin.y));
    format!(
        "{x} {y} {} {}",
        round(f32::from(bounds.right())) - x,
        round(f32::from(bounds.bottom())) - y
    )
}

impl Adapter for ChromeAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        if function == "chromeHeight" {
            return Some(Reply::value(
                self.frame
                    .bounds
                    .map_or(0, |bounds| round(f32::from(bounds.size.height)))
                    .to_string(),
            ));
        }
        if !matches!(
            function,
            "chromeButtonCentre" | "chromeButtonState" | "chromeButtonSize"
        ) {
            return None;
        }
        let name = arguments.first().map_or("", String::as_str);
        let control = GLYPHS
            .iter()
            .position(|glyph| *glyph == name)
            .map(|index| self.frame.controls[index])
            .unwrap_or_default();
        let bounds = control.bounds.filter(|_| control.visible);
        // ui/Ipc.qml chromeButtonSize: the drawn button's "widthxheight", each side rounded on its own.
        if function == "chromeButtonSize" {
            return Some(Reply::value(bounds.map_or_else(String::new, |b| {
                format!(
                    "{}x{}",
                    round(f32::from(b.size.width)),
                    round(f32::from(b.size.height))
                )
            })));
        }
        let centre = bounds.map_or_else(String::new, centre);
        Some(Reply::value(if function == "chromeButtonCentre" {
            centre
        } else {
            format!("{{\"name\":\"{}\",\"visible\":{},\"enabled\":{},\"focused\":{},\"hovered\":{},\"pressed\":{},\"centre\":\"{centre}\",\"rect\":\"{}\"}}", escape(name), control.visible && bounds.is_some(), control.enabled, control.focused, control.hovered, control.pressed, bounds.map_or_else(String::new, rect))
        }))
    }
}
