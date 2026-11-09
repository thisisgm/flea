//! Builds the shortcut sheet's keyboard from the active group's samples and the dispatcher's own lookup, user keys included.
//!
//! A platform that samples no keymap (the test platform, a headless session) gets no keyboard, and the sheet draws as it always has.
use crate::g2::mn1::keycaps;
use crate::state::Shell;
use crate::window::user_keys::Bound as UserBound;
use flea::keys::{self, physical::Sample, Context as KeyContext, Match};
use flea::ui_logic_pl3::host::sheet_action;
use flea::ui_logic_st1::sheet_keyboard::{Found, SheetKeyboard, MASKS};
use gpui::{App, Capslock, Context, KeyDownEvent, Keystroke, Modifiers};

/// One modifier set's samples, in the platform's keycode order.
pub(crate) type Samples = Option<Vec<(Keystroke, Modifiers)>>;

/// Key-table bits as the modifiers GPUI samples a keystroke with.
fn held(mask: u8) -> Modifiers {
    Modifiers {
        control: mask & keys::CONTROL != 0,
        shift: mask & keys::SHIFT != 0,
        alt: mask & keys::ALT != 0,
        platform: mask & keys::SUPER != 0,
        ..Default::default()
    }
}

/// What a key types. A dead key types nothing alone; where GPUI names it by its keysym it shows the accent it adds,
/// and where GPUI gives only a US stand-in it stays blank.
fn printed(keystroke: &Keystroke) -> Option<String> {
    let typed = keystroke.key_char.clone().filter(|text| !text.is_empty());
    typed.or_else(|| {
        let accent = match keystroke.key.strip_prefix("dead_")? {
            "acute" => "´",
            "grave" => "`",
            "diaeresis" => "¨",
            "circumflex" => "^",
            "tilde" => "~",
            "cedilla" => "¸",
            "abovering" => "˚",
            "caron" => "ˇ",
            "macron" => "¯",
            "breve" => "˘",
            "doubleacute" => "˝",
            "ogonek" => "˛",
            _ => return None,
        };
        Some(accent.into())
    })
}

/// The catalog key that names an action group: the key tables' own `keys.action.*` row, as the sheet's list says it,
/// else the label the key table gives the action.
fn label_key(group: &str) -> Option<&'static str> {
    let sheet = keys::BASE_SHEET.iter().map(|row| row.label_key);
    let table = keys::PRESET_KEYS
        .iter()
        .chain(keys::SHARED_KEYS)
        .map(|row| row.label_key);
    sheet
        .chain(table)
        .find(|key| key.strip_prefix("keys.action.") == Some(group))
        .or_else(|| keys::user::builtin_label(group).filter(|_| !group.is_empty()))
}

impl Shell {
    /// The keyboard for this open, sampled from the platform's keymap.
    pub(super) fn sheet_keyboard(
        &self,
        preset: &str,
        mirrored: bool,
        cx: &Context<Self>,
    ) -> Option<SheetKeyboard> {
        let mapper = cx.keyboard_mapper().clone();
        let mut keyboard = self.sheet_keyboard_from(preset, mirrored, |modifiers, caps| {
            mapper.shortcut_samples(modifiers, caps)
        })?;
        // A dead key GPUI names only by its US stand-in reads as the keycap the rest list draws for it: ¨ on Swedish.
        if let Some(snapshot) = keycaps::watch(cx) {
            for face in keyboard
                .faces
                .iter_mut()
                .filter(|face| face.legend.is_empty())
            {
                if let Ok(label) = snapshot.label(face.cap.us) {
                    if label != face.cap.us && !label.contains('-') {
                        face.legend = label.to_uppercase();
                    }
                }
            }
        }
        Some(self.over_trash(keyboard, cx))
    }

    /// Over an opened Trash the keyboard shows only the keys Trash answers (`host::sheet_action`), as its list does.
    pub(crate) fn over_trash(&self, mut keyboard: SheetKeyboard, cx: &App) -> SheetKeyboard {
        let trash = self
            .trash
            .as_ref()
            .filter(|host| host.side == self.focus_side);
        if trash.is_some_and(|host| host.pane.read(cx).view().opened) {
            keyboard.bound.retain(|bound| sheet_action(&bound.action));
        }
        keyboard
    }

    /// Legends from the unshifted and shifted samples, bindings from a lookup per modifier set, as a key press would find them.
    pub(crate) fn sheet_keyboard_from(
        &self,
        preset: &str,
        mirrored: bool,
        mut samples: impl FnMut(Modifiers, Capslock) -> Samples,
    ) -> Option<SheetKeyboard> {
        let caps = Capslock { on: false };
        let sampled: Vec<Vec<(Keystroke, Modifiers)>> = MASKS
            .map(|mask| samples(held(mask), caps))
            .collect::<Option<_>>()?;
        let (plain, shifted) = (&sampled[0], &sampled[keys::SHIFT as usize]);
        if plain.is_empty() || plain.len() != shifted.len() {
            return None;
        }
        let legends: Vec<Sample> = plain
            .iter()
            .zip(shifted)
            .map(|((base, _), (up, _))| Sample {
                key: base.key.clone(),
                us: base.us_key.clone(),
                text: printed(base),
                shifted: printed(up),
            })
            .collect();
        let catalog = &self.locale.catalog;
        let plugins = &self.user_keys.facts.plugins;
        let keyboard = SheetKeyboard::new(&legends, |cap, mask| {
            // The same keycode under this modifier set: the samples share the platform's keycode order.
            let index = keys::physical::sample_of(cap, &legends)?;
            let (keystroke, _) = sampled[mask as usize].get(index)?;
            // A dead key composes rather than dispatching, unless Ctrl, Alt or Super is down; Shift can make a key dead.
            let dead = !cap.us.is_empty() && keystroke.key_char.is_none();
            let event = KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: dead
                    && mask & (keys::CONTROL | keys::ALT | keys::SUPER) == 0,
            };
            let modifiers = held(mask);
            let table =
                self.keyboard
                    .lookup(preset, KeyContext::Listing, &event, modifiers, mirrored);
            let action = match table {
                Match::Action(action) => action.to_string(),
                Match::Suppressed => return None,
                Match::Unbound => match self.user_key(preset, &event, modifiers)? {
                    UserBound::Builtin(action) => action.to_string(),
                    UserBound::Plugin(id) => {
                        let label = plugins.iter().find(|(known, _, _)| *known == id)?.1.clone();
                        return Some(Found { action: id, label });
                    }
                },
            };
            let group = keys::action_group(&action);
            // A digit picks its tab, which the catalogs name once for all nine.
            let label = if keys::DIGITS.iter().any(|digit| digit.action == group) {
                catalog.tr("keys.selectTab")
            } else {
                label_key(group).map_or_else(|| group.to_owned(), |key| catalog.tr(key))
            };
            Some(Found { action, label })
        });
        (!keyboard.bound.is_empty()).then_some(keyboard)
    }

    /// The layout, the preset, the language or the user's keys changed while the sheet is open: build its keyboard again.
    /// `user_keys_changed` runs on every plugin reply too, so an unchanged keyboard keeps its looked-up chord.
    pub(crate) fn refresh_sheet_keyboard(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.settings.sheet.clone() else {
            return;
        };
        // The window mirrors for a right-to-left language, so a language switch can turn the arrows' meaning around.
        let mirrored =
            crate::locale_geometry::Direction::from_locale(self.locale.catalog.locale()).is_rtl();
        let (preset, old) = view.read_with(cx, |view, _| {
            (view.sheet.preset.clone(), view.sheet.keyboard.clone())
        });
        let keyboard = self.sheet_keyboard(&preset, mirrored, cx);
        if keyboard == old {
            return;
        }
        view.update(cx, |view, cx| {
            view.sheet.set_keyboard(keyboard);
            cx.notify();
        });
    }
}

#[cfg(test)]
#[path = "sheet_keyboard_tests.rs"]
mod tests;
