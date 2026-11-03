//! Hyprland's transparent tag for this app's windows, with the decisions Flea 0.3.12's GlassTag.js made.
pub(super) mod events;
pub(super) mod run;

use super::APP_ID;
use crate::presentation::fonts::{shell_value, uncomment};
use flea::jsondoc::{self, Json};
use std::collections::BTreeSet;

pub use events::{follow, start, stop};

/// The Hyprland tag Omarchy's transparency hotkey sets; Omarchy's own rule then draws the window at its Transparent opacity.
pub const TAG: &str = "transparent";
/// The Hyprland rule that gives Flea Nautilus's Glassy opacity; one name, so a later pass updates or disables it.
pub const RULE: &str = "flea-glassy";
// The key under [flea] in shell.toml that sets Flea's own background alpha.
const ALPHA_KEY: &str = "background-alpha";

/// Omarchy's Window Transparency mode, read from its toggle files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Opaque,
    Glassy,
    Transparent,
}

impl Mode {
    /// Glassy wins over Opaque, and neither file means Transparent, Omarchy's default.
    pub fn from_toggles(glassy: bool, opaque: bool) -> Self {
        if glassy {
            Self::Glassy
        } else if opaque {
            Self::Opaque
        } else {
            Self::Transparent
        }
    }
}

/// What one window gets in one pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Add,
    Remove,
    Keep,
}

/// Add only where Omarchy draws Transparent and Hyprland leaves the window at 1; remove only Flea's own tag.
pub fn decide(
    mode: Mode,
    uses_default: bool,
    opacity: f64,
    inactive: f64,
    tagged_by_flea: bool,
) -> Action {
    let wanted = mode == Mode::Transparent && uses_default;
    // Also re-tags a window reusing a closed one's address; a window Hyprland draws below 1 keeps its rule.
    if wanted && opacity == 1. && inactive == 1. {
        return Action::Add;
    }
    if tagged_by_flea && !wanted {
        Action::Remove
    } else {
        Action::Keep
    }
}

/// The dispatches for one PROBE text, and the tagged set once they run; a closed window drops out of it.
pub struct Plan {
    pub changes: Vec<(String, Action)>,
    pub tagged: BTreeSet<String>,
}

pub fn plan(text: &str, mode: Mode, uses_default: bool, tagged: &BTreeSet<String>) -> Plan {
    let mut next = BTreeSet::new();
    let mut changes = Vec::new();
    for line in text.lines() {
        let Some(window) = parse_props(line) else {
            // A probe whose getprop failed still names a window Flea tagged, so a later switch can take the tag off.
            if let Some(address) = line
                .split_whitespace()
                .next()
                .filter(|a| tagged.contains(*a))
            {
                next.insert(address.to_owned());
            }
            continue;
        };
        let mine = tagged.contains(&window.address);
        if mine {
            next.insert(window.address.clone());
        }
        match decide(mode, uses_default, window.opacity, window.inactive, mine) {
            Action::Add => {
                next.insert(window.address.clone());
                changes.push((window.address, Action::Add));
            }
            Action::Remove => {
                next.remove(&window.address);
                changes.push((window.address, Action::Remove));
            }
            Action::Keep => {}
        }
    }
    Plan {
        changes,
        tagged: next,
    }
}

/// A PROBE line gives a window address with its opacity and its inactive opacity.
#[derive(Debug)]
pub struct Props {
    pub address: String,
    pub opacity: f64,
    pub inactive: f64,
}

/// Parses "0x55bacb0a1f80 0.985 0.96"; any other field count, or a bad address, is no window.
pub fn parse_props(line: &str) -> Option<Props> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    match fields.as_slice() {
        [address, opacity, inactive] if valid_address(address) => Some(Props {
            address: (*address).to_owned(),
            opacity: opacity.parse().unwrap_or(f64::NAN),
            inactive: inactive.parse().unwrap_or(f64::NAN),
        }),
        _ => None,
    }
}

/// Hyprland addresses are lowercase hex after a 0x prefix; nothing else reaches a dispatch.
pub fn valid_address(text: &str) -> bool {
    text.strip_prefix("0x").is_some_and(|hex| {
        !hex.is_empty()
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// The addresses of this process's windows in `hyprctl clients -j` output, valid addresses only.
/// Sample input: `[{"address":"0x55bacb0a1f80","pid":4211,"class":"com.thisisgm.flea"}]`.
pub fn own_addresses(json: &str, pid: u32) -> Vec<String> {
    let Ok(clients) = jsondoc::parse(json) else {
        return Vec::new();
    };
    let Some(clients) = clients.as_array() else {
        return Vec::new();
    };
    clients
        .iter()
        .filter(|client| client.get("pid").and_then(Json::as_f64) == Some(f64::from(pid)))
        .filter_map(|client| client.get("address").and_then(Json::as_str))
        .filter(|address| valid_address(address))
        .map(str::to_owned)
        .collect()
}

/// Whether a `hyprctl dispatch` reply says the dispatcher ran; a refused one still exits 0.
/// Sample input: `ok` (ran), `Invalid dispatcher` or `warning: =[C]:-1: hl.focus: window not found` (did not).
pub fn dispatch_ran(reply: &str) -> bool {
    reply.trim() == "ok"
}

/// The Lua dispatch Omarchy's hotkey runs for one window argument and one tag argument.
pub fn lua_tag(window: &str, tag: &str) -> String {
    format!("hl.dsp.window.tag({{ window = \"{window}\", tag = \"{tag}\" }})")
}

/// The opacity Omarchy's Glassy toggle gives Nautilus, as (active, inactive); one value sets both, as in Hyprland.
/// Sample input: `o.window("org\\.gnome\\.Nautilus", { opacity = "0.75 0.75" })`.
pub fn nautilus_opacity(toggle: &str) -> Option<(f64, f64)> {
    let unit = |v: &f64| (0. ..=1.).contains(v);
    toggle
        .lines()
        .filter_map(|line| line.split("--").next())
        .filter(|line| line.contains("Nautilus"))
        .find_map(|line| {
            let quoted = line.split_once("opacity")?.1.split('"').nth(1)?;
            let values: Vec<f64> = quoted
                .split_whitespace()
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            match values.as_slice() {
                [both] if unit(both) => Some((*both, *both)),
                [active, inactive] if unit(active) && unit(inactive) => Some((*active, *inactive)),
                _ => None,
            }
        })
}

/// The Lua `hyprctl eval` runs: Nautilus's Glassy opacity on every Flea window, or with None that rule disabled until the next reload drops it.
pub fn lua_rule(opacity: Option<(f64, f64)>) -> String {
    match opacity {
        Some((active, inactive)) => format!(
            "hl.window_rule({{ name = \"{RULE}\", enabled = true, match = {{ class = \"^{}$\" }}, opacity = \"{active} {inactive}\" }})",
            APP_ID.replace('.', "\\\\.")
        ),
        None => format!("hl.window_rule({{ name = \"{RULE}\", enabled = false }})"),
    }
}

// Hyprland evaluates each chunk in its existing Lua state, so ownership and rule changes cannot interleave.
fn lua_owners(pid: u32, operation: &str) -> String {
    format!(
        r#"-- flea-glassy {operation} {pid}
local owners = _G.__flea_glassy_owners or {{}}
local live = {{}}
for _, window in ipairs(hl.get_windows()) do
    if window.class == "{APP_ID}" then live[window.pid] = true end
end
for owner in pairs(owners) do
    if not live[owner] then owners[owner] = nil end
end
"#
    )
}

/// Applies the shared rule and updates only this process's ownership; a mode switch still disables unconditionally.
pub fn lua_owned_rule(pid: u32, opacity: Option<(f64, f64)>) -> String {
    if opacity.is_none() {
        return format!(
            "{}owners[{pid}] = nil\n_G.__flea_glassy_owners = next(owners) and owners or nil\n{}",
            lua_owners(pid, "disable"),
            lua_rule(None)
        );
    }
    format!(
        "{}owners[{pid}] = true\n_G.__flea_glassy_owners = owners\n{}",
        lua_owners(pid, "enable"),
        lua_rule(opacity)
    )
}

/// Retires this process atomically; the last owner clears the rule even when closing clients remain listed.
pub fn lua_exit_rule(pid: u32) -> String {
    format!(
        "{}owners[{pid}] = nil\nif next(owners) == nil then\n    _G.__flea_glassy_owners = nil\n    {}\nelse\n    _G.__flea_glassy_owners = owners\nend",
        lua_owners(pid, "exit"),
        lua_rule(None)
    )
}

/// The rule to evaluate this pass; when no opacity is wanted, the first pass clears a rule an earlier process left on.
pub fn rule_step(
    wanted: Option<(f64, f64)>,
    windows: &[Props],
    ruled: Option<bool>,
) -> Option<Option<(f64, f64)>> {
    let Some((active, inactive)) = wanted else {
        return (ruled != Some(false)).then_some(None);
    };
    let off = |seen: f64, want: f64| seen.is_finite() && (seen - want).abs() > 1e-3;
    windows
        .iter()
        .any(|window| {
            // Each process confirms the shared rule once, so the last survivor owns its cleanup too.
            (ruled.is_none() && window.opacity.is_finite() && window.inactive.is_finite())
                || off(window.opacity, active)
                || off(window.inactive, inactive)
        })
        .then_some(wanted)
}

/// Whether a shell.toml sets [flea] background-alpha to a value the font reader accepts.
/// Sample input: `[flea]` on one line, then `background-alpha = 0.34`.
fn sets_alpha(text: &str) -> bool {
    let mut section = "";
    for line in text.lines() {
        let line = uncomment(line).trim();
        if let Some(name) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = name;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if section == "flea" && key.trim() == ALPHA_KEY && shell_value(value.trim()).is_some() {
            return true;
        }
    }
    false
}

/// Whether neither the user's nor the theme's shell.toml sets Flea's background alpha.
pub fn uses_default(user_shell: &str, theme_shell: &str) -> bool {
    !sets_alpha(user_shell) && !sets_alpha(theme_shell)
}

/// Whether a Hyprland socket2 line asks for a pass, as a config reload or this app's window opening does.
/// Sample input: `openwindow>>55bacb0a1f80,1,com.thisisgm.flea,Flea` or `configreloaded>>`.
pub fn retags(line: &str) -> bool {
    let Some((event, data)) = line.split_once(">>") else {
        return false;
    };
    match event {
        "configreloaded" => true,
        "openwindow" => data.split(',').nth(2) == Some(APP_ID),
        _ => false,
    }
}
