//! The window's footer joins the pane count and disk facts around fb1's message service.
use crate::{
    g2::fb1::{
        line::{ArmLost, MessageLine},
        observe::Paint,
    },
    list::{Metrics, Palette, Typeface},
    palette::Theme,
    pane::states::Kind,
    state::Shell,
    strips::status_host::{self, Listing},
};
use flea::{
    i18n::I18n,
    ui_logic_fb1::{messages::Messages, status::Hint},
};
use gpui::{div, prelude::*, px, AppContext, Context, Entity, Render, SharedString, Window};
use std::sync::Arc;

#[path = "status_filesystem.rs"]
mod filesystem;
pub(crate) use filesystem::Filesystem;

#[cfg(test)]
#[path = "status_paint_tests.rs"]
pub(crate) mod paint_tests;

pub(crate) struct Footer {
    pub line: Entity<MessageLine>,
    pub(super) external_line: Option<Entity<MessageLine>>,
    pub painted: Paint,
    pub path: String,
    pub total: usize,
    pub selected: usize,
    pub kind: Kind,
    pub counts: SharedString,
    pub filesystem: SharedString,
    pub border_width: gpui::Pixels,
    pub width: f32,
    pub zone_width: f32,
    probe: Option<crate::visual::Probe>,
    revision: u64,
    _changed: gpui::Subscription,
}

impl Footer {
    pub(crate) fn message_line(&self) -> Entity<MessageLine> {
        self.external_line.as_ref().unwrap_or(&self.line).clone()
    }
    fn new(preset: &str, catalog: Arc<I18n>, cx: &mut Context<Self>) -> Self {
        let line = cx.new(|cx| MessageLine::new(preset, catalog, cx));
        let changed = cx.observe(&line, |_, _, cx| cx.notify());
        Self {
            line,
            external_line: None,
            painted: Default::default(),
            path: String::new(),
            total: 0,
            selected: 0,
            kind: Kind::Loading,
            counts: SharedString::default(),
            filesystem: SharedString::default(),
            border_width: px(0.),
            width: 0.,
            zone_width: 0.,
            probe: None,
            revision: 0,
            _changed: changed,
        }
    }
}

impl Shell {
    pub(crate) fn footer(&mut self, cx: &mut Context<Self>) -> Entity<Footer> {
        let preset = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default");
        let catalog = self.locale.catalog.clone();
        let footer = self.surfaces[0]
            .footer
            .get_or_insert_with(|| cx.new(|cx| Footer::new(preset, catalog, cx)))
            .clone();
        if self.surfaces[0].footer_watch.is_none() {
            let line = footer.read_with(cx, |footer, _| footer.line.clone());
            self.surfaces[0].footer_watch = Some(cx.observe(&line, |shell, line, cx| {
                let detail = line.read_with(cx, |line, _| line.drawn().detail.clone());
                if shell.surfaces[0].detail_text != detail {
                    shell.surfaces[0].detail_text = detail;
                    cx.notify();
                }
            }));
        }
        footer
    }

    /// Producers publish through the same service that the footer paints and the IPC readers observe.
    pub fn status_messages<R>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Messages, u64) -> R,
    ) -> R {
        self.footer(cx)
            .read_with(cx, |footer, _| footer.line.clone())
            .update(cx, |line, cx| line.update(cx, change))
    }

    pub(crate) fn status_refused(
        &mut self,
        operation: &str,
        message: &str,
        named: &str,
        cx: &mut Context<Self>,
    ) {
        let sentence = flea::ui_logic_fb1::errors::sentence(operation, message, named)
            .render(&self.locale.catalog);
        self.status_messages(cx, |messages, now| {
            messages.say(&sentence, operation != "sort", "", Hint::None, now)
        });
    }

    pub(crate) fn sync_footer(&mut self, width: f32, cx: &mut Context<Self>) {
        let footer = self.footer(cx);
        let Some(host) = &self.panes[self.focus_side] else {
            return;
        };
        let kind = host.pane.state.kind;
        let total = host.pane.held.total;
        let selected = host.selected.len();
        let catalog = &self.locale.catalog;
        let fs = &mut self.surfaces[self.focus_side].filesystem;
        fs.enter(&host.path);
        if !host.pane.busy() && host.pane.held.listing != fs.listing && !host.stopped {
            fs.listing = host.pane.held.listing;
            if let Some(backend) = &host.backend {
                let _ = backend.fsinfo();
            }
        }
        let free = fs
            .facts
            .as_ref()
            .map(|facts| catalog.locale().format_size(facts.free));
        let filesystem = status_host::disk(
            catalog,
            fs.facts.as_ref().map_or("", |facts| facts.name.as_str()),
            free.as_deref(),
            !fs.answered,
        );
        let held = &host.pane.held;
        let bytes = if selected <= held.rows.len() {
            // A folder counts by its finished walk, as ui/js/Status.js selectionBytes reads DirSizes.
            let mut sizes = Vec::with_capacity(selected);
            let mut walked = Vec::with_capacity(selected);
            let mut dirs = Vec::with_capacity(selected);
            for &index in &host.selected {
                let row = index
                    .checked_sub(held.start)
                    .and_then(|at| held.rows.get(at));
                let dir = row.is_some_and(|row| row.dir);
                let walk = dir
                    .then(|| host.sizes.walked(held.listing, index))
                    .flatten();
                sizes.push(if dir { walk } else { row.map(|row| row.size) });
                walked.push(walk.is_some());
                dirs.push(dir);
            }
            status_host::selection_bytes(selected, held.rows.len(), &sizes, &walked, &dirs)
        } else {
            -1
        };
        let listing = match kind {
            Kind::Ready => Listing::Ready(total),
            Kind::Empty => Listing::Empty,
            Kind::Locked => Listing::Locked,
            Kind::Error => Listing::Error,
            Kind::Loading | Kind::Waiting => Listing::Loading,
        };
        let counts = status_host::left(
            catalog,
            listing,
            "",
            Some((
                selected,
                bytes,
                catalog.locale().format_size(bytes.max(0) as u64),
            )),
            None,
        );
        let path = host.path.clone();
        let state_message = host.state_text.message.clone();
        let revision = self.locale.revision;
        let catalog = self.locale.catalog.clone();
        let probe = self
            .visual
            .pending
            .as_ref()
            .map(|_| self.visual.probe.clone());
        footer.update(cx, |footer, cx| {
            let changed = footer.path != path
                || footer.total != total
                || footer.selected != selected
                || footer.kind != kind
                || footer.counts.as_ref() != counts
                || footer.filesystem.as_ref() != filesystem
                || footer.width != width
                || footer.revision != revision;
            footer.path = path.clone();
            footer.total = total;
            footer.selected = selected;
            footer.kind = kind;
            footer.counts = counts.into();
            footer.width = width;
            footer.filesystem = filesystem.into();
            footer.probe = probe;
            footer.zone_width = ((width - 28.).max(0.) / 3.).round();
            footer.line.update(cx, |line, cx| {
                if footer.revision != revision {
                    line.translated(catalog, cx);
                }
                if changed {
                    if let Some(arm) = line.update(cx, |m, now| m.set_path(&path, now)) {
                        cx.emit(ArmLost(arm));
                    }
                }
                line.set_state_message(&state_message, cx);
            });
            footer.revision = revision;
        });
        self.slots.status = Some(footer.into());
    }

    pub(crate) fn status_escape(&mut self, cx: &mut Context<Self>) -> bool {
        if self.panes[self.focus_side].as_ref().is_some_and(|host| {
            host.pane.state.search
                || host.pane.swap.session.open
                || !host.pane.state.query.is_empty()
        }) {
            return false;
        }
        let Some(footer) = &self.surfaces[0].footer else {
            return false;
        };
        let line = footer.read_with(cx, |footer, _| footer.line.clone());
        let error = line.read_with(cx, |line, _| line.messages().shown().is_error);
        if error {
            line.update(cx, |line, cx| line.update(cx, |m, now| m.dismiss(now)));
        }
        error
    }

    pub(crate) fn sync_footer_height(&mut self, window: &Window, cx: &mut Context<Self>) {
        use crate::g2::fb1::line::{DETAIL_LINE, DETAIL_PAD_X, DETAIL_PAD_Y};
        let detail = self.surfaces[0]
            .footer
            .as_ref()
            .map(|footer| {
                footer.read_with(cx, |footer, cx| {
                    footer
                        .message_line()
                        .read_with(cx, |line, _| line.drawn().detail.clone())
                })
            })
            .unwrap_or_default();
        let height = if detail.is_empty() {
            0.
        } else {
            let typeface = self.active_typeface();
            let run = gpui::TextRun {
                len: detail.len(),
                font: typeface.font,
                color: Default::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let width = (window.viewport_size().width - px(2. * DETAIL_PAD_X)).max(px(1.));
            let lines: usize = window
                .text_system()
                .shape_text(detail, typeface.caption, &[run], Some(width), None)
                .map_or(1, |lines| {
                    lines
                        .iter()
                        .map(|line| line.wrap_boundaries().len() + 1)
                        .sum()
                });
            (lines as f32 * DETAIL_LINE + 2. * DETAIL_PAD_Y)
                .min(f32::from(window.viewport_size().height) / 3.)
        };
        self.surfaces[0].detail_height = height;
    }
}

impl Render for Footer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.painted.clear();
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let ink = Palette::inherited(&window.text_style(), theme);
        let typeface = Typeface::for_window(window, cx);
        let height = Metrics::for_typeface(&typeface).header;
        let style = gpui::TextStyle {
            font_family: typeface.font.family.clone(),
            font_size: typeface.caption.into(),
            ..window.text_style()
        };
        let disk_width = window
            .text_system()
            .layout_line(
                &self.filesystem,
                typeface.caption,
                &[style.to_run(self.filesystem.len())],
                None,
            )
            .width;
        let disk_width = f32::from(disk_width).min(self.zone_width);
        let room = (self.width - 28. - disk_width - (14. + self.zone_width + 14.)).max(0.);
        let text = |name, value: SharedString| {
            let element = self.painted.watch(
                div()
                    .relative()
                    .w(px(self.zone_width))
                    .h(px(height))
                    .line_height(px(height))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(value.clone()),
                name,
                value.clone(),
            );
            if name == "counts" {
                crate::visual::geometry::label(
                    element,
                    "status-text".into(),
                    "status".into(),
                    value,
                    0.,
                    false,
                    self.probe.clone(),
                )
            } else {
                element
            }
        };
        let root = div()
            .relative()
            .w_full()
            .h(px(height))
            .flex_none()
            .border(self.border_width)
            // HIG list and rail-favorites: the status bar stands on --surface under its --hair edge.
            .bg(theme.surface)
            .font_family(typeface.font.family)
            .text_size(typeface.caption)
            .text_color(ink.foreground)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .w_full()
                    .h(px(1.))
                    .bg(ink.foreground.opacity(0.12)),
            )
            .child(text("counts", self.counts.clone()).absolute().left(px(14.)))
            .child(
                text("disk", self.filesystem.clone())
                    .absolute()
                    .right(px(14.))
                    .justify_end(),
            )
            .child(
                self.painted.watch(
                    div()
                        .absolute()
                        .right(px(28. + disk_width))
                        .w(px(room))
                        .h(px(height))
                        .child(self.message_line()),
                    "lane",
                    SharedString::default(),
                ),
            );
        let footer = div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.painted.watch(root, "frame", SharedString::default()))
            .children(
                self.message_line()
                    .read_with(cx, |line, cx| line.detail(window, cx)),
            );
        #[cfg(test)]
        let footer = paint_tests::watch(footer, self, cx);
        footer
    }
}
