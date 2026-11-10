use super::{Properties, Shown};
use crate::ipc::card_geometry::{render, text, CardGeometry};
use flea::{i18n::I18n, jsondoc::Json};
impl Properties {
    fn facts_json(&self) -> Json {
        use crate::ipc::card_geometry::number;
        match &self.shown {
            Shown::One(facts, _) => crate::visual::object([
                ("ok", Json::Bool(true)),
                ("path", text(facts.path.clone())),
                ("kind", text(facts.kind.clone())),
                ("directory", Json::Bool(facts.directory)),
                ("symlink", Json::Bool(facts.symlink)),
                ("target", text(facts.target.clone())),
                ("bytes", number(facts.bytes)),
                ("modified", number(facts.modified)),
                ("mode", text(facts.mode.clone())),
                ("owner", text(facts.owner.clone())),
                ("uid", number(facts.uid)),
                ("gid", number(facts.gid)),
                (
                    "tags",
                    Json::Arr(facts.tags.iter().cloned().map(text).collect()),
                ),
                ("photo", facts.photo.clone().unwrap_or(Json::Null)),
            ]),
            Shown::Several(facts) => crate::visual::object([
                ("ok", Json::Bool(true)),
                ("count", number(self.count)),
                ("kinds", text(facts.kinds.clone())),
                ("bytes", number(facts.bytes)),
                ("newest", text(facts.newest.clone())),
                ("oldest", text(facts.oldest.clone())),
                ("partial", Json::Bool(facts.partial)),
                ("done", Json::Bool(facts.done)),
            ]),
            _ => Json::Obj(vec![]),
        }
    }
    fn lines_json(&self, i18n: &I18n) -> Json {
        let Shown::One(facts, walk) = &self.shown else {
            return Json::Obj(vec![]);
        };
        let line = |line: flea::ui_logic_open_with_properties::properties::Line| {
            crate::visual::object([("value", text(line.value)), ("tail", text(line.tail))])
        };
        crate::visual::object([
            (
                "size",
                line(flea::ui_logic_open_with_properties::properties::size(
                    i18n,
                    facts.bytes,
                    *walk,
                )),
            ),
            (
                "contains",
                flea::ui_logic_open_with_properties::properties::contains(i18n, *walk)
                    .map_or(Json::Null, line),
            ),
        ])
    }

    pub(crate) fn ipc_state(&self, i18n: &I18n, probe: &CardGeometry) -> String {
        let error = match &self.shown {
            Shown::Refused(error) => error.as_str(),
            _ => "",
        };
        render(crate::visual::object([
            ("opened", Json::Bool(!self.closed)),
            ("action", text("properties")),
            ("busy", Json::Bool(matches!(self.shown, Shown::Asking))),
            ("committing", Json::Bool(false)),
            ("error", text(error)),
            ("facts", self.facts_json()),
            ("liveLines", self.lines_json(i18n)),
            ("applications", Json::Arr(vec![])),
            ("cursor", crate::ipc::card_geometry::number(0)),
            ("rect", text(probe.probe.rect("card"))),
            (
                "controls",
                Json::Arr(vec![probe.control("button:0", "Close", true, true)]),
            ),
            ("confirmation", Json::Null),
        ]))
    }
}
