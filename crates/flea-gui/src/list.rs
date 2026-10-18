use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Element, ElementId, Font,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine,
    SharedString, Style, TextAlign, TextStyle, TruncateFrom, Window,
};
use std::{cell::RefCell, rc::Rc};

pub const ROW_HEIGHT: f32 = 31.;
const INSET: f32 = 14.;

pub struct Snapshot {
    pub title: SharedString,
    pub total: usize,
    pub start: usize,
    pub listing: usize,
    pub names: Vec<SharedString>,
}

#[derive(PartialEq)]
struct TextKey {
    font: Font,
    size: Pixels,
    width: Pixels,
    scale: f32,
    color: Hsla,
}

struct Entry {
    index: usize,
    text: SharedString,
    line: Rc<ShapedLine>,
    used: u64,
}

#[derive(Default)]
pub struct NameCache {
    key: Option<TextKey>,
    entries: Vec<Entry>,
    prepared: Vec<Rc<ShapedLine>>,
    clock: u64,
    line_height: Pixels,
    background: Hsla,
    #[cfg(test)]
    bounds: Bounds<Pixels>,
}

impl NameCache {
    fn line(
        &mut self,
        index: usize,
        text: &SharedString,
        limit: usize,
        shape: impl FnOnce() -> ShapedLine,
    ) -> Rc<ShapedLine> {
        self.clock += 1;
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.index == index && &entry.text == text)
        {
            entry.used = self.clock;
            return Rc::clone(&entry.line);
        }
        if self.entries.len() >= limit {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(index, _)| index)
                .unwrap();
            self.entries.swap_remove(oldest);
        }
        let line = Rc::new(shape());
        self.entries.push(Entry {
            index,
            text: text.clone(),
            line: Rc::clone(&line),
            used: self.clock,
        });
        line
    }
}

pub struct ListElement {
    snapshot: Rc<Snapshot>,
    cache: Rc<RefCell<NameCache>>,
}

impl ListElement {
    pub fn new(snapshot: Rc<Snapshot>, cache: Rc<RefCell<NameCache>>) -> Self {
        Self { snapshot, cache }
    }
}

fn shape_name(
    text: SharedString,
    width: Pixels,
    style: &TextStyle,
    font_size: Pixels,
    window: &mut Window,
    cx: &mut App,
) -> ShapedLine {
    let runs = [style.to_run(text.len())];
    let full = window
        .text_system()
        .shape_line(text.clone(), font_size, &runs, None);
    if full.width() <= width {
        return full;
    }
    let mut wrapper = cx.text_system().line_wrapper(style.font(), font_size);
    let (text, runs) = wrapper.truncate_line(text, width, "...", &runs, TruncateFrom::End);
    window
        .text_system()
        .shape_line(text, font_size, &runs, None)
}

fn row_bounds(bounds: Bounds<Pixels>, slot: usize) -> Bounds<Pixels> {
    Bounds::new(
        bounds.origin + point(px(INSET), px(slot as f32 * ROW_HEIGHT)),
        size(
            (bounds.size.width - px(INSET * 2.)).max(px(0.)),
            px(ROW_HEIGHT),
        ),
    )
}

impl IntoElement for ListElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ListElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

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
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut style = window.text_style();
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        if style.background_color.is_none() {
            style.color = theme.foreground;
        }
        let font_size = style.font_size.to_pixels(window.rem_size());
        let width = row_bounds(bounds, 0).size.width;
        let key = TextKey {
            font: style.font(),
            size: font_size,
            width,
            scale: window.scale_factor(),
            color: style.color,
        };
        let count = crate::state::visible_rows(bounds.size.height).min(self.snapshot.names.len());
        let limit = count + 1;
        let mut cache = self.cache.borrow_mut();
        #[cfg(test)]
        {
            cache.bounds = bounds;
        }
        if cache.key.as_ref() != Some(&key) {
            cache.entries.clear();
            cache.key = Some(key);
        }
        while cache.entries.len() > limit {
            cache.entries.pop();
        }
        cache.prepared.clear();
        cache.line_height = window.pixel_snap(
            style
                .line_height
                .to_pixels(font_size.into(), window.rem_size()),
        );
        cache.background = style.background_color.unwrap_or(theme.background);
        for (slot, text) in std::iter::once(&self.snapshot.title)
            .chain(self.snapshot.names.iter().take(count))
            .enumerate()
        {
            let index = if slot == 0 {
                usize::MAX
            } else {
                self.snapshot.start + slot - 1
            };
            let line = cache.line(index, text, limit, || {
                shape_name(text.clone(), width, &style, font_size, window, cx)
            });
            cache.prepared.push(line);
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let cache = self.cache.borrow();
        window.paint_quad(fill(bounds, cache.background));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (slot, line) in cache.prepared.iter().enumerate() {
                let cell = row_bounds(bounds, slot);
                let origin = cell.origin + point(px(0.), (px(ROW_HEIGHT) - cache.line_height) / 2.);
                if let Err(error) = line.paint(
                    origin,
                    cache.line_height,
                    TextAlign::Left,
                    Some(cell.size.width),
                    window,
                    cx,
                ) {
                    eprintln!("flea-gui: a name could not paint: {error}");
                }
            }
        });
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
