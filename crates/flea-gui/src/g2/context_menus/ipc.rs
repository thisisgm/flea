//! The menu's read-back readers on the IPC seam, read off the drawn rows so a test asserts what the card shows.
use super::lines::{Lead, Line};
use super::menu::{Card, Menu};
use crate::ipc::Adapter;
use flea::ipc::wire::Reply;
use flea::json::escape;
use flea::ui_logic_context_menus::Brand;
mod cohesion;
pub(super) mod joined;
pub(crate) use joined::JoinedAdapter;

pub struct MenuAdapter<'a> {
    pub menu: &'a Menu,
    // The window's offset, so a rect or centre is in window pixels as the QML readers gave them.
    pub origin: (f32, f32),
}

fn label(line: &Line) -> &str {
    if line.separator {
        "-"
    } else {
        line.label.as_ref().map_or("", |l| l.text.as_str())
    }
}

pub(super) fn glyph(line: &Line) -> &str {
    if line.separator {
        "-"
    } else {
        lead_name(&line.lead)
    }
}

pub(super) fn lead_name(lead: &Lead) -> &str {
    match lead {
        Lead::Cut(name) => name,
        Lead::Brand(Brand::Tailscale) => "tailscale",
        Lead::Brand(Brand::Dropbox) => "dropbox",
        Lead::Brand(Brand::LocalSend) => "localsend",
        Lead::Brand(Brand::AirDrop) => "airdrop",
        Lead::Busy => "omi",
        Lead::Tag(_) => "tag",
        Lead::Check(true) => "checkmark",
        Lead::Icon(name) => name,
        Lead::None | Lead::Check(false) => "",
    }
}

fn joined(card: Option<&Card>, part: impl Fn(&Line) -> &str) -> String {
    card.map(|c| c.built.lines.iter().map(part).collect::<Vec<_>>().join("|"))
        .unwrap_or_default()
}

impl MenuAdapter<'_> {
    /// A rail row's menu answers the file menu's richer readers too: its model, row probe and menuState's entries and face.
    pub(crate) fn rail_call(
        &self,
        function: &str,
        arguments: &[String],
        catalog: &flea::i18n::I18n,
        pointer: (f32, f32),
    ) -> Option<Reply> {
        use flea::jsondoc::{self, Json};
        match function {
            "cohesionControls" => self.cohesion(Some(pointer)).map(Reply::value),
            "contextMenuModel" => Some(Reply::value(self.model(catalog))),
            "contextMenuRowProbe" => Some(self.row_probe(arguments, Some(pointer), None)),
            "menuState" => {
                let mut state =
                    jsondoc::parse(self.call(function, arguments)?.stdout.trim()).ok()?;
                if let Json::Obj(values) = &mut state {
                    values.extend([
                        ("entries".into(), jsondoc::parse(&self.model(catalog)).ok()?),
                        ("forRail".into(), Json::Bool(true)),
                        ("forHeader".into(), Json::Bool(false)),
                    ]);
                }
                Some(Reply::value(jsondoc::render(&state).trim_end()))
            }
            _ => self.call(function, arguments),
        }
    }

    pub(crate) fn model(&self, catalog: &flea::i18n::I18n) -> String {
        joined::model(self.menu.card.as_ref(), catalog)
    }

    pub(crate) fn row_probe(
        &self,
        arguments: &[String],
        pointer: Option<(f32, f32)>,
        resting: Option<(f32, f32)>,
    ) -> Reply {
        let card = self.menu.card.as_ref();
        let Some((card, index)) = card.zip(
            arguments
                .first()
                .and_then(|index| index.parse::<usize>().ok()),
        ) else {
            return Reply::value("");
        };
        let Some(entry) = card.entries.get(index) else {
            return Reply::value("");
        };
        let top = card.row_screen_top(index);
        if top + card.built.metrics.height(entry) as f32
            <= card.frame.y + card.built.metrics.pad as f32
            || top >= card.frame.y + (card.built.metrics.pad + card.view_h) as f32
        {
            return Reply::value("");
        }
        let (x, y) = pointer.unwrap_or((-1., -1.));
        let (rx, ry) = resting.unwrap_or((-1., -1.));
        let hovered = card.row_at(x, y) == Some(index);
        Reply::value(format!(
            "{hovered} {} {} {} {}",
            x.round(),
            y.round(),
            rx.round(),
            ry.round()
        ))
    }

    fn rect(&self, card: Option<&Card>) -> String {
        card.map(|c| {
            let f = c.frame;
            format!(
                "{} {} {} {}",
                (f.x + self.origin.0).round(),
                (f.y + self.origin.1).round(),
                f.w.round(),
                f.h.round()
            )
        })
        .unwrap_or_default()
    }

    fn centre(&self, card: Option<&Card>, index: &str) -> String {
        let Some(card) = card else {
            return String::new();
        };
        let Some(index) = index
            .parse::<usize>()
            .ok()
            .filter(|i| *i < card.entries.len())
        else {
            return String::new();
        };
        let top = card.row_screen_top(index) - card.frame.y;
        if top + card.built.metrics.height(&card.entries[index]) as f32
            <= card.built.metrics.pad as f32
            || top >= (card.built.metrics.pad + card.view_h) as f32
        {
            return String::new();
        }
        let x = card.frame.x + card.frame.w / 2. + self.origin.0;
        let y = card.row_screen_top(index)
            + card.built.metrics.height(&card.entries[index]) as f32 / 2.
            + self.origin.1;
        format!("{} {}", x.round(), y.round())
    }

    fn state(&self) -> String {
        let menu = self.menu;
        let flyout = menu.flyout.as_ref();
        let cursor = menu
            .card
            .as_ref()
            .and_then(|c| c.cursor)
            .map_or(-1, |c| c as i64);
        format!(
            "{{\"opened\":{},\"cursor\":{cursor},\"submenu\":{},\"submenuCursor\":{},\"lone\":{},\"handle\":\"{}\",\"frame\":\"{}\",\"flyout\":\"{}\",\"scroll\":{},\"view\":{}}}",
            menu.opened(),
            flyout.is_some(),
            flyout.and_then(|f| f.card.cursor).map_or(-1, |c| c as i64),
            flyout.is_some_and(|f| f.parent.is_none()),
            escape(&menu.handle),
            self.rect(menu.card.as_ref()),
            self.rect(flyout.map(|f| &f.card)),
            menu.card.as_ref().map_or(0, |c| c.content_y),
            menu.card.as_ref().map_or(0, |c| c.view_h),
        )
    }
}

impl Adapter for MenuAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let menu = self.menu;
        let card = menu.card.as_ref();
        let flyout = menu.flyout.as_ref().map(|f| &f.card);
        let first = arguments.first().map_or("", String::as_str);
        let value = match function {
            "contextMenuVisible" => menu.opened().to_string(),
            "contextMenuEntries" => joined(card, label),
            "contextMenuHints" => joined(card, |l| {
                l.word
                    .as_ref()
                    .or(l.key.as_ref())
                    .map_or("", |k| k.text.as_str())
            }),
            "contextMenuGlyphs" => joined(card, glyph),
            "contextMenuSubmenuEntries" => joined(flyout, label),
            "contextMenuSubmenuGlyphs" => joined(flyout, glyph),
            "contextMenuCursor" => card
                .and_then(|c| c.cursor)
                .map_or(-1, |c| c as i64)
                .to_string(),
            "contextMenuRect" => self.rect(card),
            "contextMenuRowCentre" => self.centre(card, first),
            "contextMenuSubmenuRowCentre" => self.centre(flyout, first),
            "menuState" => self.state(),
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
