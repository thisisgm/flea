//! Read-only settings and keyboard-sheet observations from the mounted producers.
use super::Adapter;
use crate::{
    g2::settings_pages::{panel::Side, sheet::Sheet},
    window::settings::Host,
};
use flea::{
    ipc::wire::Reply,
    jsondoc::{self, Json},
    ui_logic_settings_pages::{
        pages::SECTIONS,
        rows::{Kind, Role},
    },
};
use gpui::App;
#[path = "settings_cohesion.rs"]
mod cohesion;
#[path = "settings_plugins.rs"]
mod plugins;
#[path = "settings_rows.rs"]
mod rows;
use rows::row_model;

pub(crate) struct SettingsAdapter<'a> {
    pub host: &'a Host,
    pub document: &'a Json,
    pub hidden: bool,
    pub cx: &'a App,
}

fn object(fields: impl IntoIterator<Item = (&'static str, Json)>) -> Json {
    Json::Obj(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn word(text: impl Into<String>) -> Json {
    Json::Str(text.into())
}
fn kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Group => "group",
        Kind::Hint => "hint",
        Kind::Fact => "fact",
        Kind::Lock => "lock",
        Kind::Check => "check",
        Kind::Choice => "choice",
        Kind::Action => "action",
        Kind::Favourite => "favourite",
        Kind::Ruler => "ruler",
        Kind::KeyPreview => "keyPreview",
        Kind::Hero => "hero",
    }
}
fn role(role: Role) -> &'static str {
    match role {
        Role::None => "muted",
        Role::Live => "live",
        Role::Accent => "accent",
        Role::Error => "error",
        Role::Foreground => "foreground",
    }
}

fn results(sheet: &Sheet) -> Json {
    Json::Arr(
        sheet
            .ranked
            .iter()
            .map(|row| {
                let place = row.where_.resolve(&sheet.catalog);
                object([
                    ("keys", word(&row.keys)),
                    ("label", word(row.label.resolve(&sheet.catalog))),
                    (
                        "where",
                        word(if place.is_empty() {
                            place
                        } else {
                            sheet.catalog.tr_args("settings.sheet.in", &[&place])
                        }),
                    ),
                    ("disabled", Json::Bool(row.disabled)),
                ])
            })
            .collect(),
    )
}
pub(crate) fn sheet_rows(sheet: &Sheet) -> String {
    if !sheet.query.is_empty() {
        return sheet
            .ranked
            .iter()
            .map(|row| {
                format!(
                    "{} {}{}",
                    row.keys,
                    row.label.resolve(&sheet.catalog),
                    if row.disabled { " (disabled)" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    let mut out = Vec::new();
    for (_, group, rows) in sheet.groups() {
        out.push(sheet.catalog.tr(group.label_key));
        out.extend(rows.iter().map(|row| format!("{} {}", row.keys, row.label)));
    }
    out.join("\n")
}

// The pane's scrolled content: a standing search's results, otherwise the section's rows.
fn pane_content(panel: &crate::g2::settings_pages::Panel) -> f32 {
    if panel
        .query
        .as_deref()
        .is_some_and(|query| !query.is_empty())
    {
        panel.results_height
    } else {
        panel.heights().iter().sum()
    }
}

impl Adapter for SettingsAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        // A favourites write is saving while a favourite move is in flight, or a queued or in-flight settings change names places.favourites.
        if function == "favouritesSaving" {
            let saving = !self.host.favourite_writes.is_empty()
                || self.host.pending.iter().any(|(_, change)| {
                    change
                        .get("places")
                        .and_then(|places| places.get("favourites"))
                        .is_some()
                });
            return Some(Reply::value(saving.to_string()));
        }
        let panel = self
            .host
            .panel
            .as_ref()
            .map(|view| &view.read(self.cx).panel);
        let view = self.host.panel.as_ref().map(|view| view.read(self.cx));
        let sheet = self
            .host
            .sheet
            .as_ref()
            .map(|view| &view.read(self.cx).sheet);
        let json = |value: Json| jsondoc::render(&value).trim_end().to_owned();
        if panel.is_some_and(|panel| panel.section == "plugins") {
            if let Some(plugin) = self
                .host
                .plugins
                .pane
                .as_ref()
                .map(|view| view.read(self.cx))
            {
                let value = match function {
                    "settingsCursor" => {
                        Some(plugin.page().cursor().map_or(-1, |i| i as i64).to_string())
                    }
                    "settingsRows" => Some(
                        plugin
                            .page()
                            .rows()
                            .iter()
                            .map(|row| plugins::text(row, plugin.page().catalog()))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ),
                    "settingsModel" => Some(json(Json::Arr(
                        plugin
                            .page()
                            .rows()
                            .iter()
                            .map(|row| plugins::model(row, plugin.page().catalog()))
                            .collect(),
                    ))),
                    "settingsRowCentre" => Some(if self.host.opened {
                        plugin
                            .probe
                            .centre(&format!("row:{}", arguments.first()?), "pane")
                    } else {
                        String::new()
                    }),
                    _ => None,
                };
                if let Some(value) = value {
                    return Some(Reply::value(value));
                }
            }
        }
        let value = match function {
            "settingsOpen" => self.host.opened.to_string(),
            "settingsRailWidth" => cohesion::rail_width(view, self.host.opened),
            "settingsSection" => panel.map_or("", |p| p.section).into(),
            "settingsSide" => panel
                .map_or("", |p| match p.side {
                    Side::Rail => "rail",
                    Side::Pane => "pane",
                })
                .into(),
            "settingsCursor" => panel.map_or(-1, |p| p.cursor as i64).to_string(),
            "settingsRows" => panel.map_or_else(String::new, |p| {
                p.rows
                    .iter()
                    .map(|row| {
                        format!(
                            "{}|{}|{}",
                            kind(row.kind),
                            row.label.resolve(&p.catalog),
                            row.value.resolve(&p.catalog)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }),
            "settingsModel" => json(Json::Arr(panel.map_or_else(Vec::new, |p| {
                p.rows.iter().map(|r| row_model(r, &p.catalog)).collect()
            }))),
            "settingsSections" => json(Json::Arr(panel.map_or_else(Vec::new, |p| {
                SECTIONS
                    .iter()
                    .map(|s| object([("id", word(s.id)), ("label", word(p.catalog.tr(s.label)))]))
                    .collect()
            }))),
            // SettingsPanel.paneScroll and scrollState: the pane's content and viewport as the last frame laid them out.
            "settingsScroll" => panel
                .filter(|_| self.host.opened)
                .map_or_else(String::new, |p| {
                    format!("{}|{}", pane_content(p).round(), p.pane.1.round())
                }),
            "settingsScrollState" => match (panel, view) {
                (Some(p), Some(v)) if self.host.opened => {
                    let rail = v.probe.bounds("rail");
                    let rail_bottom = SECTIONS
                        .iter()
                        .filter_map(|section| v.probe.bounds(&format!("rail:{}", section.id)))
                        .map(|b| f32::from(b.bottom()))
                        .reduce(f32::max);
                    let (rail_height, rail_content) = rail.map_or((0., 0.), |b| {
                        let top = f32::from(b.top());
                        (
                            f32::from(b.size.height),
                            rail_bottom.map_or(0., |bottom| bottom - top),
                        )
                    });
                    let scroll = if p.query.as_deref().is_some_and(|q| !q.is_empty()) {
                        p.results_scroll
                    } else {
                        p.scroll
                    };
                    json(object([
                        ("compactHeight", Json::Num(p.view_height.to_string())),
                        (
                            "pane",
                            object([
                                ("y", Json::Num(scroll.to_string())),
                                ("height", Json::Num(p.pane.1.to_string())),
                                ("contentHeight", Json::Num(pane_content(p).to_string())),
                            ]),
                        ),
                        (
                            "rail",
                            object([
                                ("y", Json::Num("0".into())),
                                ("height", Json::Num(rail_height.to_string())),
                                ("contentHeight", Json::Num(rail_content.to_string())),
                            ]),
                        ),
                    ]))
                }
                _ => "{}".into(),
            },
            "cohesionControls" if self.host.opened && sheet.is_none() => {
                json(view.and_then(cohesion::controls)?)
            }
            "settingsCardRect" => view
                .filter(|_| self.host.opened)
                .map_or_else(String::new, |v| v.probe.rect("card")),
            "settingsTitleCentre" => view
                .filter(|_| self.host.opened)
                .map_or_else(String::new, |v| v.probe.centre("title", "card")),
            "settingsRowCentre" | "settingsRailRowCentre" => {
                let id = arguments.first()?;
                let key = format!(
                    "{}:{id}",
                    if function == "settingsRowCentre" {
                        "row"
                    } else {
                        "rail"
                    }
                );
                let clip = if function == "settingsRowCentre" {
                    "pane"
                } else {
                    "card"
                };
                view.filter(|_| self.host.opened)
                    .map_or_else(String::new, |v| v.probe.centre(&key, clip))
            }
            "settingsFavouriteControlCentre" => {
                let id = arguments.first()?;
                let part = arguments.get(1)?;
                view.filter(|_| self.host.opened)
                    .map_or_else(String::new, |v| {
                        v.probe.centre(&format!("{id}:{part}"), "pane")
                    })
            }
            "uiSettings" => json(self.document.clone()),
            "showHidden" => self.hidden.to_string(),
            "keymapPreset" => flea::keys::preset(flea::ui_logic_settings_pages::pages::word(
                self.document,
                &["keys"],
                "default",
            ))
            .into(),
            "keymapSheetOpen" => sheet.is_some().to_string(),
            // ui/Ipc.qml keymapQuery and keymapCardRect: the open sheet's typed query and its card's painted box.
            "keymapQuery" => sheet.map_or_else(String::new, |sheet| sheet.query.clone()),
            "keymapCardRect" => self
                .host
                .sheet
                .as_ref()
                .map_or_else(String::new, |view| view.read(self.cx).probe.rect("card")),
            "keymapSheetResults" => json(sheet.map_or(Json::Arr(Vec::new()), results)),
            "keymapSheetRows" => sheet.map_or_else(String::new, sheet_rows),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
