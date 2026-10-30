// Lookup checks ported from the keymap oracle, run for every preset, context and frontend.
use crate::keys::{
    binding_rows, lookup, Context, Frontend, Input, ALT, CONTROL, PRESETS, SHIFT, SUPER,
};

const NONE: u8 = 0;
const CTRL: u8 = CONTROL;
const CTRL_SHIFT: u8 = CONTROL | SHIFT;

fn key_in(
    preset: &str,
    name: &str,
    text: &str,
    mods: u8,
    expected: &str,
    context: Context,
    frontend: Frontend,
) {
    let input = Input {
        key: name,
        text,
        modifiers: mods,
    };
    let got = lookup(preset, context, frontend, input).action();
    assert_eq!(
        got,
        expected,
        "{preset} {} {} {name} {text:?} {mods}",
        context.name(),
        frontend.name()
    );
}

fn key(preset: &str, name: &str, text: &str, mods: u8, expected: &str) {
    key_in(preset, name, text, mods, expected, Context::Listing, Frontend::Gui);
}

// The Mac preset binds its command chords to Super; every other preset uses Control.
fn command_chord(preset: &str) -> u8 {
    if preset == "mac" {
        SUPER
    } else {
        CONTROL
    }
}

// The Windows preset puts the view chords on Control and Shift.
fn view_chord(preset: &str) -> u8 {
    if preset == "windows" {
        CTRL_SHIFT
    } else {
        CONTROL
    }
}

#[test]
fn preview_paging_letters_are_shift_keys() {
    for preset in PRESETS {
        key(preset, "F", "F", SHIFT, "previewPageForward");
        key(preset, "B", "B", SHIFT, "previewPageBack");
        key(preset, "F", "f", NONE, "search");
        key_in(preset, "F", "f", NONE, "copydirpath", Context::Menu, Frontend::Gui);
        for (context, forward, back) in [
            (Context::Preview, "previewPageForward", "previewPageBack"),
            (Context::Pdf, "previewPageForward", "previewPageBack"),
            (Context::Media, "", ""),
        ] {
            key_in(preset, "F", "F", SHIFT, forward, context, Frontend::Gui);
            key_in(preset, "B", "B", SHIFT, back, context, Frontend::Gui);
        }
        let paging = binding_rows(preset, Frontend::Gui)
            .filter(|row| row.mods == "text" && (row.key == "F" || row.key == "B"))
            .count();
        assert_eq!(paging, 2, "{preset} claims each preview paging letter once");
    }
}

#[test]
fn cursor_preview_and_tab_step_keys() {
    for preset in PRESETS {
        key(preset, "Down", "", NONE, "cursorDown");
        key(preset, "Up", "", NONE, "cursorUp");
        key(preset, "Space", " ", NONE, "preview");
        key(preset, "P", "p", ALT, "togglePreview");
        key(preset, "Space", " ", CTRL, "loadPreview");
        key(preset, "PageDown", "", CTRL, "tabNext");
        key(preset, "PageUp", "", CTRL, "tabPrevious");
    }
}

#[test]
fn brace_and_bracket_step_tabs() {
    for preset in PRESETS {
        key(preset, "BraceLeft", "{", SHIFT, "tabMoveLeft");
        key(preset, "BraceRight", "}", SHIFT, "tabMoveRight");
        key(preset, "PageDown", "", CTRL_SHIFT, "tabMoveRight");
        key(preset, "PageUp", "", CTRL_SHIFT, "tabMoveLeft");
        key(preset, "BracketLeft", "[", NONE, "tabPrevious");
        key(preset, "BracketRight", "]", NONE, "tabNext");
    }
}

// EY-58: [ and ] turn sheets in the Quick Look contexts, and stay tab steps in the listing.
#[test]
fn brackets_turn_sheets_in_the_quick_look_contexts() {
    for preset in PRESETS {
        for context in [Context::Preview, Context::Pdf] {
            key_in(preset, "BracketLeft", "[", NONE, "previousSheet", context, Frontend::Gui);
            key_in(preset, "BracketRight", "]", NONE, "nextSheet", context, Frontend::Gui);
        }
        key_in(preset, "BracketLeft", "[", NONE, "tabPrevious", Context::Listing, Frontend::Gui);
        key_in(preset, "BracketRight", "]", NONE, "tabNext", Context::Listing, Frontend::Gui);
        key_in(preset, "BracketRight", "]", NONE, "", Context::Media, Frontend::Gui);
    }
}

#[test]
fn reload_focus_delete_and_close_keys() {
    for preset in PRESETS {
        key(preset, "F5", "", NONE, "reload");
        key(preset, "R", "", CTRL, "reload");
        key(preset, "Tab", "", CTRL, "focusPreview");
        key(preset, "Tab", "", NONE, "focusNext");
        key(preset, "Delete", "", SHIFT, "deletePermanently");
        key(preset, "Delete", "", NONE, "trash");
        key(preset, "W", "", CTRL, "tabClose");
    }
}

#[test]
fn open_tab_sort_filter_and_sheet_keys() {
    for preset in PRESETS {
        key(preset, "Return", "", CTRL, "openTab");
        key(preset, "Enter", "", CTRL, "openTab");
        key(preset, "S", "s", NONE, "sortNext");
        key(preset, "S", "S", SHIFT, "sortReverse");
        key(preset, "Slash", "/", NONE, "filter");
        key(preset, "Question", "?", SHIFT, "keymapSheet");
    }
}

#[test]
fn insert_and_listing_digits_by_frontend() {
    for preset in PRESETS {
        key_in(preset, "Insert", "", CTRL, "copy", Context::Listing, Frontend::Tui);
        key_in(preset, "Insert", "", SHIFT, "paste", Context::Listing, Frontend::Tui);
        key_in(preset, "Insert", "", CTRL, "", Context::Listing, Frontend::Gui);
        for frontend in [Frontend::Tui, Frontend::Gui] {
            key_in(preset, "1", "1", NONE, "tab1", Context::Listing, frontend);
            key_in(preset, "9", "9", NONE, "tab9", Context::Listing, frontend);
        }
    }
}

#[test]
fn bare_digits_name_tags_in_menus_and_nothing_in_fields() {
    for preset in PRESETS {
        key_in(preset, "1", "1", NONE, "tag1", Context::Menu, Frontend::Gui);
        key_in(preset, "7", "7", NONE, "tag7", Context::Menu, Frontend::Gui);
        key_in(preset, "1", "1", NONE, "tab1", Context::Rail, Frontend::Gui);
        for context in [
            Context::Editor,
            Context::Panel,
            Context::Preview,
            Context::Pdf,
            Context::Media,
        ] {
            key_in(preset, "1", "1", NONE, "", context, Frontend::Gui);
        }
    }
}

#[test]
fn new_window_and_view_chords() {
    for preset in PRESETS {
        key(preset, "N", "", command_chord(preset), "windowNew");
        key_in(preset, "N", "", command_chord(preset), "", Context::Listing, Frontend::Tui);
        key(preset, "1", "", view_chord(preset), "viewList");
        key(preset, "2", "", view_chord(preset), "viewColumns");
        key(preset, "3", "", view_chord(preset), "viewGrid");
        key(preset, "4", "", view_chord(preset), "viewDual");
    }
}

#[test]
fn focus_back_and_settings_keys() {
    for preset in PRESETS {
        key(preset, "Tab", "", SHIFT, "focusPrevious");
        key(preset, "Backtab", "", SHIFT, "focusPrevious");
        key(preset, "Comma", "", CTRL, "settings");
    }
}

#[test]
fn menu_key_opens_the_menu_in_listing_and_rail_only() {
    for preset in PRESETS {
        for frontend in [Frontend::Gui, Frontend::Tui] {
            for (index, context) in Context::ALL.into_iter().enumerate() {
                let expected = if index < 2 { "menu" } else { "" };
                key_in(preset, "Menu", "", NONE, expected, context, frontend);
                key_in(preset, "F10", "", SHIFT, expected, context, frontend);
            }
        }
    }
}

#[test]
fn unbound_chords_stay_unbound() {
    for preset in PRESETS {
        key(preset, "N", "", CTRL_SHIFT, "newFolder");
        key(preset, "Q", "q", ALT, "");
        key(preset, "J", "j", SUPER, "");
        key(preset, "D", "D", CTRL_SHIFT, "");
    }
}

#[test]
fn enter_escape_space_and_chords_per_context() {
    for preset in PRESETS {
        for context in [
            Context::Menu,
            Context::Panel,
            Context::Preview,
            Context::Pdf,
            Context::Media,
        ] {
            key_in(preset, "Return", "", NONE, "open", context, Frontend::Gui);
            key_in(preset, "Enter", "", NONE, "open", context, Frontend::Gui);
            key_in(preset, "Space", " ", NONE, "preview", context, Frontend::Gui);
            key_in(preset, "Escape", "", NONE, "escape", context, Frontend::Gui);
            key_in(preset, "D", "d", NONE, "", context, Frontend::Gui);
            key_in(preset, "X", "", CTRL, "", context, Frontend::Gui);
            key_in(preset, "N", "", CTRL, "", context, Frontend::Gui);
        }
    }
}

#[test]
fn menu_navigation_keys() {
    for preset in PRESETS {
        key_in(preset, "Right", "", NONE, "menuRight", Context::Menu, Frontend::Gui);
        key_in(preset, "Left", "", NONE, "parent", Context::Menu, Frontend::Gui);
        key_in(preset, "J", "j", NONE, "cursorDown", Context::Menu, Frontend::Gui);
        key_in(preset, "K", "k", NONE, "cursorUp", Context::Menu, Frontend::Gui);
    }
}

#[test]
fn pdf_and_editor_keys() {
    for preset in PRESETS {
        key_in(preset, "Minus", "-", NONE, "zoomOut", Context::Pdf, Frontend::Gui);
        key_in(preset, "Plus", "+", SHIFT, "zoomIn", Context::Pdf, Frontend::Gui);
        key_in(preset, "E", "e", NONE, "expand", Context::Pdf, Frontend::Gui);
        key_in(preset, "Tab", "", SHIFT, "focusPrevious", Context::Pdf, Frontend::Gui);
        key_in(preset, "Tab", "", CTRL, "focusPreview", Context::Preview, Frontend::Gui);
        key_in(preset, "J", "j", NONE, "", Context::Pdf, Frontend::Gui);
        key_in(preset, "K", "k", NONE, "", Context::Pdf, Frontend::Gui);
        key_in(preset, "Up", "", NONE, "cursorUp", Context::Pdf, Frontend::Gui);
        key_in(preset, "Down", "", NONE, "cursorDown", Context::Pdf, Frontend::Gui);
        key_in(preset, "Return", "", NONE, "open", Context::Pdf, Frontend::Gui);
        key_in(preset, "Return", "", NONE, "", Context::Editor, Frontend::Gui);
    }
}

#[test]
fn default_preset_keys() {
    key("default", "Return", "", NONE, "open");
    key("default", "Backspace", "", NONE, "parent");
    key("default", "H", "H", SHIFT, "historyBack");
    key("default", "L", "L", SHIFT, "historyForward");
    key("default", "Y", "y", NONE, "copy");
    key("default", "X", "x", NONE, "cut");
    key("default", "P", "p", NONE, "paste");
    key("default", "Z", "z", NONE, "undo");
    key("default", "D", "d", NONE, "trashArm");
    key("default", "D", "", CTRL, "pageDown");
    key("default", "T", "t", NONE, "tabNew");
}

#[test]
fn vim_preset_keys() {
    key("vim", "L", "l", NONE, "open");
    key("vim", "H", "h", NONE, "parent");
    key("vim", "Y", "y", NONE, "copyArm");
    key("vim", "D", "d", NONE, "cutArm");
    key("vim", "P", "p", NONE, "pasteArm");
    key("vim", "G", "g", NONE, "cursorFirstArm");
    key("vim", "G", "G", SHIFT, "cursorLast");
    key("vim", "D", "D", SHIFT, "trash");
    key("vim", "U", "u", NONE, "undo");
}

#[test]
fn mac_navigation_and_clipboard_keys() {
    key("mac", "Return", "", NONE, "rename");
    key("mac", "Enter", "", NONE, "rename");
    key("mac", "Right", "", NONE, "open");
    key("mac", "Left", "", NONE, "parent");
    key("mac", "C", "", SUPER, "copy");
    key("mac", "C", "", CTRL, "copy");
    key("mac", "V", "", SUPER, "paste");
    key("mac", "V", "", CTRL, "paste");
    key("mac", "V", "", SUPER | ALT, "movePaste");
    key("mac", "X", "", CTRL, "");
    key("mac", "D", "", SUPER, "duplicate");
    key("mac", "Z", "", SUPER, "undo");
}

#[test]
fn mac_modifier_chords_and_default_redo() {
    key("mac", "Z", "", SUPER | SHIFT, "redo");
    key("default", "Z", "", CTRL_SHIFT, "redo");
    key("mac", "A", "", SUPER, "selectAll");
    key("mac", "I", "", SUPER, "properties");
    key("mac", "BracketLeft", "", SUPER, "historyBack");
    key("mac", "BracketRight", "", SUPER, "historyForward");
    key("mac", "Period", ".", SUPER | SHIFT, "toggleHidden");
    key("mac", "Delete", "", SHIFT, "deletePermanently");
    key("mac", "T", "", CTRL, "tabNew");
}

#[test]
fn windows_preset_keys() {
    key("windows", "Return", "", NONE, "open");
    key("windows", "F2", "", NONE, "rename");
    key("windows", "X", "", CTRL, "cut");
    key("windows", "D", "", CTRL, "trash");
    key("windows", "Delete", "", SHIFT, "deletePermanently");
    key("windows", "Y", "", CTRL, "redo");
    key("windows", "Return", "", ALT, "properties");
    key("windows", "Enter", "", ALT, "properties");
    key("windows", "Left", "", ALT, "historyBack");
    key("windows", "Right", "", ALT, "historyForward");
    key("windows", "Up", "", ALT, "parent");
    key("windows", "H", "", CTRL, "toggleHidden");
    key("windows", "T", "", CTRL, "tabNew");
}
