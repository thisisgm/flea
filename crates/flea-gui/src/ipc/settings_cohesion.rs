//! ui/js/ControlReader.js over the open Settings card: the keyboard rests on the panel's own item, every drawn row is a SettingsRow.
use crate::g2::settings_pages::view::SettingsView;
use flea::{
    jsondoc::Json,
    ui_logic_settings_pages::rows::{Kind, Row},
};
use gpui::{Bounds, Pixels};

fn entry(
    kind: &str,
    bounds: Bounds<Pixels>,
    focused: bool,
    pointer: bool,
    enabled: bool,
    via: &str,
) -> Json {
    let number = |value: Pixels| Json::Num(f32::from(value).round().to_string());
    let flag = Json::Bool;
    let fields = [
        ("kind", Json::Str(kind.into())),
        ("name", Json::Str(String::new())),
        ("x", number(bounds.origin.x)),
        ("y", number(bounds.origin.y)),
        ("w", number(bounds.size.width)),
        ("h", number(bounds.size.height)),
        ("focused", flag(focused)),
        ("hovered", flag(false)),
        ("pressed", flag(false)),
        ("hoverable", flag(pointer)),
        ("pressable", flag(pointer)),
        ("enabled", flag(enabled)),
        ("value", Json::Str(String::new())),
        ("primary", flag(false)),
        ("destructive", flag(false)),
        ("member", flag(false)),
        ("via", Json::Str(via.into())),
    ];
    Json::Obj(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

// v0.3.12 SettingsRow.qml:54 greys a row that is unavailable or an inert check; its HoverHandler and TapHandler always stand.
fn row(row: &Row, view: &SettingsView) -> Option<Json> {
    let id = format!("row:{}", row.id);
    let probe = &view.probe;
    probe.bounds(&id)?;
    let enabled = row.available && !(row.kind == Kind::Check && row.inert);
    let (hovered, pressed) = probe.pointer(&id);
    let source = crate::visual::object([
        ("name", Json::Str(String::new())),
        ("rect", Json::Str(probe.rect(&id))),
        ("centre", Json::Str(probe.centre(&id, "pane"))),
        ("visible", Json::Bool(true)),
        ("enabled", Json::Bool(enabled)),
        ("hovered", Json::Bool(hovered)),
        ("pressed", Json::Bool(pressed)),
        ("value", Json::Str(format!("{:?}", row.kind))),
    ]);
    crate::ipc::cohesion_controls::control(&source, "SettingsPanel", "SettingsRow")
}

/// ui/Ipc.qml cohesionControls while Settings holds the keyboard: its keys item fills the window, so focus is that plain item.
pub(super) fn controls(view: &SettingsView) -> Option<Json> {
    let ground = view.probe.bounds("ground")?;
    let controls = view
        .panel
        .rows
        .iter()
        .filter_map(|r| row(r, view))
        .collect();
    Some(Json::Obj(vec![
        ("captureReader".into(), Json::Str("gpui-paint-v1".into())),
        (
            "focus".into(),
            entry("QQuickItem", ground, true, false, true, "item"),
        ),
        ("controls".into(), Json::Arr(controls)),
    ]))
}

pub(crate) fn sheet_controls(view: &crate::g2::settings_pages::SheetView, focused: bool) -> Json {
    let probe = &view.probe;
    let (hovered, pressed) = probe.pointer("body");
    let source = crate::visual::object([
        ("name", Json::Str("Keys body".into())),
        ("rect", Json::Str(probe.rect("body"))),
        ("centre", Json::Str(probe.centre("body", "card"))),
        ("visible", Json::Bool(true)),
        ("enabled", Json::Bool(true)),
        ("focused", Json::Bool(focused)),
        ("hovered", Json::Bool(hovered)),
        ("pressed", Json::Bool(pressed)),
    ]);
    crate::ipc::cohesion_controls::state(
        crate::ipc::cohesion_controls::control(&source, "KeymapSheet", "CardScroll")
            .into_iter()
            .collect(),
    )
}
pub(super) fn rail_width(
    view: Option<&crate::g2::settings_pages::view::SettingsView>,
    opened: bool,
) -> String {
    view.filter(|_| opened)
        .and_then(|view| view.probe.bounds("rail"))
        .map_or_else(String::new, |bounds| {
            f32::from(bounds.size.width).round().to_string()
        })
}
