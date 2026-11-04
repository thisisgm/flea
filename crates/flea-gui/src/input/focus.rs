//! A producer registers its real focus handle while drawn; absent surfaces never become tab stops.
use gpui::{FocusHandle, Window};

pub struct Stop {
    pub name: &'static str,
    pub handle: FocusHandle,
    pub context: flea::keys::Context,
}
#[derive(Default)]
pub struct Focus {
    pub blocked: bool,
    stops: Vec<Stop>,
}
impl Focus {
    pub fn register(&mut self, stop: Stop) {
        self.remove(stop.name);
        self.stops.push(stop);
        self.stops.sort_by_key(|stop| match stop.name {
            "rail" => 0,
            "chrome" => 1,
            "strip" => 2,
            _ => 3,
        });
    }
    pub fn remove(&mut self, name: &str) {
        self.stops.retain(|stop| stop.name != name);
    }
    pub fn current(&self, window: &Window) -> Option<&Stop> {
        self.stops
            .iter()
            .find(|stop| stop.handle.is_focused(window))
    }
    pub fn step(&self, current: &str, forward: bool) -> Option<&Stop> {
        if current == "list" {
            return if forward {
                self.stops.first()
            } else {
                self.stops.last()
            };
        }
        let at = self.stops.iter().position(|stop| stop.name == current)?;
        if forward {
            self.stops.get(at + 1)
        } else {
            at.checked_sub(1).and_then(|at| self.stops.get(at))
        }
    }
}
