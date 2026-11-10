//! Feedback readers observe the live service and measurements recorded by its painted elements.
use super::{
    strips::{Snapshot, StripsAdapter},
    Adapter,
};
use crate::{
    g2::status_feedback::observe::Text,
    ipc::rows::{colour, rect_of},
    pane::states::Kind,
    state::Shell,
    visual::{number, object},
};
use flea::{ipc::wire::Reply, jsondoc::Json};
use gpui::App;

pub(crate) struct StatusAdapter<'a> {
    pub shell: &'a Shell,
    pub cx: &'a App,
}

fn encode(value: Json) -> String {
    flea::jsondoc::render_at(&value, 0)
}

pub(super) fn rect(text: &Text) -> String {
    let b = text.bounds;
    rect_of(
        f32::from(b.origin.x),
        f32::from(b.origin.y),
        f32::from(b.size.width),
        f32::from(b.size.height),
    )
}

pub(super) fn text_state(text: Option<Text>, origin: f32) -> Json {
    let visible = text.is_some();
    let text = text.unwrap_or_default();
    let width = f32::from(text.bounds.size.width);
    object([
        ("text", Json::Str(text.text.to_string())),
        ("visible", Json::Bool(visible)),
        ("width", number(width)),
        ("implicitWidth", number(text.implicit_width)),
        (
            "x",
            number((f32::from(text.bounds.origin.x) - origin).round()),
        ),
        ("truncated", Json::Bool(text.implicit_width > width)),
        ("color", Json::Str(colour(text.color))),
        ("fontSize", number(text.font_size)),
    ])
}

fn secondary(paint: &crate::g2::status_feedback::observe::Paint) -> Option<Text> {
    match (paint.get("hint"), paint.get("extra")) {
        (Some(mut hint), Some(extra)) => {
            hint.bounds = hint.bounds.union(&extra.bounds);
            hint.text = format!("{}{}", hint.text, extra.text).into();
            hint.implicit_width += extra.implicit_width;
            Some(hint)
        }
        (hint, extra) => hint.or(extra),
    }
}

impl StatusAdapter<'_> {
    fn empty(&self, function: &str) -> Option<String> {
        let side = self.shell.focus_side;
        let shown = self.shell.panes[side]
            .as_ref()
            .is_some_and(|host| host.pane.state.kind == Kind::Empty);
        if function == "emptyShown" {
            return Some(shown.to_string());
        }
        if function == "stateLayers" {
            let (empty, message) = self.shell.surfaces[side].layers(self.cx);
            // The no-match headline render.rs draws in the pane's place is a state message too (EY-470).
            let headline = self
                .shell
                .no_match_headline(side)
                .filter(|_| self.shell.slots.covers[side].is_none());
            return Some(encode(object([
                ("empty", Json::Bool(empty)),
                ("message", Json::Bool(message || headline.is_some())),
                ("headline", Json::Str(headline.unwrap_or_default())),
            ])));
        }
        if !matches!(
            function,
            "emptyHeroState" | "emptyMarkRect" | "emptyStateRect"
        ) {
            return None;
        }
        let hero = self.shell.surfaces[side]
            .hero
            .as_ref()
            .filter(|_| shown)
            .map(|hero| hero.read(self.cx));
        let Some(hero) = hero else {
            return Some(
                if function == "emptyHeroState" {
                    "{}"
                } else {
                    ""
                }
                .into(),
            );
        };
        if function != "emptyHeroState" {
            return Some(
                hero.painted
                    .get(if function == "emptyMarkRect" {
                        "mark"
                    } else {
                        "frame"
                    })
                    .as_ref()
                    .map(rect)
                    .unwrap_or_default(),
            );
        }
        let Some(frame) = hero
            .shown_frame
            .filter(|_| hero.painted.get("frame").is_some())
        else {
            return Some("{}".into());
        };
        let caption = hero.painted.get("caption");
        let ink = hero.painted_ink?;
        Some(encode(object([
            ("visible", Json::Bool(hero.rotation().visible())),
            (
                "settled",
                Json::Bool(hero.rotation().mark_settled() && frame.opacity == 1.),
            ),
            ("opacity", number(frame.opacity)),
            ("offset", number(frame.offset)),
            (
                "caption",
                Json::Str(
                    caption
                        .as_ref()
                        .map(|t| t.text.to_string())
                        .unwrap_or_default(),
                ),
            ),
            ("captionOpacity", number(frame.caption)),
            (
                "captionColor",
                Json::Str(colour(caption.map_or(ink.foreground, |t| t.color))),
            ),
            ("markColor", Json::Str(colour(hero.omi.ink()))),
            ("foreground", Json::Str(colour(ink.foreground))),
            ("muted", Json::Str(colour(ink.muted))),
            // Omi draws in the theme accent (GM 2026-11-07, HIG CHANGES empty folder).
            (
                "accent",
                Json::Str(colour(
                    self.cx
                        .try_global::<crate::palette::Theme>()
                        .copied()
                        .unwrap_or_default()
                        .accent,
                )),
            ),
            ("reducedMotion", Json::Bool(hero.reduced())),
            (
                "rotateMs",
                number(flea::ui_logic_status_feedback::empty::ROTATE_MS),
            ),
            (
                "omi",
                object([
                    ("mode", Json::Str(hero.omi.mode().id().into())),
                    ("label", Json::Str(hero.omi.label().into())),
                    ("shown", Json::Bool(hero.omi.shown())),
                    ("sampled", Json::Bool(hero.omi.counters.writes.get() > 0)),
                    ("sampledSeconds", number(hero.omi.offset())),
                    ("reducedMotion", Json::Bool(hero.omi.reduced())),
                    (
                        "slot",
                        Json::Str({
                            let slot = hero.omi.slot();
                            rect_of(
                                f32::from(slot.origin.x),
                                f32::from(slot.origin.y),
                                f32::from(slot.size.width),
                                f32::from(slot.size.height),
                            )
                        }),
                    ),
                    ("frames", number(hero.omi.counters.frames.get())),
                    ("writes", number(hero.omi.counters.writes.get())),
                    ("requests", number(hero.omi.counters.requests.get())),
                    ("heroRenders", number(hero.counters.renders.get())),
                ]),
            ),
        ])))
    }

    fn footer_state(&self, footer: &crate::window::status::Footer, common: Reply) -> Json {
        let paint = &footer.painted;
        let frame = paint.get("frame");
        let origin = frame.as_ref().map_or(0., |t| f32::from(t.bounds.origin.x));
        let message_line = footer.message_line();
        let line = message_line.read(self.cx);
        let lane = paint.get("lane");
        let Json::Obj(mut fields) = flea::jsondoc::parse(common.stdout.trim())
            .expect("the shared status reader returns JSON")
        else {
            unreachable!()
        };
        let Json::Obj(geometry) = object([
            (
                "frame",
                Json::Str(frame.as_ref().map(rect).unwrap_or_default()),
            ),
            ("borderWidth", number(f32::from(footer.border_width))),
            ("zoneWidth", number(footer.zone_width)),
            (
                "hintWidth",
                number(line.painted.get("hint").map_or(0., |t| t.implicit_width)),
            ),
            ("left", text_state(paint.get("counts"), origin)),
            ("centre", text_state(line.painted.get("primary"), origin)),
            ("secondary", text_state(secondary(&line.painted), origin)),
            ("undo", text_state(line.painted.get("undo"), origin)),
            ("disk", text_state(paint.get("disk"), origin)),
            (
                "lane",
                object([
                    (
                        "x",
                        number(
                            lane.as_ref()
                                .map_or(0., |t| f32::from(t.bounds.origin.x) - origin),
                        ),
                    ),
                    (
                        "width",
                        number(lane.map_or(0., |t| f32::from(t.bounds.size.width))),
                    ),
                ]),
            ),
        ]) else {
            unreachable!()
        };
        fields.extend(geometry);
        Json::Obj(fields)
    }
}

impl Adapter for StatusAdapter<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        if let Some(value) = self.empty(function) {
            return Some(Reply::value(value));
        }
        // The shown folder's fsinfo class, empty until its answer lands or for a local folder.
        if function == "storageClass" {
            let facts = &self.shell.surfaces[self.shell.focus_side].filesystem.facts;
            return Some(Reply::value(
                facts
                    .as_ref()
                    .map_or(String::new(), |facts| facts.class.clone()),
            ));
        }
        if !matches!(
            function,
            "lastMessage"
                | "statusPrimary"
                | "statusSecondary"
                | "statusError"
                | "statusDetail"
                | "statusActivityState"
                | "statusFooterState"
                | "stickyMessage"
                | "statusColor"
        ) {
            return None;
        }
        let Some(footer) = &self.shell.surfaces[0].footer else {
            return None;
        };
        let footer = footer.read(self.cx);
        let message_line = footer.message_line();
        let line = message_line.read(self.cx);
        let messages = line.messages();
        let shown = messages.shown();
        // StatusBar.centreColor(): the ink the centre message draws in, the foreground while none is drawn.
        if function == "statusColor" {
            let ink = match line.painted.get("primary") {
                Some(primary) => primary.color,
                None => {
                    let theme = self
                        .cx
                        .try_global::<crate::palette::Theme>()
                        .copied()
                        .unwrap_or_default();
                    crate::list::Palette::inherited(&gpui::TextStyle::default(), theme).foreground
                }
            };
            return Some(Reply::value(super::rows::colour(ink)));
        }
        let snapshot = Snapshot {
            path: footer.path.clone(),
            total: footer.total,
            selected_count: footer.selected,
            listing_state: footer.kind.name().into(),
            filesystem: footer
                .painted
                .get("disk")
                .map(|text| text.text.to_string())
                .unwrap_or_default(),
            counts: footer
                .painted
                .get("counts")
                .map(|text| text.text.to_string())
                .unwrap_or_default(),
            primary: line
                .painted
                .get("primary")
                .map(|text| text.text.to_string())
                .unwrap_or_default(),
            secondary: format!(
                "{}{}{}{}",
                line.drawn().hint,
                if line.drawn().undo.is_empty() {
                    ""
                } else {
                    flea::ui_logic_status_feedback::status::SEP
                },
                line.drawn().undo,
                line.drawn().extra
            ),
            is_error: shown.is_error,
            detail: shown.detail,
            activity_state: if function == "statusActivityState" {
                encode(self.activity_state(line))
            } else {
                String::new()
            },
            ..Snapshot::default()
        };
        let shared = StripsAdapter {
            snapshot: &snapshot,
        };
        let value = match function {
            "lastMessage" => messages.transient_text(&self.shell.locale.catalog),
            "stickyMessage" => messages
                .activities()
                .first()
                .filter(|a| !a.running)
                .map(|a| a.text.clone())
                .unwrap_or_default(),
            "statusFooterState" => encode(
                self.footer_state(
                    footer,
                    shared
                        .call(function, &[])
                        .expect("the shared footer reader"),
                ),
            ),
            _ => return shared.call(function, &[]),
        };
        Some(Reply::value(value))
    }
}
