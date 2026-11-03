// The run card's decisions: which buttons a kind draws, where the ring opens, how keys walk them and what each exit says.
use crate::i18n::I18n;
use crate::jsondoc::Json;

/// The asks `flea --open` answers beyond its 0, 2 and 3 contract, and the run card's own failures (src/runfile.rs).
pub const IS_DIRECTORY: i32 = 3;
pub const ASK_RUN: i32 = 4;
pub const ASK_OPEN_WITH: i32 = 5;
pub const RUN_CHANGED: i32 = 6;
pub const RUN_NOEXEC: i32 = 7;
pub const RUN_SPAWN: i32 = 8;
pub const RUN_HANDLER_GONE: i32 = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Script,
    Elf,
    AppImage,
}

impl Kind {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "script" => Some(Self::Script),
            "elf" => Some(Self::Elf),
            "appimage" => Some(Self::AppImage),
            _ => None,
        }
    }

    /// The ask line's word for the kind, as `parse` reads it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Elf => "elf",
            Self::AppImage => "appimage",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Script => "dialogs.file.run.kind_script",
            Self::Elf => "dialogs.file.run.kind_program",
            Self::AppImage => "dialogs.file.run.kind_appimage",
        }
    }
}

/// The card's buttons; Run is Run in Terminal held with Shift, never a fourth button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Cancel,
    Text,
    Handler,
    Terminal,
}

impl Button {
    /// RunFile.js's name for the button, as the card's IPC reader reports it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Cancel => "cancel",
            Self::Text => "text",
            Self::Handler => "handler",
            Self::Terminal => "terminal",
        }
    }
}

/// The ask line `flea --open` prints with status 4, carrying the identity the run checks again.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub kind: Kind,
    pub size: u64,
    pub handler_id: String,
    pub handler_name: String,
    pub dev: String,
    pub ino: String,
    pub mtime_s: i64,
    pub mtime_ns: i64,
    pub target: String,
}

impl Ask {
    pub fn has_handler(&self) -> bool {
        !self.handler_id.is_empty() && !self.handler_name.trim().is_empty()
    }
}

/// Sample input: {"ask":"run","kind":"script","size":9420,"handlerId":"","handlerName":"","dev":"8","ino":"9","mtimeS":11,"mtimeNs":12,"target":"/real/tool.sh"}.
pub fn parse_ask(text: &str) -> Option<Ask> {
    let message = crate::jsondoc::parse(text.trim()).ok()?;
    if message.get("ask").and_then(Json::as_str) != Some("run") {
        return None;
    }
    let kind = Kind::parse(message.get("kind").and_then(Json::as_str)?)?;
    let text = |key| message.get(key).and_then(Json::as_str).map(str::to_string);
    // A number may ride as a JSON number or as its decimal string, as the oracle's Number() read either.
    let number = |key| {
        let value = message.get(key);
        value
            .and_then(Json::as_f64)
            .or_else(|| {
                value
                    .and_then(Json::as_str)
                    .and_then(|s| s.trim().parse().ok())
            })
            .filter(|n: &f64| n.is_finite())
            .unwrap_or(0.)
    };
    Some(Ask {
        kind,
        size: number("size").max(0.) as u64,
        handler_id: text("handlerId").unwrap_or_default(),
        handler_name: text("handlerName").unwrap_or_default(),
        dev: text("dev")?,
        ino: text("ino")?,
        mtime_s: number("mtimeS") as i64,
        mtime_ns: number("mtimeNs") as i64,
        target: text("target")?,
    })
}

/// The handler where the desktop names one for an AppImage, Run in Terminal everywhere else.
pub fn default_button(kind: Kind, has_handler: bool) -> Button {
    if kind == Kind::AppImage && has_handler {
        Button::Handler
    } else {
        Button::Terminal
    }
}

/// Left to right, with Open as Text a script's alone since a binary is not text.
pub fn buttons_for(kind: Kind, has_handler: bool) -> Vec<Button> {
    let mut out = vec![Button::Cancel];
    if kind == Kind::Script {
        out.push(Button::Text);
    }
    if kind == Kind::AppImage && has_handler {
        out.push(Button::Handler);
    }
    out.push(Button::Terminal);
    out
}

/// A tweak naming a button the card did not draw falls back to the default, never to nothing.
pub fn start_focus(buttons: &[Button], tweak: Option<Button>) -> Button {
    if let Some(tweak) = tweak.filter(|t| buttons.contains(t)) {
        return tweak;
    }
    let handler = buttons.contains(&Button::Handler);
    let fallback = default_button(
        if handler {
            Kind::AppImage
        } else {
            Kind::Script
        },
        handler,
    );
    if buttons.contains(&fallback) {
        fallback
    } else {
        buttons[0]
    }
}

/// The keys the ring answers: h and l stop at the ends, Tab and Backtab come round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Left,
    Right,
    Tab,
    Backtab,
}

pub fn moved(buttons: &[Button], focus: Button, step: Step) -> Button {
    let handler = buttons.contains(&Button::Handler);
    let fallback = default_button(
        if handler {
            Kind::AppImage
        } else {
            Kind::Script
        },
        handler,
    );
    let at = buttons
        .iter()
        .position(|b| *b == focus)
        .or_else(|| buttons.iter().position(|b| *b == fallback))
        .unwrap_or(0);
    let last = buttons.len() - 1;
    match step {
        Step::Left => buttons[at.saturating_sub(1)],
        Step::Right => buttons[(at + 1).min(last)],
        Step::Tab => buttons[(at + 1) % buttons.len()],
        Step::Backtab => buttons[(at + last) % buttons.len()],
    }
}

/// The handler button is the application's own Name=; Shift relabels Run in Terminal to Run.
pub fn label_for(i18n: &I18n, button: Button, handler_name: &str, shift: bool) -> String {
    match button {
        Button::Cancel => i18n.tr("dialogs.file.cancel"),
        Button::Text => i18n.tr("dialogs.file.run.text"),
        Button::Handler => handler_name.to_string(),
        Button::Terminal if shift => i18n.tr("dialogs.file.run.run"),
        Button::Terminal => i18n.tr("dialogs.file.run.terminal"),
    }
}

/// Control and bidi characters are shown escaped, never raw, and a clean name passes through byte for byte.
pub fn escape_name(name: &str) -> String {
    let spoofed = |c: char| matches!(c as u32, 0x00..=0x1F | 0x7F..=0x9F | 0x061C | 0x200E | 0x200F | 0x2028 | 0x2029 | 0x202A..=0x202E | 0x2066..=0x2069 | 0xFEFF);
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if spoofed(c) {
            out.push_str(&format!("<U+{:04X}>", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

pub fn title(i18n: &I18n, name: &str) -> String {
    i18n.tr_args("dialogs.file.run.title", &[&escape_name(name)])
}

pub fn subtitle(i18n: &I18n, kind: Kind, bytes: u64) -> String {
    i18n.tr_args(
        "dialogs.file.run.subtitle",
        &[&i18n.tr(kind.key()), &i18n.locale().format_size(bytes)],
    )
}

/// The name row's own mark: the code glyph beside a script, the listed row's mark for a binary when one is known.
pub fn glyph_for(kind: Kind, listed: &str) -> String {
    match kind {
        Kind::Script => "code".into(),
        _ if !listed.is_empty() => listed.into(),
        Kind::Elf => "terminal".into(),
        Kind::AppImage => "app-window".into(),
    }
}

/// What a press launches; every argv carries the identity the ask named, never a shell string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Run,
    Terminal,
    Text,
    Handler,
}

pub fn argv(flea: &str, action: Action, ask: &Ask) -> Vec<String> {
    let identity = [
        ask.dev.clone(),
        ask.ino.clone(),
        ask.size.to_string(),
        ask.mtime_s.to_string(),
        ask.mtime_ns.to_string(),
    ];
    let mut out = vec![flea.to_string()];
    match action {
        Action::Run => out.extend(["--run".into(), ask.target.clone()]),
        Action::Terminal => out.extend(["--run-terminal".into(), ask.target.clone()]),
        Action::Text => return vec![flea.to_string(), "--open-text".into(), ask.target.clone()],
        Action::Handler => out.extend([
            "--open-handler".into(),
            ask.target.clone(),
            ask.handler_id.clone(),
        ]),
    }
    out.extend(identity);
    out
}

/// What the pane does after a launch exits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    Quiet,
    Navigate,
    Message(String),
}

/// Sample input: (Some(6), Handler) says the file changed so it was not opened; a launch that never started is a failure.
pub fn exit_outcome(i18n: &I18n, action: Action, status: Option<i32>) -> Exit {
    let key = |run, terminal, handler, text| match action {
        Action::Run => run,
        Action::Terminal => terminal,
        Action::Handler => handler,
        Action::Text => text,
    };
    let message = |k: &str| Exit::Message(i18n.tr(k));
    match status {
        Some(0) => Exit::Quiet,
        Some(IS_DIRECTORY) => Exit::Navigate,
        Some(RUN_CHANGED) if action == Action::Handler => message("dialogs.file.run.changed_open"),
        Some(RUN_CHANGED) => message("dialogs.file.run.changed_run"),
        Some(RUN_NOEXEC) => message("dialogs.file.run.noexec"),
        Some(RUN_HANDLER_GONE) => message("dialogs.file.run.handler_gone"),
        Some(RUN_SPAWN) => message(key(
            "dialogs.file.run.spawn_run",
            "dialogs.file.run.spawn_terminal",
            "dialogs.file.run.spawn_handler",
            "dialogs.file.run.spawn_text",
        )),
        _ => message(key(
            "dialogs.file.run.failed_run",
            "dialogs.file.run.failed_terminal",
            "dialogs.file.run.failed_handler",
            "dialogs.file.run.failed_text",
        )),
    }
}
