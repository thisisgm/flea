//! The physical desktop keyboard the shortcut sheet draws: caps in key units, ISO or ANSI, with a navigation block.
//!
//! A printing cap is found by its US position, the `us_key` GPUI's samples carry, so its legend is whatever the layout
//! types there. A named cap (Escape, the function row, Enter, the arrows) is found by the key name GPUI reports for it.
#[path = "physical_legends.rs"]
mod legends;
pub use legends::{is_iso, legends, right_alt_is_alt, sample_of, Sample};

/// One row is a key unit tall; the function row is row 0 and stands a little apart from the five below it.
pub const ROWS: u8 = 6;
/// The board's width in key units: the main block, a half-unit gap and the three-column navigation block.
pub const WIDTH: f32 = 18.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Whole,
    /// ISO Enter is one key drawn in two pieces: the wide top on the Tab row and the narrower foot under it.
    EnterTop,
    EnterFoot,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cap {
    /// A stable position name, as xkb names it.
    pub id: &'static str,
    pub x: f32,
    pub row: u8,
    pub w: f32,
    /// The GPUI key name a named cap sends. Empty on a printing cap and on caps that bind nothing.
    pub key: &'static str,
    /// What a named cap reads. Empty on a printing cap, whose legend is the layout's.
    pub name: &'static str,
    /// A printing cap's US position, the character GPUI's `us_key` names; `<` is the ISO key beside left Shift.
    pub us: &'static str,
    pub part: Part,
}

/// A cap that sends a named key, such as Tab or an arrow, and reads its name.
const fn named(
    id: &'static str,
    x: f32,
    row: u8,
    w: f32,
    key: &'static str,
    name: &'static str,
) -> Cap {
    Cap {
        id,
        x,
        row,
        w,
        key,
        name,
        us: "",
        part: Part::Whole,
    }
}

/// A cap that types a character; its legend comes from the live layout.
const fn printing(id: &'static str, x: f32, row: u8, us: &'static str) -> Cap {
    Cap {
        id,
        x,
        row,
        w: 1.,
        key: "",
        name: "",
        us,
        part: Part::Whole,
    }
}

/// The modifier bit a cap holds. Right Alt is AltGr on most groups; the board decides it from the keymap.
pub fn modifier(cap: &Cap) -> u8 {
    match cap.id {
        "LFSH" | "RTSH" => super::SHIFT,
        "LCTL" | "RCTL" => super::CONTROL,
        "LALT" => super::ALT,
        "LWIN" | "RWIN" => super::SUPER,
        _ => 0,
    }
}

/// Caps that never carry a binding of their own: the modifiers, right Alt, Caps Lock and the three lock keys.
pub fn inert(cap: &Cap) -> bool {
    modifier(cap) != 0 || matches!(cap.id, "RALT" | "CAPS" | "PRSC" | "SCLK" | "PAUS")
}

/// Escape, F1 to F12 in groups of four, and the three lock keys.
fn function_row(caps: &mut Vec<Cap>) {
    caps.push(named("ESC", 0., 0, 1., "escape", "ESC"));
    let ids = [
        "FK01", "FK02", "FK03", "FK04", "FK05", "FK06", "FK07", "FK08", "FK09", "FK10", "FK11",
        "FK12",
    ];
    let keys = "f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12".split(' ');
    let names = "F1 F2 F3 F4 F5 F6 F7 F8 F9 F10 F11 F12".split(' ');
    for (index, ((id, key), name)) in ids.into_iter().zip(keys).zip(names).enumerate() {
        // Three groups of four, half a unit apart, the last ending flush with Backspace.
        let x = 2. + index as f32 + 0.5 * (index / 4) as f32;
        caps.push(named(id, x, 0, 1., key, name));
    }
    caps.push(named("PRSC", 15.5, 0, 1., "", "PRT"));
    caps.push(named("SCLK", 16.5, 0, 1., "", "SCRL"));
    caps.push(named("PAUS", 17.5, 0, 1., "", "PAUS"));
}

/// A row of printing caps one unit apart, named by their US characters.
fn run(caps: &mut Vec<Cap>, ids: &[&'static str], us: &'static str, x: f32, row: u8) {
    for (index, (id, us)) in ids.iter().zip(us.split(' ')).enumerate() {
        caps.push(printing(id, x + index as f32, row, us));
    }
}

/// The typing block, ISO or ANSI: the digit row to the space bar, then the editing keys and arrows.
fn main_block(caps: &mut Vec<Cap>, iso: bool) {
    caps.push(printing("TLDE", 0., 1, "`"));
    let digits = [
        "AE01", "AE02", "AE03", "AE04", "AE05", "AE06", "AE07", "AE08", "AE09", "AE10", "AE11",
        "AE12",
    ];
    run(caps, &digits, "1 2 3 4 5 6 7 8 9 0 - =", 1., 1);
    caps.push(named("BKSP", 13., 1, 2., "backspace", "BKSP"));
    caps.push(named("TAB", 0., 2, 1.5, "tab", "TAB"));
    let top = [
        "AD01", "AD02", "AD03", "AD04", "AD05", "AD06", "AD07", "AD08", "AD09", "AD10", "AD11",
        "AD12",
    ];
    run(caps, &top, "q w e r t y u i o p [ ]", 1.5, 2);
    caps.push(named("CAPS", 0., 3, 1.75, "", "CAPS"));
    let home = [
        "AC01", "AC02", "AC03", "AC04", "AC05", "AC06", "AC07", "AC08", "AC09", "AC10", "AC11",
    ];
    run(caps, &home, "a s d f g h j k l ; '", 1.75, 3);
    if iso {
        caps.push(Cap {
            part: Part::EnterTop,
            ..named("RTRN", 13.5, 2, 1.5, "enter", "RETURN")
        });
        caps.push(printing("BKSL", 12.75, 3, "\\"));
        caps.push(Cap {
            part: Part::EnterFoot,
            ..named("RTRN", 13.75, 3, 1.25, "enter", "")
        });
        caps.push(named("LFSH", 0., 4, 1.25, "shift", "SHIFT"));
        caps.push(printing("LSGT", 1.25, 4, "<"));
    } else {
        caps.push(Cap {
            w: 1.5,
            ..printing("BKSL", 13.5, 2, "\\")
        });
        caps.push(named("RTRN", 12.75, 3, 2.25, "enter", "RETURN"));
        caps.push(named("LFSH", 0., 4, 2.25, "shift", "SHIFT"));
    }
    let bottom = [
        "AB01", "AB02", "AB03", "AB04", "AB05", "AB06", "AB07", "AB08", "AB09", "AB10",
    ];
    run(caps, &bottom, "z x c v b n m , . /", 2.25, 4);
    caps.push(named("RTSH", 12.25, 4, 2.75, "shift", "SHIFT"));
    for (id, x, w, key, name) in [
        ("LCTL", 0., 1.25, "control", "CTRL"),
        ("LWIN", 1.25, 1.25, "platform", "SUPER"),
        ("LALT", 2.5, 1.25, "alt", "ALT"),
        ("SPCE", 3.75, 6.25, "space", ""),
        ("RALT", 10., 1.25, "", "ALTGR"),
        ("RWIN", 11.25, 1.25, "platform", "SUPER"),
        ("MENU", 12.5, 1.25, "menu", "MENU"),
        ("RCTL", 13.75, 1.25, "control", "CTRL"),
    ] {
        caps.push(named(id, x, 5, w, key, name));
    }
}

/// Insert, Home and Page Up over Delete, End and Page Down, and the inverted-T arrows below them.
fn navigation(caps: &mut Vec<Cap>) {
    for (id, x, row, key, name) in [
        ("INS", 15.5, 1, "insert", "INS"),
        ("HOME", 16.5, 1, "home", "HOME"),
        ("PGUP", 17.5, 1, "pageup", "PGUP"),
        ("DELE", 15.5, 2, "delete", "DEL"),
        ("END", 16.5, 2, "end", "END"),
        ("PGDN", 17.5, 2, "pagedown", "PGDN"),
        ("UP", 16.5, 4, "up", "↑"),
        ("LEFT", 15.5, 5, "left", "←"),
        ("DOWN", 16.5, 5, "down", "↓"),
        ("RGHT", 17.5, 5, "right", "→"),
    ] {
        caps.push(named(id, x, row, 1., key, name));
    }
}

/// The desktop board, ISO when the layout has a key of its own beside left Shift, otherwise ANSI.
pub fn caps(iso: bool) -> Vec<Cap> {
    let mut caps = Vec::new();
    function_row(&mut caps);
    main_block(&mut caps, iso);
    navigation(&mut caps);
    caps
}

#[cfg(test)]
#[path = "physical_tests.rs"]
mod tests;
