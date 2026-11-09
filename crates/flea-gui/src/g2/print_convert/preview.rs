//! A bounded image composition Element; all geometry and shaping happen before paint.
use super::{host::OutputView, model::Card};
use flea::ui_logic_print_convert::{composition::Background, geometry::Composition};
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Corners, Element, ElementId, Font,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, RenderImage,
    ShapedLine, SharedString, Style, TextRun, Window,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(Default)]
pub struct Labels {
    key: Option<(Font, f32, Hsla)>,
    texts: Vec<SharedString>,
    lines: Vec<ShapedLine>,
}

pub struct Preview {
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    composition: Option<Composition>,
    images: Vec<Arc<RenderImage>>,
    digits: Vec<SharedString>,
    labels: Rc<RefCell<Labels>>,
    focused: Option<usize>,
    background: Background,
    palette: crate::g2::file_ops::palette::CardPalette,
}

impl Preview {
    pub fn new(view: &OutputView) -> Self {
        let mut out = Self {
            bounds: view.preview_bounds.clone(),
            composition: None,
            images: vec![],
            digits: vec![],
            labels: view.labels.clone(),
            focused: None,
            background: Background::Transparent,
            palette: view.palette,
        };
        let (Some(Card::Combine(c)), Some(selection)) = (&view.model.card, &view.model.selection)
        else {
            return out;
        };
        let ordered = selection
            .files
            .iter()
            .map(|f| view.thumbnails.iter().find(|t| t.path == f.path))
            .collect::<Option<Vec<_>>>();
        let Some(ordered) = ordered else {
            return out;
        };
        let sizes = ordered.iter().map(|t| t.size).collect::<Vec<_>>();
        out.composition = Composition::new(&sizes, &c.settings).ok();
        out.images = ordered.iter().map(|t| t.image.clone()).collect();
        out.digits = (1..=ordered.len())
            .map(|n| view.catalog.locale().format_count(n as i64).into())
            .collect();
        out.focused = if view.model.focus == Some(super::model::Focus::Preview) {
            c.focused
        } else {
            None
        };
        out.background = c.settings.background;
        out
    }
}

impl IntoElement for Preview {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Preview {
    type RequestLayoutState = ();
    type PrepaintState = Option<Composition>;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (
            window.request_layout(
                Style {
                    size: size(relative(1.).into(), relative(1.).into()),
                    ..Default::default()
                },
                None,
                cx,
            ),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> Option<Composition> {
        self.bounds.set(Some(bounds));
        let mut cache = self.labels.borrow_mut();
        let key = (
            window.text_style().font(),
            window.scale_factor(),
            self.palette.muted,
        );
        if cache.key.as_ref() != Some(&key) || cache.texts != self.digits {
            cache.lines = self
                .digits
                .iter()
                .map(|text| {
                    window.text_system().shape_line(
                        text.clone(),
                        px(12.),
                        &[TextRun {
                            len: text.len(),
                            font: key.0.clone(),
                            color: self.palette.muted,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    )
                })
                .collect();
            cache.texts = self.digits.clone();
            cache.key = Some(key);
        }
        self.composition.as_ref().map(|c| {
            c.fitted(
                f64::from(f32::from(bounds.size.width)),
                f64::from(f32::from(bounds.size.height)),
                14.,
            )
        })
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        geometry: &mut Option<Composition>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(geometry) = geometry else {
            return;
        };
        let labels = self.labels.borrow();
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let origin = bounds.origin
                + point(
                    px(((f32::from(bounds.size.width) - geometry.width as f32) / 2.).round()),
                    px(((f32::from(bounds.size.height) - geometry.height as f32) / 2.).round()),
                );
            // White and Black are output image content, not invented UI palette roles.
            let ink = crate::palette::output_background(self.background);
            if let Some(ink) = ink {
                window.paint_quad(fill(
                    Bounds::new(
                        origin,
                        size(px(geometry.width as f32), px(geometry.height as f32)),
                    ),
                    ink,
                ));
            }
            for (i, (r, image)) in geometry.tiles.iter().zip(&self.images).enumerate() {
                let rect = Bounds::new(
                    bounds.origin + point(px(r.x as f32), px(r.y as f32)),
                    size(px(r.width as f32), px(r.height as f32)),
                );
                let _ = window.paint_image(rect, rect, Corners::default(), image.clone(), 0, false);
                if self.focused == Some(i) {
                    window.paint_quad(gpui::quad(
                        rect.dilate(px(2.)),
                        px(0.),
                        self.palette.surface.opacity(0.),
                        px(2.),
                        self.palette.foreground,
                        gpui::BorderStyle::Solid,
                    ));
                }
                let plate =
                    Bounds::new(rect.origin + point(px(4.), px(4.)), size(px(16.), px(16.)));
                window.paint_quad(
                    fill(plate, self.palette.surface.opacity(0.86))
                        .corner_radii(px(self.palette.radius / 2.)),
                );
                if let Some(line) = labels.lines.get(i) {
                    let at = plate.origin + point((px(16.) - line.width) / 2., px(0.));
                    let _ = line.paint(at, px(16.), gpui::TextAlign::Left, None, window, cx);
                }
            }
        });
    }
}
