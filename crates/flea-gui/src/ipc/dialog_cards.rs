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

// An open card's painted bounds are not recorded, so its reader refuses instead of inventing them.
fn open_card(function: &str, fields: &str) -> Reply {
    Reply::error(
        3,
        format!("FLEA_IPC_ERROR flea.{function} unavailable fields: {fields} (GPUI does not record the card's painted bounds)"),
    )
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
            // The menu actions' card is Open With, the run card, Delete permanently's review, or Copy To, Move To and New File's field card; with none open the Root has no dialog to describe.
            "menuDialogState" => {
                if let Some(state) = plugin_input_state(self.shell) {
                    return Some(Reply::value(state));
                }
                if let Some(open) = &self.shell.properties {
                    return Some(Reply::value(
                        open.card.ipc_state(&self.shell.locale.catalog, &open.probe),
                    ));
                }
                if self.shell.open_with.is_some() || self.shell.run_card.is_some() {
                    return Some(open_card(function, "rect, controls, confirmation"));
                }
                self.shell
                    .overlay
                    .as_ref()
                    .map(|overlay| overlay.read(self.cx))
                    .and_then(|overlay| {
                        menu_delete_state(overlay).or_else(|| menu_field_state(overlay))
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
                ),
                None => "{\"opened\":false}".into(),
            },
            "runCardState" => self.shell.run_card.as_ref().map_or_else(
                || "{\"opened\":false}".into(),
                |open| {
                    open.card
                        .ipc_state(&self.shell.locale.catalog, &open.probe.probe)
                },
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
const OPEN_WITH_UNAVAILABLE: &str = "controls[].visible, controls[].hovered, controls[].pressed, controls[].centre, controls[].rect";

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

fn open_with_state(card: &OpenWith, painted: &str, i18n: &flea::i18n::I18n) -> String {
    let controls = [
        ("Field", Stop::Search, true),
        ("Applications", Stop::List, true),
        ("Always", Stop::Always, true),
        ("Cancel", Stop::Cancel, true),
        ("Open", Stop::Open, card.can_open()),
    ]
    .map(|(name, stop, enabled)| {
        format!(
            r#"{{"name":"{name}","focused":{},"enabled":{enabled}}}"#,
            card.focus == stop
        )
    })
    .join(",");
    let catalogue = card.catalogue();
    let fact = |pick: fn(&Catalogue) -> &str| flea::json::escape(catalogue.map_or("", pick));
    format!(
        r#"{{"opened":true,"busy":{},"committing":{},"kind":"{}","mime":"{}","name":"{}","always":{},"error":"{}","search":"{}","cursor":{},"rows":[{}],"controls":[{controls}],{painted},"unavailable":"{OPEN_WITH_UNAVAILABLE}"}}"#,
        catalogue.is_none() && card.error().is_none(),
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
