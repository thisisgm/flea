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
        let style = window.text_style();
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
        // Derive the background from the inherited text tokens.
        cache.background = style.background_color.unwrap_or(Hsla {
            l: 1. - style.color.l,
            ..style.color
        });
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
mod tests {
    use super::*;
    use gpui::{AvailableSpace, TestAppContext};

    #[gpui::test]
    fn list_element_lays_out_n_fixed_rows_for_n_names(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        for count in [0, 1, 7, 20] {
            let snapshot = Rc::new(Snapshot {
                title: "files".into(),
                total: count,
                start: 0,
                listing: 1,
                names: (0..count)
                    .map(|index| format!("file-{index}").into())
                    .collect(),
            });
            let cache = Rc::new(RefCell::new(NameCache::default()));
            let height = px((count + 1) as f32 * ROW_HEIGHT);
            cx.draw(
                point(px(0.), px(0.)),
                size(
                    AvailableSpace::Definite(px(640.)),
                    AvailableSpace::Definite(height),
                ),
                |_, _| ListElement::new(Rc::clone(&snapshot), Rc::clone(&cache)),
            );
            let bounds = cache.borrow().bounds;
            assert_eq!(bounds.size.height, height);
            assert_eq!(cache.borrow().prepared.len(), count + 1);
            for index in 0..count {
                let row = row_bounds(bounds, index + 1);
                assert_eq!(row.size.height, px(31.));
                assert_eq!(row.origin.y, px((index + 1) as f32 * 31.));
            }
            let first_line = Rc::clone(&cache.borrow().prepared[0]);
            cx.draw(
                point(px(0.), px(0.)),
                size(
                    AvailableSpace::Definite(px(640.)),
                    AvailableSpace::Definite(height),
                ),
                |_, _| ListElement::new(snapshot, Rc::clone(&cache)),
            );
            assert_eq!(cache.borrow().entries.len(), count + 1);
            assert!(Rc::ptr_eq(&first_line, &cache.borrow().prepared[0]));
        }
    }
}
