//! Cohesion records project the current owner's painted controls into the capture vocabulary.
use super::Adapter;
use flea::ipc::wire::Reply;
use flea::jsondoc::Json;

pub(crate) struct Owners<'a> {
    sources: &'a [&'a dyn Adapter],
    overlays: &'a [&'a dyn Adapter],
    sheet: Option<(&'a crate::g2::settings_pages::SheetView, bool)>,
    shell: &'a crate::state::Shell,
    cx: &'a gpui::App,
    window: &'a gpui::Window,
}

impl<'a> Owners<'a> {
    pub fn new(
        sources: &'a [&'a dyn Adapter],
        overlays: &'a [&'a dyn Adapter],
        shell: &'a crate::state::Shell,
        window: &'a gpui::Window,
        cx: &'a gpui::App,
    ) -> Self {
        let sheet = shell.settings.sheet.as_ref().map(|entity| {
            let view = entity.read(cx);
            (view, view.focus.contains_focused(window, cx))
        });
        Self {
            sources,
            overlays,
            sheet,
            shell,
            cx,
            window,
        }
    }
}

impl Adapter for Owners<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        if matches!(function, "langmarksState" | "langmarksCode") {
            return super::langmarks::read(self.shell, self.cx, function, arguments);
        }
        if function != "cohesionControls" {
            return None;
        }
        if let Some((view, focused)) = self.sheet {
            return Some(Reply::value(
                flea::jsondoc::render(&super::settings::cohesion::sheet_controls(view, focused))
                    .trim_end(),
            ));
        }
        for (reader, surface) in [
            ("permissionsState", "PermissionsDialog"),
            ("convertState", "ConvertDialog"),
            ("openWithState", "OpenWithDialog"),
            ("menuDialogState", "MenuActionDialog"),
            ("collideState", "CollideConfirm"),
        ] {
            let Some(source) = self
                .sources
                .iter()
                .find_map(|adapter| adapter.call(reader, &[]))
            else {
                continue;
            };
            if source.code != 0 {
                continue;
            }
            let Ok(mut source) = flea::jsondoc::parse(source.stdout.trim()) else {
                continue;
            };
            if !flag(&source, "opened") {
                continue;
            }
            let surface =
                if reader == "menuDialogState" && field(&source, "action") == "deletePermanently" {
                    if let Some(confirm) = source.get("confirmation") {
                        let controls = ["cancel", "danger"]
                            .iter()
                            .filter_map(|key| confirm.get(key).cloned())
                            .collect();
                        if let Json::Obj(fields) = &mut source {
                            fields.push(("controls".into(), Json::Arr(controls)));
                        }
                    }
                    "TrashConfirm"
                } else {
                    surface
                };
            return Some(Reply::value(
                flea::jsondoc::render(&from_controls(&source, surface)).trim_end(),
            ));
        }
        if let Some(reply) = self
            .overlays
            .iter()
            .find_map(|owner| owner.call(function, arguments))
        {
            return Some(reply);
        }
        super::cohesion_surfaces::read(self.shell, self.window, self.cx)
            .map(|value| Reply::value(flea::jsondoc::render(&value).trim_end()))
    }
}

fn field(value: &Json, key: &str) -> String {
    value
        .get(key)
        .and_then(Json::as_str)
        .unwrap_or_default()
        .into()
}

fn flag(value: &Json, key: &str) -> bool {
    value.get(key).and_then(Json::as_bool) == Some(true)
}

/// Missing geometry is absent, never replaced by a desired control position.
pub(crate) fn control(source: &Json, surface: &str, kind: &str) -> Option<Json> {
    // Sample input: {"rect":"232 96 640 28","centre":"552 110"}.
    let rect: Vec<f32> = field(source, "rect")
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let [x, y, w, h] = rect.as_slice() else {
        return None;
    };
    let centre: Vec<f32> = field(source, "centre")
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let [cx, cy] = centre.as_slice() else {
        return None;
    };
    let number = |v: f32| Json::Num(v.round().to_string());
    let hover = source
        .get("hoverLive")
        .and_then(Json::as_bool)
        .unwrap_or(source.get("hovered").is_some());
    let press = source
        .get("pressLive")
        .and_then(Json::as_bool)
        .unwrap_or(source.get("pressed").is_some());
    Some(crate::visual::object([
        ("surface", Json::Str(surface.into())),
        ("kind", Json::Str(kind.into())),
        (
            "name",
            Json::Str({
                let label = field(source, "label");
                if label.is_empty() || kind != "DialogButton" {
                    field(source, "name")
                } else {
                    label
                }
            }),
        ),
        ("x", number(*x)),
        ("y", number(*y)),
        ("w", number(*w)),
        ("h", number(*h)),
        ("targetX", number(*cx)),
        ("targetY", number(*cy)),
        ("enabled", Json::Bool(flag(source, "enabled"))),
        (
            "focused",
            Json::Bool(flag(source, "focused") || flag(source, "ring")),
        ),
        ("hovered", Json::Bool(flag(source, "hovered"))),
        ("pressed", Json::Bool(flag(source, "pressed"))),
        ("hoverable", Json::Bool(hover)),
        ("hoverLive", Json::Bool(hover)),
        ("pressable", Json::Bool(press)),
        ("pressLive", Json::Bool(press)),
        (
            "exposed",
            Json::Bool(flag(source, "visible") && *w > 0. && *h > 0.),
        ),
        ("primary", Json::Bool(flag(source, "primary"))),
        ("destructive", Json::Bool(flag(source, "destructive"))),
        ("member", Json::Bool(flag(source, "member"))),
        ("value", Json::Str(field(source, "value"))),
    ]))
}

pub(crate) fn state(controls: Vec<Json>) -> Json {
    let focus = controls
        .iter()
        .find(|c| flag(c, "focused"))
        .cloned()
        .unwrap_or(Json::Null);
    crate::visual::object([
        ("captureReader", Json::Str("gpui-paint-v1".into())),
        ("focus", focus),
        ("controls", Json::Arr(controls)),
    ])
}

pub(crate) fn from_controls(source: &Json, surface: &str) -> Json {
    let controls = source
        .get("controls")
        .or_else(|| source.get("buttons"))
        .and_then(Json::as_array)
        .unwrap_or(&[]);
    let entries: Vec<_> = controls
        .iter()
        .filter(|row| field(row, "name") != "Applications")
        .filter_map(|row| {
            let supplied = field(row, "kind");
            let name = field(row, "name");
            let kind = if !supplied.is_empty() {
                supplied.as_str()
            } else {
                match name.as_str() {
                    "Field" | "Octal" => "DialogField",
                    "Always" | "Remove metadata" | "Combine into one PDF" => "CheckBox",
                    _ => "DialogButton",
                }
            };
            control(row, surface, kind)
        })
        .collect();
    let mut result = state(entries);
    let list_focus = controls
        .iter()
        .filter(|row| field(row, "name") == "Applications")
        .chain(
            source
                .get("formats")
                .and_then(Json::as_array)
                .unwrap_or(&[]),
        )
        .find(|row| flag(row, "focused"))
        .and_then(|row| control(row, surface, "QQuickItem"));
    if let (Some(focus), Json::Obj(fields)) = (list_focus, &mut result) {
        if let Some((_, value)) = fields.iter_mut().find(|(key, _)| key == "focus") {
            *value = focus;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_focus_does_not_replace_button_roles_in_pointer_inventory() {
        let source = flea::jsondoc::parse(r#"{"controls":[{"name":"Submit","rect":"10 20 80 30","centre":"50 35","visible":true,"enabled":false,"primary":true}],"formats":[{"name":"PNG","focused":true,"rect":"10 60 80 30","centre":"50 75","visible":true,"enabled":true}]}"#).unwrap();
        let state = from_controls(&source, "ConvertDialog");
        let controls = state.get("controls").and_then(Json::as_array).unwrap();
        assert_eq!(controls.len(), 1);
        assert!(flag(&controls[0], "primary"));
        assert!(!flag(&controls[0], "enabled"));
        assert_eq!(
            state
                .get("focus")
                .unwrap()
                .get("name")
                .and_then(Json::as_str),
            Some("PNG")
        );
    }

    #[test]
    fn missing_painted_geometry_never_becomes_a_target() {
        let source = flea::jsondoc::parse(r#"{"name":"Open","focused":true}"#).unwrap();
        assert!(control(&source, "OpenWithDialog", "DialogButton").is_none());
    }

    #[test]
    fn owner_pointer_and_clipped_target_are_preserved() {
        let source = flea::jsondoc::parse(r#"{"name":"Open","rect":"10 20 80 30","centre":"30 25","visible":true,"enabled":false,"focused":true,"hovered":true,"pressed":false}"#).unwrap();
        let row = control(&source, "OpenWithDialog", "DialogButton").unwrap();
        assert_eq!(
            row.get("surface").and_then(Json::as_str),
            Some("OpenWithDialog")
        );
        assert_eq!(row.get("targetX").and_then(Json::as_f64), Some(30.));
        assert!(!flag(&row, "enabled"));
        assert!(flag(&row, "hovered"));
        assert!(!flag(&row, "pressed"));
        let state = state(vec![row]);
        assert_eq!(
            state
                .get("focus")
                .unwrap()
                .get("name")
                .and_then(Json::as_str),
            Some("Open")
        );
    }
}
