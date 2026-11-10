//! The live menu's captured state and painted inventory, in the existing UI observer shapes.
use super::{glyph, label, lead_name, Card, MenuAdapter};
use crate::{
    g2::context_menus::{
        join::Joined,
        lines::{lead, Ink},
        menu::Face,
    },
    ipc::Adapter,
};
use flea::{
    i18n::I18n,
    ipc::wire::Reply,
    json::escape,
    ui_logic_context_menus::{Entry, Text},
};

pub(crate) struct JoinedAdapter<'a> {
    pub joined: &'a Joined,
    pub identity: String,
    pub catalog: &'a I18n,
    pub field: Option<std::rc::Rc<std::cell::RefCell<super::super::menu::Menu>>>,
}

fn row(entry: Row<'_>) -> String {
    let glyph = if entry.mark.is_some() {
        "null".into()
    } else {
        format!("\"{}\"", escape(entry.glyph))
    };
    let mark = entry.mark.map_or(String::new(), |mark| {
        format!(",\"mark\":\"{}\"", escape(mark))
    });
    format!(
        "{{\"id\":\"{}\",\"action\":\"{}\",\"label\":\"{}\",\"glyph\":{glyph}{mark},\"disabled\":{},\"errored\":{},\"danger\":{},\"hint\":\"{}\"{}{}}}",
        escape(entry.id),
        escape(entry.action),
        escape(entry.label),
        entry.disabled,
        entry.ink == Ink::Errored,
        entry.ink == Ink::Danger,
        escape(entry.hint),
        entry.icon.map_or(String::new(), |icon| format!(",\"icon\":\"{}\"", escape(icon))),
        entry
            .leaves
            .map_or(String::new(), |leaves| format!(",\"submenu\":[{leaves}]"))
    )
}

struct Row<'a> {
    id: &'a str,
    action: &'a str,
    label: &'a str,
    glyph: &'a str,
    mark: Option<&'a str>,
    icon: Option<&'a str>,
    disabled: bool,
    ink: Ink,
    hint: &'a str,
    leaves: Option<String>,
}

fn brand_name(brand: flea::ui_logic_context_menus::Brand) -> &'static str {
    match brand {
        flea::ui_logic_context_menus::Brand::Tailscale => "tailscale",
        flea::ui_logic_context_menus::Brand::Dropbox => "dropbox",
        flea::ui_logic_context_menus::Brand::LocalSend => "localsend",
        flea::ui_logic_context_menus::Brand::AirDrop => "airdrop",
    }
}

fn ink(entry: &Entry) -> Ink {
    if entry.errored {
        Ink::Errored
    } else if entry.danger {
        Ink::Danger
    } else {
        Ink::Plain
    }
}

// A flyout leaf is not shaped until its flyout opens, so its words resolve through the catalog as the build would.
fn leaf(entry: &Entry, catalog: &I18n) -> String {
    if entry.separator {
        return "{\"separator\":true}".into();
    }
    let text = |text: &Option<Text>| {
        text.as_ref()
            .map_or(String::new(), |text| text.resolve(catalog))
    };
    let output = row(Row {
        id: &entry.id,
        action: &entry.action,
        label: &text(&entry.label),
        glyph: lead_name(&lead(entry)),
        mark: entry.brand.map(brand_name),
        icon: entry.icon.as_deref(),
        disabled: entry.disabled,
        ink: ink(entry),
        hint: &text(&entry.hint),
        leaves: entry.submenu.as_ref().map(|leaves| {
            leaves
                .iter()
                .map(|leaf_entry| leaf(leaf_entry, catalog))
                .collect::<Vec<_>>()
                .join(",")
        }),
    });
    with_tag_state(output, entry.tag_state)
}

fn with_tag_state(
    mut output: String,
    state: Option<flea::ui_logic_rail_places::tags::State>,
) -> String {
    if let Some(state) = state {
        let state = match state {
            flea::ui_logic_rail_places::tags::State::Applied => "applied",
            flea::ui_logic_rail_places::tags::State::Mixed => "mixed",
            flea::ui_logic_rail_places::tags::State::Absent => "absent",
        };
        output.pop();
        output.push_str(&format!(",\"tagRow\":{{\"state\":\"{state}\"}}}}"));
    }
    output
}

pub(crate) fn model(card: Option<&Card>, catalog: &I18n) -> String {
    let rows = card
        .map(|card| {
            card.built
                .lines
                .iter()
                .zip(&card.entries)
                .map(|(line, entry)| {
                    if line.separator {
                        return "{\"separator\":true}".into();
                    }
                    with_tag_state(
                        row(Row {
                            id: &line.id,
                            action: &line.action,
                            label: label(line),
                            glyph: glyph(line),
                            mark: match &line.lead {
                                super::super::lines::Lead::Brand(brand) => Some(brand_name(*brand)),
                                _ => None,
                            },
                            icon: entry.icon.as_deref(),
                            disabled: line.disabled,
                            ink: line.ink,
                            hint: line.word.as_ref().map_or("", |word| word.text.as_str()),
                            leaves: entry.submenu.as_ref().map(|leaves| {
                                leaves
                                    .iter()
                                    .map(|entry| leaf(entry, catalog))
                                    .collect::<Vec<_>>()
                                    .join(",")
                            }),
                        }),
                        line.tag_state,
                    )
                })
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    format!("[{}]", rows.join(","))
}

// Each row's laid-out segments in painter order, summing to the row's own `wanted` width: the frame's edge and padding,
// the mark slot, the label's gap, the shaped label, the gaps before and the widths of the key, word and chevron, the padding.
fn parts(card: Option<&Card>) -> String {
    let rows = card.map(|card| card.built.lines.iter().map(|line| {
        if line.separator { return "{\"separator\":true}".into(); }
        let label = line.label.as_ref().map_or(0., |label| label.width());
        let hint = [&line.key, &line.word].into_iter().flatten().map(|part| part.width()).collect::<Vec<_>>();
        let chevron = line.has_trailing_column();
        let metrics = card.built.metrics;
        let padding = metrics.pad_x + line.frame_inset();
        let trailing = hint.len() + usize::from(chevron);
        let gap = |count: usize| count as f32 * metrics.gap;
        let (hint_width, hint_gaps) = (hint.iter().sum::<f32>(), hint.len().saturating_sub(1));
        let room = line.label_room(card.frame.w - 2.);
        format!("{{\"separator\":false,\"markLeft\":{},\"slot\":{},\"labelLeft\":{},\"label\":{label},\"labelRight\":{},\"labelTruncated\":{},\"hint\":{},\"hintRight\":0,\"hintWrap\":{},\"status\":0,\"chevron\":{},\"chevronRight\":{}}}", padding, metrics.slot, metrics.gap, gap(usize::from(trailing > 0)), label > room, hint_width + gap(hint_gaps), line.hint_wrap, if chevron { metrics.chevron + gap(usize::from(!hint.is_empty())) } else { 0. }, padding)
    }).collect::<Vec<String>>()).unwrap_or_default();
    format!("[{}]", rows.join(","))
}

impl Adapter for JoinedAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let menu = self.joined.menu.borrow();
        let base = MenuAdapter {
            menu: &menu,
            origin: (0., 0.),
        };
        match function {
            "menuState" => {
                let mut state = base.state();
                state.pop();
                let counts = &self.joined.presentation.counts;
                let atomic = if counts.available {
                    format!(
                        "{{\"holds\":{},\"fallbacks\":{},\"blankFrames\":{},\"loadingFrames\":{},\"blinkBacks\":{},\"clockId\":{}}}",
                        counts.holds,
                        counts.fallbacks,
                        counts.blank,
                        counts.loading,
                        counts.blink_backs,
                        counts.clock_id.map_or("null".into(), |id| id.to_string())
                    )
                } else {
                    "null".into()
                };
                state.push_str(&format!(",\"atomic\":{atomic}"));
                let capture = self.joined.captured.as_ref();
                let kinds = capture
                    .map(|c| {
                        c.kinds
                            .iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default();
                state.push_str(&format!(
                    ",\"snapshotPath\":\"{}\",\"snapshotRowName\":\"{}\"",
                    escape(capture.map_or("", |capture| capture.path.as_str())),
                    escape(
                        capture
                            .and_then(|capture| capture.target.as_ref())
                            .map_or("", |row| row.name.as_str())
                    )
                ));
                let work = menu.work;
                state.push_str(&format!(
                    ",{},\"rows\":{}",
                    self.joined.shebang.members(),
                    parts(menu.card.as_ref())
                ));
                state.push_str(&format!(",\"entries\":{},\"submenuEntries\":{},\"snapshotReady\":{},\"snapshotId\":{},\"snapshotKinds\":[{kinds}],\"snapshotIdentity\":\"{}\",\"menuSelectionIdentity\":\"{}\",\"hasRow\":{},\"forHeader\":{},\"forRail\":{},\"workArea\":{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}}}", model(menu.card.as_ref(), self.catalog), model(menu.flyout.as_ref().map(|f| &f.card), self.catalog), capture.is_some_and(|c| c.ready), capture.map_or(0, |c| c.id), escape(capture.map_or("", |c| c.identity.as_str())), escape(&self.identity), capture.is_some_and(|c| !c.background), menu.face == Face::Header, matches!(menu.face, Face::Place | Face::Rail | Face::Locked), work.x, work.y, work.w, work.h));
                Some(Reply::value(state))
            }
            "contextMenuModel" => Some(Reply::value(model(menu.card.as_ref(), self.catalog))),
            "contextMenuRowProbe" => {
                Some(base.row_probe(arguments, self.joined.pointer, self.joined.resting))
            }
            // A field's own text menu card (the path bar, a card field, Settings search) answers the contextMenu readers while it shows.
            _ if !menu.opened() && function.starts_with("contextMenu") => {
                match self.field.as_ref() {
                    Some(field) => MenuAdapter {
                        menu: &field.borrow(),
                        origin: (0., 0.),
                    }
                    .call(function, arguments),
                    None => base.call(function, arguments),
                }
            }
            _ => base.call(function, arguments),
        }
    }
}
