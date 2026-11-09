//! The open card's permissionsState: v0.3.12's facts, edit state and controls; painted_state adds the painted bounds.
use super::{Permissions, Phase, Stop};
use crate::g2::open_with_properties::shell::Tri;
use flea::jsondoc::Json;

impl Permissions {
    /// Control names are v0.3.12's accessible names, so both frontends answer one contract.
    pub fn state(&self) -> Json {
        let editable = self.phase == Phase::Ready && self.changeable();
        let control = |name: String, enabled: bool, stop: Stop| {
            vec![
                ("name".to_string(), Json::Str(name)),
                ("enabled".to_string(), Json::Bool(enabled)),
                ("focused".to_string(), Json::Bool(self.focus == stop)),
            ]
        };
        let execute = if self.held.directory {
            "enter"
        } else {
            "execute"
        };
        let mut controls = Vec::new();
        if !self.several() {
            controls.push(Json::Obj(control("Octal".into(), editable, Stop::Octal)));
        }
        let cancel = self.phase != Phase::Applying;
        controls.push(Json::Obj(control("Cancel".into(), cancel, Stop::Cancel)));
        controls.push(Json::Obj(control(
            "Apply".into(),
            self.can_apply(),
            Stop::Apply,
        )));
        for (b, value) in self.boxes().into_iter().enumerate() {
            let who = ["Owner", "Group", "Everyone"][b / 3];
            let name = format!("{who} {}", ["read", "write", execute][b % 3]);
            let mut fields = control(name, editable, Stop::Box(b));
            fields.push(("checked".into(), Json::Bool(value == Tri::On)));
            fields.push(("mixed".into(), Json::Bool(value == Tri::Mixed)));
            fields.push(("bit".into(), Json::Num((1u32 << (8 - b)).to_string())));
            controls.push(Json::Obj(fields));
        }
        let text = |value: &str| Json::Str(value.to_string());
        let path = if self.several() { "" } else { &self.held.path };
        let busy = matches!(self.phase, Phase::Reading | Phase::Applying);
        Json::Obj(vec![
            ("opened".into(), Json::Bool(true)),
            ("inputReady".into(), Json::Bool(false)),
            ("path".into(), text(path)),
            ("mode".into(), text(&self.text)),
            (
                "paths".into(),
                Json::Arr(self.paths.iter().map(|p| text(p)).collect()),
            ),
            ("editable".into(), Json::Bool(editable)),
            ("busy".into(), Json::Bool(busy)),
            ("error".into(), text(&self.error)),
            ("controls".into(), Json::Arr(controls)),
        ])
    }
}
