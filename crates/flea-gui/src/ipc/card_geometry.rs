//! Card readers observe laid-out boxes and pointer events without carrying field contents.
use crate::g2::settings_pages::probe::Probe;
use flea::jsondoc::Json;
use gpui::{div, prelude::*, Div, MouseButton, Stateful};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Clone, Default)]
pub(crate) struct CardGeometry {
    pub probe: Probe,
    roles: Rc<RefCell<BTreeMap<String, String>>>,
    pointer: Rc<RefCell<BTreeMap<String, (bool, bool)>>>,
}
impl CardGeometry {
    pub fn clear(&self) {
        self.probe.clear();
        self.roles.borrow_mut().clear();
    }
    pub fn set_role(&self, id: &str, role: &str) {
        self.roles.borrow_mut().insert(id.into(), role.into());
    }
    pub fn role(&self, id: &str) -> String {
        if self.probe.bounds(id).is_none() {
            return String::new();
        }
        self.roles.borrow().get(id).cloned().unwrap_or_default()
    }
    pub fn track(&self, element: Stateful<Div>, id: impl Into<String>) -> Stateful<Div> {
        let id = id.into();
        let (hover, down, up, out) = (
            self.pointer.clone(),
            self.pointer.clone(),
            self.pointer.clone(),
            self.pointer.clone(),
        );
        let (a, b, c, d) = (id.clone(), id.clone(), id.clone(), id.clone());
        element
            .relative()
            .child(
                self.probe
                    .attach(div().absolute().left_0().top_0().size_full(), id),
            )
            .on_hover(move |over, _, _| {
                hover.borrow_mut().entry(a.clone()).or_default().0 = *over;
            })
            .on_mouse_down(MouseButton::Left, move |_, _, _| {
                down.borrow_mut().entry(b.clone()).or_default().1 = true;
            })
            .on_mouse_up(MouseButton::Left, move |_, _, _| {
                up.borrow_mut().entry(c.clone()).or_default().1 = false;
            })
            .on_mouse_up_out(MouseButton::Left, move |_, _, _| {
                out.borrow_mut().entry(d.clone()).or_default().1 = false;
            })
    }
    pub fn control(&self, id: &str, name: &str, focused: bool, enabled: bool) -> Json {
        let visible = self
            .probe
            .bounds(id)
            .is_some_and(|b| b.size.width > gpui::px(0.) && b.size.height > gpui::px(0.));
        let (hovered, pressed) = self.pointer.borrow().get(id).copied().unwrap_or_default();
        crate::visual::object([
            ("name", Json::Str(name.into())),
            ("visible", Json::Bool(visible)),
            ("enabled", Json::Bool(enabled)),
            ("focused", Json::Bool(focused)),
            ("hovered", Json::Bool(visible && hovered)),
            ("pressed", Json::Bool(visible && pressed)),
            ("ring", Json::Bool(focused && enabled)),
            ("centre", Json::Str(self.probe.centre(id, id))),
            ("rect", Json::Str(self.probe.rect(id))),
        ])
    }
}

pub(crate) fn text(value: impl Into<String>) -> Json {
    Json::Str(value.into())
}
pub(crate) fn number(value: impl ToString) -> Json {
    Json::Num(value.to_string())
}
pub(crate) fn render(value: Json) -> String {
    flea::jsondoc::render(&value).trim_end().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size, Bounds};

    #[test]
    fn vf_readers_absent_controls_do_not_report_pointer_state() {
        let geometry = CardGeometry::default();
        geometry.probe.record(
            "eye",
            Bounds::new(point(px(10.), px(10.)), size(px(20.), px(20.))),
        );
        geometry
            .pointer
            .borrow_mut()
            .insert("eye".into(), (true, true));
        let visible = geometry.control("eye", "eye", false, true);
        for key in ["visible", "hovered", "pressed"] {
            assert_eq!(
                visible.get(key).and_then(Json::as_bool),
                Some(true),
                "{key}"
            );
        }
        geometry.clear();
        let absent = geometry.control("eye", "eye", false, true);
        for key in ["visible", "hovered", "pressed"] {
            assert_eq!(
                absent.get(key).and_then(Json::as_bool),
                Some(false),
                "{key}"
            );
        }
    }
}
