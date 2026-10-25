//! The per-type views the preview column mounts (PreviewColumn.qml): code and text, Markdown, CSV, HTML, archives and office documents.
use super::wire::{code_reply, html_reply, markdown_answer, Wire, SLOT};
use super::*;
use crate::g2::pv2::{
    self,
    archive::ArchiveView,
    code::{CodeView, Mark},
    csv::{CsvView, TableFace, TablePalette},
    html::HtmlView,
    markdown::{Faces, MarkdownView, Tokens},
    office::OfficeView,
};
use gpui::{AnyElement, AppContext, IntoElement, Window};

/// One mounted per-type view.
pub enum Doc {
    Code(Entity<CodeView>),
    Markdown(Entity<MarkdownView>),
    Table(Entity<CsvView>),
    Html(Entity<HtmlView>),
    Archive(Entity<ArchiveView>),
    Office(Entity<OfficeView>),
}

/// The theme and type the views draw with, read once per mount.
pub(crate) struct Look {
    pub theme: crate::palette::Theme,
    pub face: crate::list::Typeface,
    pub metrics: crate::list::Metrics,
    pub muted: gpui::Hsla,
    pub mark: Mark,
}
/// The views' line pitch, as a multiple of the caption size.
const LINE_PITCH: f32 = 1.3;
/// The opacity of an archive's "and more" line.
const MORE_OPACITY: f32 = 0.6;

impl Look {
    fn row(&self) -> f32 {
        (f32::from(self.face.caption) * LINE_PITCH).round()
    }
    fn state_mark(&self) -> f32 {
        crate::window::preview_facts::state_mark_size(&self.face)
    }
    fn code(&self, numbered: bool) -> CodeView {
        let m = &self.metrics;
        let mut view = CodeView::new(
            SLOT,
            pv2::code::Typeface {
                font: self.face.font.clone(),
                body: self.face.caption,
                caption: self.face.caption,
            },
            pv2::code::Palette {
                foreground: self.theme.foreground,
                muted: self.muted,
                muted_frame: self.theme.muted_frame,
                gutter: self.theme.gutter,
                inks: {
                    let i = self.theme.code_inks;
                    [self.theme.foreground, i[0], i[1], i[2], i[3], i[4], i[5]]
                },
            },
            pv2::code::Metrics {
                inset: m.gap,
                top: m.gap,
                right: m.gap,
                row: self.row(),
                gap: m.gap,
                state_mark: self.state_mark(),
            },
            self.mark.clone(),
        );
        view.numbered = numbered;
        view
    }
    fn table(&self) -> (TableFace, TablePalette) {
        (
            TableFace {
                font: self.face.font.clone(),
                caption: self.face.caption,
                line: self.row(),
                row_padding_x: self.metrics.pad,
                hairline: 1.,
            },
            TablePalette {
                foreground: self.theme.foreground,
                bright: self.theme.foreground,
                muted: self.muted,
            },
        )
    }
    fn pdf(&self) -> (pv2::Palette, pv2::Metrics) {
        (
            pv2::Palette {
                background: self.theme.background,
            },
            pv2::Metrics {
                gap: self.metrics.gap,
                margin: 2. * self.metrics.pad,
                row_height: self.metrics.row,
            },
        )
    }
}

impl Doc {
    /// The view a kind mounts, or None for a kind the frame draws itself.
    pub(crate) fn mount(
        kind: Kind,
        subject: &Subject,
        look: &Look,
        wire: &Wire,
        catalog: &Arc<flea::i18n::I18n>,
        window: &mut Window,
        cx: &mut Context<SelectionPreview>,
    ) -> Option<Self> {
        let path = subject.path();
        let size = subject.row.size;
        Some(match kind {
            Kind::Text | Kind::Code => Doc::Code(cx.new(|cx| {
                let mut view = look.code(kind == Kind::Code);
                view.set_host(Some(Box::new(wire.clone())), cx);
                view.set_catalog(catalog.clone(), cx);
                view.set_active(true, cx);
                view.set_subject(&path, size, false, None, cx);
                view
            })),
            Kind::Markdown => Doc::Markdown(cx.new(|cx| {
                let m = &look.metrics;
                let tokens = Tokens {
                    body: f32::from(look.face.body_small),
                    theme_body: f32::from(look.face.body),
                    caption: f32::from(look.face.caption),
                    gap: m.gap,
                    hairline: m.hairline,
                    row_pad_x: m.pad,
                    row_pad_y: (6. * f32::from(look.face.base) / 12.).round(),
                    mark: m.mark,
                    state_mark: look.state_mark(),
                    column: true,
                };
                let t = &look.theme;
                let palette = pv2::markdown::Palette {
                    background: t.background,
                    code: t.surface,
                    surface: t.surface,
                    foreground: t.foreground,
                    bright: t.foreground,
                    muted: look.muted,
                    muted_frame: t.muted_frame,
                    accent_frame: t.accent_frame,
                    inks: t.code_inks,
                };
                let faces = Faces {
                    family: look.face.font.family.to_string(),
                    ..Faces::omarchy_default()
                };
                let mut view = MarkdownView::new(palette, tokens, faces, SLOT, catalog, cx);
                view.set_code_host(Box::new(wire.clone()), cx);
                view.set_scale(window);
                view.set_active(true, cx);
                view.set_path(Some(path.clone().into()), cx);
                view
            })),
            Kind::Table => Doc::Table(cx.new(|cx| {
                let (face, palette) = look.table();
                let mut view = CsvView::new(false, face, palette);
                view.set_host(Box::new(wire.clone()));
                view.set_path(&path, cx);
                view
            })),
            Kind::Html => Doc::Html(cx.new(|cx| {
                let (palette, _) = look.pdf();
                let code = look.code(true);
                let parts = pv2::html::SourceParts {
                    typeface: code.typeface.clone(),
                    palette: code.palette,
                    metrics: code.metrics,
                };
                let metrics = pv2::html::Metrics {
                    row_height: look.row(),
                    radius: 0.,
                    round_top: true,
                };
                let mut view = HtmlView::new(SLOT, palette, metrics, parts, look.mark.clone(), cx);
                let source = wire.clone();
                view.set_hosts(
                    Some(Box::new(wire.clone())),
                    Some(Box::new(move || {
                        Box::new(source.clone()) as Box<dyn pv2::code::CodeHost>
                    })),
                );
                view.set_catalog(catalog.clone(), cx);
                view.set_path(&path, size, false, cx);
                view.set_active(true, cx);
                view
            })),
            Kind::Archive => Doc::Archive(cx.new(|cx| {
                let m = &look.metrics;
                let folder = crate::row::marks::Mark::Folder.svg();
                let file = crate::row::marks::Mark::File.svg();
                let mut view = ArchiveView::new(
                    pv2::archive::Palette {
                        foreground: look.theme.foreground,
                        muted: look.muted,
                        muted_frame: look.theme.muted_frame,
                    },
                    pv2::archive::Metrics {
                        inset: m.gap,
                        gap: m.gap,
                        row: look.row(),
                        glyph: f32::from(look.face.caption),
                        font: look.face.font.clone(),
                        name: look.face.body,
                        caption: look.face.caption,
                        more_opacity: MORE_OPACITY,
                    },
                    look.mark.clone(),
                    file,
                    folder,
                );
                view.column = true;
                view.set_host(Some(Box::new(wire.clone())));
                view.set_catalog(catalog.clone(), cx);
                view.set_subject(&path, cx);
                view.set_active(true, cx);
                view
            })),
            Kind::Office => Doc::Office(cx.new(|cx| {
                let (palette, metrics) = look.pdf();
                let (face, table) = look.table();
                let mut view = OfficeView::new(false, palette, metrics, face, table, cx);
                view.set_host(Box::new(wire.clone()), Box::new(wire.clone()), cx);
                view.set_path(&path, window, cx);
                view
            })),
            _ => return None,
        })
    }

    pub(crate) fn observe(&self, cx: &mut Context<SelectionPreview>) -> Subscription {
        match self {
            Doc::Code(view) => cx.observe(view, |_, _, cx| cx.notify()),
            Doc::Markdown(view) => cx.observe(view, |_, _, cx| cx.notify()),
            Doc::Table(view) => cx.observe(view, |_, _, cx| cx.notify()),
            Doc::Html(view) => cx.observe(view, |_, _, cx| cx.notify()),
            Doc::Archive(view) => cx.observe(view, |_, _, cx| cx.notify()),
            Doc::Office(view) => cx.observe(view, |_, _, cx| cx.notify()),
        }
    }

    /// One backend line for this view; true when it took it.
    pub(crate) fn on_line(
        &self,
        doc: &Json,
        line: &str,
        path: &str,
        window: &mut Window,
        cx: &mut Context<SelectionPreview>,
    ) -> bool {
        let kind = doc.get("t").and_then(Json::as_str).unwrap_or("");
        let slot = doc.get("slot").and_then(Json::as_str);
        match (self, kind) {
            (_, "code") if slot != Some(SLOT) => false,
            (Doc::Code(view), "code") => {
                let reply = code_reply(doc);
                let mine = view.read(cx).lines.token == reply.token;
                view.update(cx, |view, cx| view.reply(reply, cx));
                mine
            }
            (Doc::Markdown(view), "code") => {
                view.update(cx, |view, cx| view.code_answered(markdown_answer(doc), cx));
                true
            }
            (Doc::Table(view), "code") => view.update(cx, |view, cx| view.answered(line, cx)),
            (Doc::Html(view), "code") => {
                let Some(source) = view.read(cx).source.clone() else {
                    return false;
                };
                source.update(cx, |code, cx| code.reply(code_reply(doc), cx));
                true
            }
            (Doc::Office(view), "code") => view
                .read(cx)
                .table
                .clone()
                .update(cx, |table, cx| table.answered(line, cx)),
            (Doc::Html(view), "html") if slot == Some(SLOT) => {
                view.update(cx, |view, cx| view.reply(html_reply(doc), cx));
                true
            }
            (Doc::Office(view), _) => view.update(cx, |view, cx| view.reply(line, window, cx)),
            (Doc::Archive(view), "meta") => {
                let Some(meta) = flea::ui_logic_pv1::meta::parse(line) else {
                    return false;
                };
                let names = meta.names.into_iter().map(|e| (e.name, e.is_dir)).collect();
                let archive = flea::ui_logic_pv2::archive::Meta {
                    entries: meta.entries.unwrap_or(0),
                    unpacked: meta.unpacked,
                    failed: meta.archive_failed,
                    names,
                };
                view.update(cx, |view, cx| view.answered(path, archive, cx))
            }
            _ => false,
        }
    }

    /// The view has its first screen, its refusal or its failure to show, so the swap may draw it.
    pub(crate) fn ready(&self, cx: &gpui::App) -> bool {
        match self {
            Doc::Code(view) => view.read(cx).first_screen(),
            Doc::Markdown(view) => !view.read(cx).loading(cx),
            Doc::Table(view) => {
                let table = view.read(cx);
                table.first_screen() || table.feed.answer.refused()
            }
            Doc::Html(view) => {
                let html = view.read(cx);
                html.first_screen(cx) || html.refusal(cx).is_some()
            }
            Doc::Archive(view) => view.read(cx).meta.is_some(),
            Doc::Office(view) => view.read(cx).shown(cx),
        }
    }

    /// The one sentence a failed read draws in the frame's error state.
    pub(crate) fn failure(&self, catalog: &flea::i18n::I18n, cx: &gpui::App) -> Option<String> {
        match self {
            Doc::Code(view) => view.read(cx).fail_sentence(catalog),
            Doc::Markdown(view) => view
                .read(cx)
                .status_sentence()
                .filter(|_| view.read(cx).doc.status == pv2::markdown::Status::Unreadable)
                .map(str::to_owned),
            Doc::Html(view) => view.read(cx).fail_sentence(catalog, cx),
            Doc::Archive(view) => view.read(cx).fail_sentence(catalog),
            Doc::Office(view) => view.read(cx).failure(catalog).map(|f| f.sentence),
            Doc::Table(_) => None,
        }
    }

    /// The lines the frame shows, for the column's text readers.
    pub(crate) fn lines(&self, cx: &gpui::App) -> Vec<String> {
        match self {
            Doc::Code(view) => view.read(cx).lines.plain_lines(),
            Doc::Archive(view) => view
                .read(cx)
                .meta
                .as_ref()
                .map_or(Vec::new(), |m| m.names.iter().map(|(n, _)| n.clone()).collect()),
            _ => Vec::new(),
        }
    }

    /// PreviewScroll.qml pageBy: one viewport height down or up, never past either end; `frame` stands in before layout.
    pub(crate) fn page(&self, direction: i64, frame: f32, cx: &mut gpui::App) {
        let by = |own: f32| direction as f32 * if own > 0. { own } else { frame };
        match self {
            Doc::Code(view) => view.update(cx, |view, cx| view.scroll_by(by(view.frame.1), cx)),
            Doc::Markdown(view) => view.update(cx, |view, cx| view.scroll_by(by(view.frame.1), cx)),
            // The table's viewport is the body under its header; layout clamps the scroll to the table's ends.
            Doc::Table(view) => view.update(cx, |view, cx| {
                view.scroll.1 += f64::from(by((view.frame.1 - view.feed.geometry.head_height).max(0.) as f32));
                cx.notify();
            }),
            Doc::Html(view) => view.update(cx, |view, cx| match view.source.clone() {
                Some(source) if view.doc.source_view() => {
                    source.update(cx, |code, cx| code.scroll_by(by(code.frame.1), cx))
                }
                _ => view.scroll_by(by(view.frame.1), cx),
            }),
            Doc::Archive(_) | Doc::Office(_) => {}
        }
    }

    pub(crate) fn close(&self, window: &mut Window, cx: &mut gpui::App) {
        match self {
            Doc::Code(view) => view.update(cx, |view, cx| view.set_active(false, cx)),
            Doc::Markdown(view) => view.update(cx, |view, cx| view.set_active(false, cx)),
            Doc::Html(view) => view.update(cx, |view, cx| view.set_active(false, cx)),
            Doc::Archive(view) => view.update(cx, |view, cx| view.set_active(false, cx)),
            Doc::Office(view) => view.update(cx, |view, cx| view.leave(window, cx)),
            Doc::Table(_) => {}
        }
    }

    pub(crate) fn element(&self) -> AnyElement {
        match self {
            Doc::Code(view) => view.clone().into_any_element(),
            Doc::Markdown(view) => view.clone().into_any_element(),
            Doc::Table(view) => view.clone().into_any_element(),
            Doc::Html(view) => view.clone().into_any_element(),
            Doc::Archive(view) => view.clone().into_any_element(),
            Doc::Office(view) => view.clone().into_any_element(),
        }
    }
}
