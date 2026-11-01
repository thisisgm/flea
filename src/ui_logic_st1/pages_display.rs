// The Display and Keys panes, from ui/js/Settings.js displayRows, keyRows and keyPreview.
use super::super::rows::{choice, KeyItem, Kind, Role, Row};
use super::super::textsize::{effective, following, nearest, parse, STOPS};
use super::super::Text;
use super::{at, is_true, State};
use crate::jsondoc::Json;
use crate::keys::{action_group, binding_rows, lookup, presentation::key_for, Context, Frontend, Input, PRESETS};

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
        KeyItem { keys: "arrows".into(), label: Text::Key(if preset == "mac" { "settings.keys.move_open_up" } else { "settings.keys.move_cursor" }), glyph: "" },
        KeyItem { keys: "enter".into(), label: label_of(preset, enter), glyph: "" },
        KeyItem { keys: "space".into(), label: Text::Key("keys.action.preview"), glyph: "" },
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
            items.push(KeyItem { keys: binding.keys.into(), label: Text::Key(binding.label_key), glyph });
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
    out
}
