//! Dialog readers distinguish observable closed cards from unavailable window owners.
use super::menu_cards::{menu_delete_state, menu_field_state};
use super::Adapter;
use crate::visual::{number, object};
use crate::{
    g2::open_with_properties::openwith::{OpenWith, Stop},
    state::Shell,
};
use flea::ipc::wire::Reply;
use flea::jsondoc::Json;
use flea::ui_logic_open_with_properties::openwith::{self as logic, Catalogue, Row as Seat};
use gpui::App;

pub(crate) struct DialogAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
    /// The list holds the keyboard and no listing is in flight, the rest state of Pane.qml's permissions inputReady.
    pub input_ready: bool,
}

impl Adapter for DialogAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        let value = match function {
            "dialogOpen" => self
                .shell
                .rail
                .as_ref()
                .and_then(|rail| rail.network.as_ref())
                .is_some_and(|network| network.card.read(self.cx).model.opened)
                .to_string(),
            "unlockState" => self
                .shell
                .rail
                .as_ref()
                .and_then(|rail| rail.unlock.as_ref())
                .and_then(|unlock| unlock.view.read(self.cx).reading())
                .unwrap_or_else(|| "{\"opened\":false}".into()),
            // The menu actions' card is Open With, the run card, Copy To or Move To, Delete permanently's review, or New File's field card; with none open the Root has no dialog to describe.
            "menuDialogState" => {
                if let Some(state) = plugin_input_state(self.shell) {
                    return Some(Reply::value(state));
                }
                if let Some(open) = &self.shell.properties {
                    return Some(Reply::value(
                        open.card.ipc_state(&self.shell.locale.catalog, &open.probe),
                    ));
                }
                if self.shell.open_with.is_some() {
                    return self.call("openWithState", &[]);
                }
                if self.shell.run_card.is_some() {
                    return self.call("runCardState", &[]);
                }
                self.shell
                    .overlay
                    .as_ref()
                    .and_then(|overlay| {
                        let overlay = overlay.read(self.cx);
                        super::copy_to_card::menu_copy_to_state(overlay)
                            .or_else(|| menu_delete_state(overlay))
                            .or_else(|| menu_field_state(overlay))
                    })
                    .unwrap_or_else(|| "{\"opened\":false}".into())
            }
            // The card's model facts and painted parts the design CI drives; every other ui/Ipc.qml field is named under unavailable, as a refusal would.
            "openWithState" => match &self.shell.open_with {
                Some(open) => open_with_state(
                    &open.card,
                    &super::open_with_painted::fields(
                        open,
                        f32::from(self.shell.active_typeface().body)
                            / crate::g2::open_with_properties::shell::BOARD_BODY,
                        &self.shell.locale.catalog,
                    ),
                    &self.shell.locale.catalog,
                    &open.frame,
                ),
                None => "{\"opened\":false}".into(),
            },
            "runCardState" => self.shell.run_card.as_ref().map_or_else(
                || "{\"opened\":false}".into(),
                |open| open.card.ipc_state(&self.shell.locale.catalog, &open.probe),
            ),
            // An open card answers its own state; a closed one only the list's rest input state.
            "permissionsState" => match self.shell.permissions_state(self.cx) {
                Some(state) => flea::jsondoc::render(&state).trim_end().to_owned(),
                None => format!("{{\"opened\":false,\"inputReady\":{}}}", self.input_ready),
            },
            _ => return None,
        };
        Some(Reply::value(value))
    }
}

fn plugin_input_state(shell: &Shell) -> Option<String> {
    let open = shell.plugins.input.as_ref()?;
    let entry = &shell.rename.ime.entry;
    let state = object([
        ("opened", Json::Bool(true)),
        ("kind", Json::Str("pluginInput".into())),
        ("value", Json::Str(open.value.clone())),
        ("editorValue", Json::Str(entry.text.clone())),
        (
            "selection",
            Json::Arr(vec![
                number(entry.selected.start),
                number(entry.selected.end),
            ]),
        ),
        ("caret", number(entry.cursor())),
        ("offsetUnit", Json::Str("utf8-bytes".into())),
        ("unavailable", Json::Str("rect, controls".into())),
    ]);
    Some(flea::jsondoc::render(&state).trim_end().to_owned())
}

/// The ui/Ipc.qml openWithState fields this reader does not send: the controls' painted state.
const OPEN_WITH_UNAVAILABLE: &str = "";

/// ui/Ipc.qml's row shapes: an eyebrow with its rule, or an application with its default claim.
fn open_with_rows(card: &OpenWith, i18n: &flea::i18n::I18n) -> String {
    let Some(catalogue) = card.catalogue() else {
        return String::new();
    };
    let text = |value: &str| flea::json::escape(value);
    card.rows()
        .iter()
        .map(|row| match row {
            Seat::Eyebrow { group, rule } => format!(
                r#"{{"eyebrow":"{}","rule":{rule}}}"#,
                text(&logic::eyebrow(i18n, *group, &catalogue.kind))
            ),
            Seat::App { app, group, .. } => {
                let app = catalogue.app(*group, *app).cloned().unwrap_or_default();
                format!(
                    r#"{{"id":"{}","label":"{}","icon":"{}","isDefault":{}}}"#,
                    text(&app.id),
                    text(&app.label),
                    text(&app.icon),
                    app.default
                )
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn open_with_state(
    card: &OpenWith,
    painted: &str,
    i18n: &flea::i18n::I18n,
    frame: &crate::g2::file_ops::measured::Frame,
) -> String {
    use crate::g2::file_ops::measured;
    let controls = [
        ("Field", Stop::Search, true, "field"),
        ("Applications", Stop::List, true, "applications"),
        ("Always", Stop::Always, true, "check:0"),
        ("Cancel", Stop::Cancel, true, "button:0"),
        ("Open", Stop::Open, card.can_open(), "button:1"),
    ]
    .map(|(name, stop, enabled, id)| {
        let part = frame.part(id);
        let visible = part.as_ref().is_some_and(|part| {
            part.bounds.size.width > gpui::px(0.) && part.bounds.size.height > gpui::px(0.)
        });
        let (hovered, pressed) = frame.pointer(id);
        format!(
            r#"{{"name":"{name}","focused":{},"primary":{},"enabled":{enabled},"visible":{visible},"hovered":{hovered},"pressed":{pressed},"pressLive":{},"centre":"{}","rect":"{}"}}"#,
            card.focus == stop,
            stop == Stop::Open,
            !matches!(stop, Stop::Search | Stop::List),
            measured::centre(part.as_ref()),
            measured::rect(part.as_ref()),
        )
    })
    .join(",");
    let catalogue = card.catalogue();
    let fact = |pick: fn(&Catalogue) -> &str| flea::json::escape(catalogue.map_or("", pick));
    format!(
        r#"{{"opened":true,"busy":{},"committing":{},"kind":"{}","mime":"{}","name":"{}","always":{},"error":"{}","search":"{}","cursor":{},"rows":[{}],"controls":[{controls}],{painted},"unavailable":"{OPEN_WITH_UNAVAILABLE}"}}"#,
        card.opening() || catalogue.is_none() && card.error().is_none(),
        card.opening(),
        fact(|c| &c.kind),
        fact(|c| &c.mime),
        flea::json::escape(card.name()),
        card.always,
        flea::json::escape(card.error().unwrap_or("")),
        flea::json::escape(&card.query),
        card.cursor,
        open_with_rows(card, i18n)
    )
}
