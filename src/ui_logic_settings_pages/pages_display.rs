// The Display and Keys panes, from ui/js/Settings.js displayRows, keyRows and keyPreview.
use super::super::rows::{choice, KeyItem, Kind, Role, Row};
use super::super::textsize::{effective, following, nearest, parse, STOPS};
use super::super::Text;
use super::{at, is_true, State};
use crate::jsondoc::Json;
use crate::keys::{action_group, binding_rows, lookup, presentation::key_for, Context, Frontend, Input, PRESETS};
use crate::keys::user::{builtin_label, Projection, UserKeys, Verdict};

fn hinted(template: &'static str, preset: &str, actions: &[&str]) -> Option<Text> {
    sentence(template, actions.iter().map(|action| key_for(preset, action)).collect())
}

// Every key in the list, or nothing when the preset leaves one unbound, so no sentence has a hole.
pub(super) fn sentence(template: &'static str, keys: Vec<String>) -> Option<Text> {
    if keys.iter().any(String::is_empty) {
        return None;
    }
    Some(Text::args(template, keys.into_iter().map(Text::Keycap).collect()))
}

fn px(value: u32) -> Text {
    Text::args("settings.display.px", vec![Text::data(value.to_string())])
}

// The sentence beside Effective has a job only while the ruler cannot state the running size.
fn effective_note(follows: bool, base: u32) -> Text {
    let stop = nearest(f64::from(base));
    if !follows {
        Text::Key("settings.display.your_override")
    } else if stop == base {
        Text::Key("settings.display.omarchy_size")
    } else {
        Text::args("settings.display.nearest_stop", vec![Text::data(stop.to_string())])
    }
}

// The compositor's own number as Hyprland writes it; an unanswered query says so rather than reading 1x.
pub fn scale_label(scale: f64) -> Text {
    if scale.is_nan() || scale <= 0. {
        return Text::Key("settings.display.not_reported");
    }
    Text::data(format!("{}x", (scale * 100.).round() / 100.))
}

pub fn display_rows(state: &State<'_>) -> Vec<Row> {
    let stored = parse(at(state.data, &["display", "textSize"]).unwrap_or(&Json::Null));
    let follows = following(stored);
    let base = effective(stored, state.base_size);
    let mut out = vec![
        Row::group("settings.group.text_size"),
        Row { id: "textMode".into(), glyph: "type",
            labels: vec![Text::Key("settings.choice.follow_omarchy"), Text::Key("settings.choice.override")],
            value: Text::Key(if follows { "settings.choice.follow_omarchy" } else { "settings.choice.override" }),
            ..Row::new(Kind::Choice, Text::Key("settings.row.text_size")) },
        Row { id: "textStop".into(), stops: STOPS.to_vec(), on: !follows,
            index: STOPS.iter().position(|stop| *stop == nearest(f64::from(base))).unwrap_or(0),
            ..Row::new(Kind::Ruler, Text::Empty) },
    ];
    if let Some(keys) = hinted("settings.hint.text_keys", state.preset, &["textSizeUp", "textSizeDown", "textSizeReset"]) {
        out.push(Row::hint(keys));
    }
    out.push(Row { id: "textEffective".into(), role: Role::Live, caption: effective_note(follows, base), ..Row::fact("settings.row.effective", px(base)) });
    out.push(Row::group("settings.group.scale"));
    out.push(Row { glyph: "maximize", ..Row::fact("settings.row.scale", scale_label(state.monitor_scale)) });
    out.push(Row::hint(Text::Key("settings.hint.scale")));
    out.push(Row::group("settings.group.appearance"));
    out.push(Row { mark: "hyprland", ..Row::check("display.hyprlandIcons", "settings.row.hyprland_icons", "", is_true(state.data, &["display", "hyprlandIcons"])) });
    out
}

// The binding a preview example names, as the label keys of the generated table spell it.
fn label_of(preset: &str, action: &str) -> Text {
    binding_rows(preset, Frontend::Gui).find(|row| action_group(row.action) == action)
        .map_or_else(|| Text::data(action), |row| Text::Key(row.label_key))
}

// Settings shows six examples; the keyboard sheet keeps the complete inventory.
pub fn key_preview(preset: &str) -> Vec<KeyItem> {
    let enter = lookup(preset, Context::Listing, Frontend::Gui, Input { key: "Return", text: "", modifiers: 0 }).action();
    let mut items = vec![
        KeyItem { keys: "arrows".into(), word: Text::Key("settings.keys.arrows"), label: Text::Key(if preset == "mac" { "settings.keys.move_open_up" } else { "settings.keys.move_cursor" }), glyph: "" },
        KeyItem { keys: "enter".into(), label: label_of(preset, enter), glyph: "", ..KeyItem::default() },
        KeyItem { keys: "space".into(), label: Text::Key("keys.action.preview"), glyph: "", ..KeyItem::default() },
    ];
    for action in ["copy", "paste", "trash"] {
        let mods = match (preset, action) {
            ("mac" | "windows", "trash") => "none",
            ("mac", _) => "super",
            ("windows", _) => "ctrl",
            _ => "text",
        };
        if let Some(binding) = binding_rows(preset, Frontend::Gui).find(|row| action_group(row.action) == action && row.mods == mods) {
            let glyph = match action { "copy" => "copy", "paste" => "clipboard", _ => "" };
            items.push(KeyItem { keys: binding.keys.into(), label: Text::Key(binding.label_key), glyph, ..KeyItem::default() });
        }
    }
    items
}

const PRESET_LABELS: [&str; 4] = ["settings.choice.preset_default", "settings.choice.preset_vim", "settings.choice.preset_mac", "settings.choice.preset_windows"];

pub fn key_rows(state: &State<'_>) -> Vec<Row> {
    let mut preset = choice("preset", "settings.row.preset", "keyboard", &PRESETS, &PRESET_LABELS, state.preset);
    if !PRESETS.contains(&state.preset) {
        preset.value = Text::data(state.preset);
    }
    let mut out = vec![
        Row::group("settings.group.preset"),
        preset,
        Row::hint(Text::Key("settings.hint.preset")),
        Row::group("settings.group.this_preset"),
        Row { id: "keyPreview".into(), items: key_preview(state.preset), ..Row::new(Kind::KeyPreview, Text::Empty) },
    ];
    if let Some(tip) = hinted("settings.hint.sheet_key", state.preset, &["keymapSheet"]) {
        out.push(Row::hint(tip));
    }
    if let Some(facts) = state.user_keys {
        out.extend(user_key_rows(facts, state.preset));
    }
    out
}

pub const USER_KEYS: &str = "userKeys";

/// What Settings > Keys needs to list the user's own bindings: the parsed file and each enabled plugin action's id, label and glyph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserKeyFacts {
    pub keys: UserKeys,
    pub plugins: Vec<(String, String, String)>,
    pub catalog: Option<Vec<String>>,
    pub resolved: Option<(String, Vec<Verdict>)>,
}

impl UserKeyFacts {
    pub fn declares(&self, id: &str) -> bool {
        self.plugins.iter().any(|(known, _, _)| known == id)
    }

    pub fn resolve(&mut self, preset: &str, projection: &Projection) {
        let preset = crate::keys::preset(preset);
        let verdicts = self
            .keys
            .verdicts(preset, &|id| self.declares(id), projection);
        self.resolved = Some((preset.into(), verdicts));
    }

    pub fn current_verdicts(&self, preset: &str) -> Option<&[Verdict]> {
        let (known, verdicts) = self.resolved.as_ref()?;
        (known == crate::keys::preset(preset) && verdicts.len() == self.keys.bindings.len())
            .then_some(verdicts.as_slice())
    }

    fn verdicts(&self, preset: &str) -> Vec<Verdict> {
        let preset = crate::keys::preset(preset);
        let verdicts = self
            .current_verdicts(preset)
            .map(<[Verdict]>::to_vec)
            .unwrap_or_else(|| {
                self.keys
                    .verdicts(preset, &|id| self.declares(id), &Projection::default())
            });
        verdicts
            .into_iter()
            .zip(&self.keys.bindings)
            .map(|(verdict, binding)| {
                let verdict = if binding.is_plugin() && !self.declares(&binding.action) {
                    Verdict::Inactive
                } else {
                    verdict
                };
                if verdict == Verdict::Inactive
                    && binding.is_plugin()
                    && (binding.preset == "all" || binding.preset == preset)
                    && self
                        .catalog
                        .as_ref()
                        .is_some_and(|catalog| !catalog.contains(&binding.action))
                {
                    Verdict::Unknown
                } else {
                    verdict
                }
            })
            .collect()
    }
}

/// One refusal sentence per binding that stays unbound in the active preset.
pub fn user_binding_refusals(facts: &UserKeyFacts, preset: &str) -> Vec<Text> {
    let verdicts = facts.verdicts(preset);
    facts
        .keys
        .bindings
        .iter()
        .zip(&verdicts)
        .filter_map(|(binding, verdict)| {
            let keys = Text::data(binding.keys());
            match verdict {
                Verdict::Taken("") => Some(Text::args("settings.hint.key_reserved", vec![keys])),
                Verdict::Taken(action) => {
                    let by = builtin_label(action).map_or_else(|| Text::data(*action), Text::Key);
                    Some(Text::args("settings.hint.key_taken", vec![keys, by]))
                }
                Verdict::Repeated => Some(Text::args("settings.hint.key_repeated", vec![keys])),
                Verdict::Unknown => Some(Text::args(
                    "settings.hint.key_unknown",
                    vec![keys, Text::data(binding.action.clone())],
                )),
                Verdict::Unavailable => Some(Text::args("settings.hint.key_layout", vec![keys])),
                Verdict::Live | Verdict::Inactive => None,
            }
        })
        .collect()
}

/// Binding refusals followed by unreadable blocks in file order.
pub fn user_key_refusals(facts: &UserKeyFacts, preset: &str) -> Vec<Text> {
    let mut out = user_binding_refusals(facts, preset);
    // Line 0 is the whole file: too large or not UTF-8 text.
    out.extend(
        facts
            .keys
            .problems
            .iter()
            .map(|problem| match problem.line {
                0 => Text::Key("settings.hint.key_file"),
                line => Text::args(
                    "settings.hint.key_malformed",
                    vec![Text::data(line.to_string())],
                ),
            }),
    );
    out
}

/// The Your Keys group: live bindings in the This Preset recipe, then each refusal as a hint; nothing when the file binds nothing here.
pub fn user_key_rows(facts: &UserKeyFacts, preset: &str) -> Vec<Row> {
    let verdicts = facts.verdicts(preset);
    let items: Vec<KeyItem> = facts
        .keys
        .bindings
        .iter()
        .zip(&verdicts)
        .filter(|(_, verdict)| **verdict == Verdict::Live)
        .map(|(binding, _)| {
            let plugin = facts
                .plugins
                .iter()
                .find(|(id, _, _)| *id == binding.action);
            let label = plugin.map_or_else(
                || {
                    builtin_label(&binding.action)
                        .map_or_else(|| Text::data(binding.action.clone()), Text::Key)
                },
                |(_, label, _)| Text::data(label.clone()),
            );
            let glyph = plugin
                .and_then(|(_, _, glyph)| crate::ui_logic_custom_actions::rows::static_glyph(glyph))
                .unwrap_or("");
            KeyItem {
                keys: binding.keys(),
                label,
                glyph,
                ..KeyItem::default()
            }
        })
        .collect();
    let refusals = user_key_refusals(facts, preset);
    if items.is_empty() && refusals.is_empty() {
        return Vec::new();
    }
    let mut out = vec![Row::group("settings.group.your_keys")];
    if !items.is_empty() {
        out.push(Row {
            id: USER_KEYS.into(),
            items,
            ..Row::new(Kind::KeyPreview, Text::Empty)
        });
    }
    out.extend(refusals.into_iter().map(Row::hint));
    out
}
