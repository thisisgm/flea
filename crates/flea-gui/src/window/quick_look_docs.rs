//! The per-kind views the card mounts (Preview.qml's loaders): the views the preview column mounts, asked on slot "look".
use super::{
    mount::Mount,
    wire::{Wire, COPY_SLOT, SLOT},
};
use crate::g2::{
    pv1::{
        doc::{Doc, Media, View},
        look as card,
    },
    pv2::{
        self,
        code::{CodeView, Mark},
        csv::{CsvView, TableFace, TablePalette},
        font::FontView,
        html::HtmlView,
        markdown::{style::hex, Faces, MarkdownView, Tokens},
        office::OfficeView,
        PdfView,
    },
    pv3::{self, player::MediaPlayer, surface::Facts},
    pv4::{seam::CacheReader, view::Model3dView},
};
use flea::{i18n::I18n, protocol::RowFacts, ui_logic_pv4::control::Surface};
use gpui::{px, AppContext, Context, Window};
use std::{rc::Rc, sync::Arc};

#[path = "quick_look_archive.rs"]
mod archive;

#[test]
fn visual_geometry_quick_look_plain_text_ink_starts_at_x132() {
    let look = Look {
        theme: crate::palette::Theme::default(),
        face: crate::list::Typeface::omarchy_default(),
        muted: crate::palette::Theme::default().muted,
        mark: ("file-text".into(), &[]),
    };
    assert_eq!(100. + look.code(false).text_left(px(8.4)), 132.);
}

/// The theme and type the card's views draw with, read once per mount; sizes are the boards' (ql-code: 14 on rows of 20).
pub(crate) struct Look {
    pub theme: crate::palette::Theme,
    pub face: crate::list::Typeface,
    pub muted: gpui::Hsla,
    pub mark: Mark,
}

impl Look {
    pub(super) fn markdown(
        &self,
        path: &str,
        wire: Option<&Wire>,
        catalog: &I18n,
        prepare: Option<(f32, f32)>,
        window: &Window,
        cx: &mut Context<MarkdownView>,
    ) -> MarkdownView {
        let t = &self.theme;
        let tokens = Tokens {
            body: f32::from(self.face.body),
            theme_body: f32::from(self.face.body),
            caption: card::CAPTION,
            gap: card::GAP,
            hairline: 1.,
            row_pad_x: card::PAD,
            row_pad_y: (6. * f32::from(self.face.base) / 12.).round(),
            mark: card::CHROME_MARK,
            state_mark: card::MARK,
            source_line: Some(card::TEXT_LINE),
            column: false,
        };
        let palette = pv2::markdown::Palette {
            background: t.background,
            code: t.background,
            surface: t.surface,
            foreground: t.foreground,
            bright: t.foreground_bright,
            muted: self.muted,
            figure_muted: t.muted,
            muted_frame: t.muted_frame,
            accent_frame: t.accent_frame,
            inks: t.code_inks,
        };
        let faces = Faces {
            family: self.face.font.family.to_string(),
            ..Faces::omarchy_default()
        };
        let mut view = MarkdownView::new(palette, tokens, faces, SLOT, catalog, cx);
        if let Some(wire) = wire {
            view.set_code_host(Box::new(wire.clone()), cx);
        }
        if let Some(frame) = prepare {
            view.configure_prepare(frame);
        }
        view.set_scale(window);
        view.set_active(true, cx);
        view.set_path(Some(path.into()), cx);
        view
    }

    pub(super) fn code(&self, numbered: bool) -> CodeView {
        let t = &self.theme;
        let mut view = CodeView::new(
            SLOT,
            pv2::code::Typeface {
                font: self.face.font.clone(),
                body: px(card::BODY),
                caption: px(card::CAPTION),
            },
            pv2::code::Palette {
                foreground: t.foreground,
                muted: self.muted,
                muted_frame: t.muted_frame,
                gutter: t.gutter,
                inks: {
                    let i = t.code_inks;
                    [t.foreground, i[0], i[1], i[2], i[3], i[4], i[5]]
                },
            },
            pv2::code::Metrics {
                // Both text columns start inside the one-pixel card frame.
                inset: if numbered {
                    card::PAD + 1.
                } else {
                    card::TEXT_LEFT + 1.
                },
                top: card::TEXT_TOP - card::BAR,
                right: card::TEXT_RIGHT,
                row: card::TEXT_LINE,
                gap: card::GAP,
                state_mark: card::MARK,
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
                caption: px(card::CAPTION),
                line: pv2::table_line_pitch(card::CAPTION),
                row_padding_x: card::PAD,
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
                gap: card::GAP,
                margin: 2. * card::PAD,
                row_height: card::BAR,
            },
        )
    }
}

pub(crate) struct Mounting<'a> {
    pub path: &'a str,
    pub row: &'a RowFacts,
    pub look: &'a Look,
    pub wire: &'a Wire,
    pub catalog: &'a Arc<I18n>,
    pub backend: Option<Rc<dyn pv3::engine::Backend>>,
}

impl Mounting<'_> {
    /// The view for `kind`, its first ask already in the wire; None when media has no decoder to play through.
    pub(crate) fn mount<T>(
        &self,
        kind: Mount,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Option<Doc> {
        let (path, size, look, wire, catalog) =
            (self.path, self.row.size, self.look, self.wire, self.catalog);
        let view = match kind {
            Mount::Text | Mount::Code => View::Code(cx.new(|cx| {
                let mut view = look.code(kind == Mount::Code);
                view.set_host(Some(Box::new(wire.clone())), cx);
                view.set_catalog(catalog.clone(), cx);
                view.set_active(true, cx);
                view.set_subject(path, size, false, None, cx);
                view
            })),
            Mount::Markdown => View::Markdown(
                cx.new(|cx| look.markdown(path, Some(wire), catalog, None, window, cx)),
            ),
            Mount::Table => View::Table(cx.new(|cx| {
                let (face, palette) = look.table();
                let mut view = CsvView::new(true, face, palette);
                view.set_host(Box::new(wire.clone()));
                view.set_path(path, cx);
                view
            })),
            Mount::Html => View::Html(cx.new(|cx| {
                let (palette, _) = look.pdf();
                let code = look.code(true);
                let parts = pv2::html::SourceParts {
                    typeface: code.typeface.clone(),
                    palette: code.palette,
                    metrics: code.metrics,
                };
                let metrics = pv2::html::Metrics {
                    row_height: card::TEXT_LINE,
                    radius: 0.,
                    round_top: false,
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
                view.set_path(path, size, false, cx);
                view.set_active(true, cx);
                view
            })),
            Mount::Archive => {
                View::Archive(cx.new(|cx| archive::mount(look, wire, catalog, path, cx)))
            }
            Mount::Office => View::Office(cx.new(|cx| {
                let (palette, metrics) = look.pdf();
                let (face, table) = look.table();
                let mut view = OfficeView::new(true, palette, metrics, face, table, cx);
                view.set_host(Box::new(wire.clone()), Box::new(wire.clone()), cx);
                view.set_path(path, window, cx);
                view
            })),
            Mount::Pdf => View::Pdf(cx.new(|cx| {
                let (palette, metrics) = look.pdf();
                let mut pdf = PdfView::new(false, palette, metrics, cx);
                pdf.set_copy_host(Box::new(wire.clone()), COPY_SLOT, false);
                pdf.set_path(path, window, cx);
                pdf.set_active(true, window, cx);
                pdf
            })),
            Mount::Font => View::Font(cx.new(|cx| {
                let size_ratio = cx
                    .try_global::<crate::presentation::Fonts>()
                    .filter(|fonts| fonts.base > 0)
                    .map_or(1., |fonts| {
                        f64::from(f32::from(look.face.base)) / f64::from(fonts.base)
                    });
                let ground = card::Look::of_text(&window.text_style(), look.theme).surface;
                let theme = pv2::font::Theme {
                    foreground: hex(look.theme.foreground),
                    surface: hex(ground),
                    muted: look.muted,
                    size_ratio,
                };
                let metrics = pv2::font::Metrics {
                    font: look.face.font.clone(),
                    caption: px(card::CAPTION),
                };
                let mut view = FontView::new(theme, metrics, look.mark.clone());
                view.set_host(Some(Box::new(wire.clone())));
                view.set_catalog(catalog.clone(), cx);
                view.set_active(true, cx);
                view.set_subject(path, size, cx);
                // The font view asks again from its layout, which no notify follows; this first ask goes out with the mount.
                view.refresh(window.scale_factor());
                view
            })),
            Mount::Model => View::Model(cx.new(|cx| {
                let mut model = Model3dView::new(
                    Surface::QuickLook,
                    Rc::new(wire.clone()),
                    Arc::new(CacheReader),
                    catalog.clone(),
                    cx,
                );
                model.host.ground = look.theme.surface;
                model.show(path, 0, cx);
                model
            })),
            Mount::Video | Mount::Audio => {
                let backend = self.backend.clone().or_else(|| {
                    pv3::sandbox::Sandbox::current()
                        .ok()
                        .map(|b| Rc::new(b) as Rc<dyn pv3::engine::Backend>)
                })?;
                let video = kind == Mount::Video;
                let scale = window.scale_factor();
                let viewport = window.viewport_size();
                let subject = pv3::player::Subject {
                    path: path.into(),
                    video,
                    box_w: (f32::from(viewport.width) * scale).ceil().max(1.) as u32,
                    box_h: (f32::from(viewport.height) * scale).ceil().max(1.) as u32,
                    autoplay: true,
                };
                let player = cx.new(|cx| {
                    let mut player = MediaPlayer::new(backend, None, cx);
                    player.open(subject, cx);
                    player
                });
                View::Media(Media {
                    player,
                    known: Facts::of_row(self.row, catalog),
                    caches: Default::default(),
                    typeface: look.face.clone(),
                    video,
                })
            }
        };
        Some(Doc {
            view,
            path: path.to_owned(),
            page: 0,
        })
    }
}

/// Preview.qml binds source wrapping to the saved preview.wrap for every text document.
pub(super) fn wrap_saved(view: &View, settings: &flea::jsondoc::Json, cx: &mut gpui::App) {
    let wrap = settings
        .get("preview")
        .and_then(|preview| preview.get("wrap"))
        .and_then(flea::jsondoc::Json::as_bool)
        == Some(true);
    match view {
        View::Code(code) => code.update(cx, |code, cx| code.set_wrap(wrap, cx)),
        View::Html(html) => html.update(cx, |html, cx| html.set_wrap(wrap, cx)),
        View::Markdown(markdown) => markdown.update(cx, |markdown, cx| {
            markdown.wrap = wrap;
            cx.notify();
        }),
        _ => {}
    }
}

/// Read the live choice before a mount replaces it; a closed card starts a fresh session.
pub(super) fn remember_markdown(host: &mut super::Host, cx: &gpui::App) {
    let look = host.view.read(cx);
    if !look.is_open() {
        host.markdown_view = Default::default();
    } else if let Some(View::Markdown(view)) = look
        .subject()
        .and_then(|subject| subject.body.doc())
        .map(|doc| &doc.view)
    {
        host.markdown_view = view.read(cx).view;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // v0.3.12 CsvTable.qml: a grid row is the font's 16 px height plus 6 px above and below (28 px), the header 16 px plus
    // 8 px above and below and the 1 px rule (33 px).
    #[test]
    fn quick_look_table_rows_keep_the_v0312_pitch() {
        let look = Look {
            theme: crate::palette::Theme::default(),
            face: crate::list::Typeface::omarchy_default(),
            muted: gpui::Hsla::default(),
            mark: (gpui::SharedString::default(), &[]),
        };
        let (face, _) = look.table();
        let geometry = face.geometry(true);
        assert_eq!(geometry.row_height, 28.);
        assert_eq!(geometry.head_height, 33.);
    }
}
