//! The menu's two-byte backend probe and its newest accepted receipt.
use super::{delivery::string, join::ProducerReply};
use crate::state::Shell;
use flea::{json::escape, jsondoc::Json, ui_logic_context_menus::rows};
use gpui::{Context, Subscription, Window};

#[path = "makeexec.rs"]
mod makeexec;
#[cfg(test)]
#[path = "makeexec_tests.rs"]
mod tests;

/// What the probe's reply carried, as ui/IpcMenuState.qml's `shebangReply` holds it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShebangReply {
    pub pane: String,
    pub path: String,
    pub id: u64,
    pub has_shebang: bool,
}

/// The active menu's probe, with ids retained per pane across menu opens.
#[derive(Default)]
pub struct Shebang {
    // The pane's name as ui.sh compares it with the reply's pane, such as pane-B.
    pub pane: Option<String>,
    // The path the last two-byte read asked about.
    pub asked: Option<String>,
    // The id of that ask, which rises with every probe.
    pub id: Option<u64>,
    // The reply that landed for the ask in flight.
    pub reply: Option<ShebangReply>,
    pub last_reply: Option<ShebangReply>,
    // Whether the cursor row has a shebang.
    pub has: bool,
    reader: Option<Subscription>,
    sequence: [u64; 2],
    ask: Option<(usize, u64)>,
    execution: makeexec::Execution,
}

fn text(value: &Option<String>) -> String {
    value
        .as_ref()
        .map_or("null".into(), |text| format!("\"{}\"", escape(text)))
}

impl Shebang {
    /// The `pane` and `shebang*` members of `menuState`, without the braces.
    pub fn members(&self) -> String {
        let reply = |reply: &Option<ShebangReply>| {
            reply.as_ref().map_or("null".into(), |reply| {
                format!(
                    "{{\"pane\":\"{}\",\"path\":\"{}\",\"id\":{},\"hasShebang\":{}}}",
                    escape(&reply.pane),
                    escape(&reply.path),
                    reply.id,
                    reply.has_shebang
                )
            })
        };
        format!(
            "\"pane\":{},\"shebangAsked\":{},\"shebangId\":{},\"shebangReply\":{},\"shebangLastReply\":{},\"shebangHas\":{}",
            text(&self.pane),
            text(&self.asked),
            self.id.map_or("null".into(), |id| id.to_string()),
            reply(&self.reply),
            reply(&self.last_reply),
            self.has
        )
    }
}

impl Shell {
    pub(crate) fn check_menu_shebang(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.shebang.reader.is_none() {
            self.menu.shebang.reader = Some(cx.subscribe_in(
                &cx.entity(),
                window,
                |shell, _, event: &ProducerReply, window, cx| {
                    let Ok(reply) = flea::jsondoc::parse(&event.line) else {
                        return;
                    };
                    match string(&reply, "t") {
                        "shebang" => shell.menu_shebang_reply(event.side, &reply, window, cx),
                        "permissions" => {
                            shell.make_executable_reply(event.side, &reply, window, cx)
                        }
                        _ => {}
                    }
                },
            ));
        }
        let Some(capture) = &self.menu.captured else {
            return;
        };
        let side = capture.side;
        let context = &self.menu.context;
        let probe = rows::can_make_executable(
            context.row_mode,
            capture.rows.len(),
            true,
            context.cursor_is_target,
        ) && !capture.background;
        let base = self.panes[side]
            .as_ref()
            .map_or(capture.path.as_str(), |host| {
                host.search.owner.session.base_of(&capture.path)
            });
        let path = std::path::Path::new(base)
            .join(&context.row_name)
            .to_string_lossy()
            .into_owned();
        let shebang = &mut self.menu.shebang;
        shebang.pane = Some(format!("pane-{}", if side == 0 { "A" } else { "B" }));
        shebang.has = false;
        shebang.reply = None;
        shebang.asked = Some(if probe { path.clone() } else { String::new() });
        shebang.ask = probe.then_some((side, capture.id));
        if probe {
            shebang.sequence[side] += 1;
        }
        shebang.id = Some(shebang.sequence[side]);
        if probe {
            let id = shebang.sequence[side];
            self.menu_send(
                side,
                format!(
                    "{{\"c\":\"shebang\",\"path\":\"{}\",\"id\":{id}}}",
                    escape(&path)
                ),
                cx,
            );
        }
    }

    fn menu_shebang_reply(
        &mut self,
        side: usize,
        reply: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let receipt = ShebangReply {
            pane: format!("pane-{}", if side == 0 { "A" } else { "B" }),
            path: string(reply, "path").into(),
            id: number(reply, "id"),
            has_shebang: reply.get("hasShebang").and_then(Json::as_bool) == Some(true),
        };
        self.menu.shebang.last_reply = Some(receipt.clone());
        let Some(capture) = &self.menu.captured else {
            return;
        };
        let shebang = &self.menu.shebang;
        if shebang.ask != Some((side, capture.id))
            || capture.identity != self.menu_identity()
            || !flea::ui_logic_open_with_properties::permissions::lands_shebang(
                &receipt.path,
                receipt.id,
                shebang.asked.as_deref().unwrap_or(""),
                shebang.id.unwrap_or(0),
            )
        {
            return;
        }
        self.menu.shebang.has = receipt.has_shebang;
        self.menu.context.has_shebang = receipt.has_shebang;
        self.menu.shebang.reply = Some(receipt);
        if self.menu_opened() {
            let entries = self.menu_entries();
            let list = self.panes[side]
                .as_ref()
                .expect("the menu's pane")
                .list
                .borrow();
            let env = self.menu_env(&list.typeface);
            self.menu.menu.borrow_mut().refresh(entries, &env, window);
        } else {
            self.refresh_menu(window, cx);
        }
        cx.notify();
    }
}

fn number(reply: &Json, key: &str) -> u64 {
    match reply.get(key) {
        Some(Json::Num(value)) => value.parse().unwrap_or(0),
        _ => 0,
    }
}
