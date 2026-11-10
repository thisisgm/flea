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
                ("primary".to_string(), Json::Bool(stop == Stop::Apply)),
                (
                    "kind".to_string(),
                    Json::Str(
                        match stop {
                            Stop::Box(_) => "CheckBox",
                            Stop::Octal => "DialogField",
                            _ => "DialogButton",
                        }
                        .into(),
                    ),
                ),
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
            // v0.3.12's box words: a mixed box reads "some".
            let word = match value {
                Tri::On => "on",
                Tri::Off => "off",
                Tri::Mixed => "some",
            };
            fields.push(("value".into(), Json::Str(word.into())));
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
            ("facts".into(), self.facts()),
        ])
    }

    /// What the inspect reply said about the one item; several items answer only whether every item was read.
    fn facts(&self) -> Json {
        let held = &self.held;
        let read = self.phase != Phase::Reading;
        if self.several() {
            let ok = read && self.store.pending == 0 && self.store.skipped.is_empty();
            return Json::Obj(vec![("ok".into(), Json::Bool(ok))]);
        }
        let text = |value: &str| Json::Str(value.to_string());
        Json::Obj(vec![
            ("ok".into(), Json::Bool(read && !held.path.is_empty())),
            ("path".into(), text(&held.path)),
            ("directory".into(), Json::Bool(held.directory)),
            ("mode".into(), text(&held.mode)),
            ("owner".into(), text(&held.owner)),
            ("group".into(), text(&held.group)),
            ("uid".into(), Json::Num(held.uid.to_string())),
            ("gid".into(), Json::Num(held.gid.to_string())),
            ("reason".into(), text(&held.reason)),
        ])
    }
}
