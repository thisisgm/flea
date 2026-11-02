//! Network card and NETWORK rail readers in ui/Ipc.qml's shapes, read from the card's model and its last painted frame.
use super::rail::{group_name, kind_name, RailAdapter};
use super::rows::{centre_of, rect_of};
use super::Adapter;
use crate::g2::pl2::{dialog::Focus, view::NetworkView};
use flea::ipc::wire::Reply;
use flea::json::escape;
use flea::ui_logic_pl2::form::{Protocol, PROTOCOLS};
use flea::ui_logic_pl4::rail::Group;

pub struct NetworkAdapter<'a> {
    pub card: Option<&'a NetworkView>,
    pub inside: bool,
    pub built: bool,
    pub rail: &'a RailAdapter<'a>,
}

/// Protocols.PROTOCOLS: the wire names a test picks a chip by, independent of the interface language.
fn protocol_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Smb => "SMB",
        Protocol::Sftp => "SFTP",
        Protocol::Ftps => "FTPS",
        Protocol::WebDav => "WebDAV",
        Protocol::Nfs => "NFS",
    }
}

/// Protocols.FIELDS pathLabel: the row's wire name per protocol.
fn path_label(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Smb => "Share",
        Protocol::Nfs => "Export",
        _ => "Path",
    }
}

/// Every card reader, so a shut card answers each of them empty and never swallows another surface's call.
const CARD: [&str; 21] = [
    "networkProtocol",
    "networkPort",
    "networkUri",
    "networkPathLabel",
    "networkTitle",
    "networkFields",
    "networkFocus",
    "networkFocusState",
    "networkHostPortWidths",
    "networkPasswordState",
    "networkPasswordEyeCentre",
    "networkNote",
    "networkPasswordHint",
    "networkAction",
    "networkStatus",
    "networkCardRect",
    "networkBodyRect",
    "networkScroll",
    "networkDialogMetrics",
    "networkDialogMetricTargets",
    "networkChipCentre",
];

fn round(value: f32) -> i64 {
    value.round() as i64
}

impl NetworkAdapter<'_> {
    /// Only an open card answers; a shut card reads as empty, the way an unloaded dialog does.
    fn open(&self) -> Option<&NetworkView> {
        self.card.filter(|card| card.model.opened)
    }

    /// A painted hit box in window coordinates.
    fn hit(&self, card: &NetworkView, focus: Focus) -> Option<(f32, f32, f32, f32)> {
        let drawn = card.drawn.borrow();
        let (x, y) = drawn.origin;
        drawn
            .hits
            .iter()
            .find(|(at, _)| *at == focus)
            .map(|(_, r)| (x + r.x, y + r.y, r.w, r.h))
    }

    fn action(card: &NetworkView) -> &str {
        let (model, words) = (&card.model, &card.words);
        if model.committed {
            words.close.as_str()
        } else if model.saving {
            words.saving.as_str()
        } else if model.connecting {
            words.connecting.as_str()
        } else if model.retry {
            words.retry.as_str()
        } else {
            words.connect.as_str()
        }
    }

    /// NetworkForm.focusName(): a field's wire name, a button's label, nothing for a chip or the eye.
    fn focus_name(card: &NetworkView) -> String {
        let protocol = card.model.form.protocol;
        match card.model.focus {
            Focus::Label => "Label",
            Focus::Host => "Host",
            Focus::Port => "Port",
            Focus::Path => path_label(protocol),
            Focus::Domain => "Domain",
            Focus::User => "Username",
            Focus::Password => "Password",
            Focus::Tls => "TLS",
            Focus::Cancel => card.words.cancel.as_str(),
            Focus::Connect => Self::action(card),
            Focus::Dropbox => card.words.install.as_str(),
            Focus::Protocol(_) | Focus::Eye => "",
        }
        .into()
    }

    /// NetworkForm.visibleFields(): the rows the chosen protocol shows, in reading order.
    fn fields(card: &NetworkView) -> String {
        let protocol = card.model.form.protocol;
        let mut out = vec!["Label", "Host", "Port", path_label(protocol)];
        if protocol == Protocol::Smb {
            out.push("Domain");
        }
        if protocol.credentials() {
            out.extend(["Username", "Password"]);
        }
        if protocol.tls_control() {
            out.push("TLS");
        }
        out.join("|")
    }

    fn card_value(&self, function: &str, arguments: &[String]) -> Option<String> {
        if !CARD.contains(&function) {
            return None;
        }
        let Some(card) = self.open() else {
            return Some(String::new());
        };
        let (model, words) = (&card.model, &card.words);
        let protocol = model.form.protocol;
        let value = match function {
            "networkProtocol" => protocol_name(protocol).into(),
            "networkPort" => model.form.port.clone(),
            "networkUri" => model.form.uri().unwrap_or_default(),
            "networkPathLabel" => path_label(protocol).into(),
            "networkTitle" => {
                let title = if model.failed_connect {
                    &words.failed_title
                } else if model.retry {
                    &words.retry_title
                } else {
                    &words.title
                };
                title.clone()
            }
            "networkFields" => Self::fields(card),
            "networkFocus" => Self::focus_name(card),
            "networkFocusState" => {
                let field = Self::focus_name(card);
                format!(
                    "{{\"inside\":{},\"busy\":{},\"activeItem\":\"{}\",\"field\":\"{}\"}}",
                    self.inside,
                    model.busy(),
                    if self.inside {
                        escape(&field)
                    } else {
                        String::new()
                    },
                    escape(&field)
                )
            }
            "networkHostPortWidths" => {
                match (self.hit(card, Focus::Host), self.hit(card, Focus::Port)) {
                    (Some(host), Some(port)) => format!("{}|{}", round(host.2), round(port.2)),
                    _ => String::new(),
                }
            }
            "networkPasswordState" => format!(
                "{}|{}",
                if model.reveal { "visible" } else { "masked" },
                if model.password.is_empty() {
                    "empty"
                } else {
                    "set"
                }
            ),
            "networkPasswordEyeCentre" => self
                .hit(card, Focus::Eye)
                .map(|(x, y, w, h)| centre_of(x, y, w, h))
                .unwrap_or_default(),
            "networkNote" => {
                if protocol == Protocol::Nfs {
                    words.nfs_hint.clone()
                } else {
                    String::new()
                }
            }
            "networkPasswordHint" => {
                if protocol.credentials() {
                    words.password_hint.clone()
                } else {
                    String::new()
                }
            }
            "networkAction" => Self::action(card).into(),
            "networkStatus" => model
                .status
                .map(|key| card.catalog.tr_args(key, &[&model.detail]))
                .unwrap_or_default(),
            "networkCardRect" | "networkBodyRect" => {
                let drawn = card.drawn.borrow();
                let rect = if function == "networkCardRect" {
                    drawn.card
                } else {
                    drawn.body()
                };
                rect.map(|r| rect_of(drawn.origin.0 + r.x, drawn.origin.1 + r.y, r.w, r.h))
                    .unwrap_or_default()
            }
            "networkScroll" => {
                let drawn = card.drawn.borrow();
                let chrome = drawn.body_height - drawn.body().map_or(0., |body| body.h);
                format!(
                    "{}|{}|{}",
                    round(card.scroll),
                    round(card.scroll_content() - chrome),
                    round(drawn.body_height - chrome)
                )
            }
            "networkDialogMetrics" => {
                let drawn = card.drawn.borrow();
                let (Some(frame), Some(body), Some(rule)) =
                    (drawn.boxes.first(), drawn.body(), drawn.boxes.get(1))
                else {
                    return Some(String::new());
                };
                format!(
                    "{}|{}",
                    round(body.x - frame.rect.x - frame.border_width),
                    round(body.y - rule.rect.y - rule.rect.h)
                )
            }
            "networkDialogMetricTargets" => format!(
                "{}|{}",
                round(flea::ui_logic_op1::card::PADDING),
                round(flea::ui_logic_op1::card::GAP)
            ),
            "networkChipCentre" => {
                let name = arguments.first().map_or("", String::as_str);
                PROTOCOLS
                    .iter()
                    .position(|protocol| protocol_name(*protocol) == name)
                    .or_else(|| words.protocols.iter().position(|label| label == name))
                    .and_then(|index| self.hit(card, Focus::Protocol(index)))
                    .map(|(x, y, w, h)| centre_of(x, y, w, h))
                    .unwrap_or_default()
            }
            _ => return None,
        };
        Some(value)
    }

    /// Sidebar.networkMarkItems(): the plus ink, its press box and the first NETWORK row's indicator dot.
    fn marks(&self) -> Option<[crate::rail::geometry::Rect; 3]> {
        if !self.rail.shown() {
            return None;
        }
        let producer = self.rail.producer()?;
        let first = producer
            .entries()
            .iter()
            .position(|entry| entry.group == Group::Network)?;
        let laid = producer.laid.borrow();
        let ink = laid.headers.iter().find_map(|header| header.add)?;
        let dot = laid.row(first)?.indicator_rect;
        Some([ink, crate::g2::pl4::rows::add_hit(ink), dot])
    }

    fn rail_value(&self, function: &str) -> Option<String> {
        let value = match function {
            "networkEntries" => {
                let catalog = self.rail.producer().map(|producer| &producer.catalog);
                self.rail
                    .entries()
                    .iter()
                    .filter(|entry| entry.group == Group::Network)
                    .map(|entry| {
                        let label = catalog.map_or_else(
                            || entry.label.to_string(),
                            |catalog| {
                                crate::g2::pl4::content::row_label(catalog, entry).to_string()
                            },
                        );
                        format!(
                            "{}|{}|{}|{}",
                            label,
                            group_name(entry.group),
                            kind_name(entry.kind),
                            entry.mounted.unwrap_or(false)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            "networkStartIndex" => self
                .rail
                .producer()
                .map_or(0, |producer| {
                    producer
                        .entries()
                        .iter()
                        .filter(|entry| {
                            matches!(
                                entry.group,
                                Group::Home
                                    | Group::Recent
                                    | Group::Trash
                                    | Group::Favourite
                                    | Group::Favorite
                                    | Group::Tag
                            )
                        })
                        .count()
                })
                .to_string(),
            "networkBuilt" => self.built.to_string(),
            "networkMarkGeometry" => self
                .marks()
                .map(|boxes| {
                    boxes
                        .iter()
                        .map(|r| {
                            format!(
                                "{} {} {:.3}",
                                round(r.x),
                                round(r.width),
                                r.x + r.width / 2.
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .unwrap_or_default(),
            "networkMarkCentre" => self
                .marks()
                .map(|[_, hit, _]| centre_of(hit.x, hit.y, hit.width, hit.height))
                .unwrap_or_default(),
            _ => return None,
        };
        Some(value)
    }
}

impl Adapter for NetworkAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        self.rail_value(function)
            .or_else(|| self.card_value(function, arguments))
            .map(Reply::value)
    }
}

/// The NETWORK row menu answers the shared menu readers while open, ahead of the pane's own closed menu.
pub struct NetworkMenu<'a> {
    pub menu: Option<&'a std::cell::RefCell<crate::g2::mn1::menu::Menu>>,
    pub catalog: &'a flea::i18n::I18n,
    pub pointer: (f32, f32),
}

impl Adapter for NetworkMenu<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let menu = self.menu?.borrow();
        if !menu.opened() {
            return None;
        }
        crate::g2::mn1::ipc::MenuAdapter {
            menu: &menu,
            origin: (0., 0.),
        }
        .rail_call(function, arguments, self.catalog, self.pointer)
    }
}
