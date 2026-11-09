//! The chooser's existing filename and query slots share one shaped input leaf.
use crate::list::{Palette, Typeface};
use crate::path::{entry::Entry, field::Line};
use gpui::{
    fill, point, px, relative, size, AnyElement, App, Bounds, ContentMask, Context, DispatchPhase,
    Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle,
    GlobalElementId, GlyphRun, InspectorElementId, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, Pixels, Style, Window,
};
use std::{cell::RefCell, rc::Rc};

// The selection wash over the shaped leaf, and the band heights of the plain field and the filter field.
const WASH: f32 = 0.24;
const FIELD_BAND: f32 = 20.;
const FILTER_BAND: f32 = 18.;

#[derive(Default)]
pub(super) struct Drawn {
    pub line: Line,
    pub bounds: Option<Bounds<Pixels>>,
    font: Option<gpui::Font>,
    pixels: Pixels,
    generation: u64,
}

pub(super) trait EntryHost: EntityInputHandler + Sized + 'static {
    fn entry(&self) -> &Entry;
    fn entry_mut(&mut self) -> &mut Entry;
    fn drawn(&self) -> &RefCell<Drawn>;
    fn focus(&self) -> &FocusHandle;
    fn active(&self, window: &Window, cx: &App) -> bool;
    fn changed(&mut self, cx: &mut Context<Self>);

    fn accepts(&self, window: &Window, cx: &App) -> bool {
        self.active(window, cx) && self.drawn().borrow().line.accepts(self.entry())
    }
}

pub(super) struct Input<T: EntryHost> {
    host: Entity<T>,
    face: Typeface,
    strip: Option<Rc<RefCell<crate::g2::listing_filter::strip::StripState>>>,
}

impl<T: EntryHost> Input<T> {
    pub fn new(host: Entity<T>, face: Typeface) -> Self {
        Self {
            host,
            face,
            strip: None,
        }
    }

    pub fn strip(
        mut self,
        state: Rc<RefCell<crate::g2::listing_filter::strip::StripState>>,
    ) -> Self {
        self.strip = Some(state);
        self
    }
}

impl<T: EntryHost> IntoElement for Input<T> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl<T: EntryHost> Element for Input<T> {
    type RequestLayoutState = Option<AnyElement>;
    type PrepaintState = Palette;

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
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut child = self.strip.as_ref().map(|state| {
            crate::g2::listing_filter::strip::StripElement::new(state.clone()).into_any_element()
        });
        let children = child.as_mut().map(|child| child.request_layout(window, cx));
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, children, cx), child)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Palette {
        if let Some(child) = child {
            child.prepaint(window, cx);
        }
        let slot = self
            .strip
            .as_ref()
            .and_then(|state| state.borrow().laid)
            .map_or(bounds, |laid| {
                let right = laid
                    .summary
                    .map_or(laid.way_out.left(), |summary| summary.left())
                    - px(9.);
                Bounds::new(
                    laid.query.origin,
                    size(
                        (right - laid.query.left()).max(px(0.)),
                        laid.query.size.height,
                    ),
                )
            });
        let host = self.host.read(cx);
        let entry = host.entry();
        let mut drawn = host.drawn().borrow_mut();
        drawn.bounds = Some(slot);
        if drawn.line.run.is_none()
            || drawn.line.revision != entry.revision
            || drawn.generation != entry.generation
            || drawn.font.as_ref() != Some(&self.face.font)
            || drawn.pixels != self.face.body
        {
            let mut run = window.text_style().to_run(entry.text.len());
            run.font = self.face.font.clone();
            drawn.line.run = Some(Rc::new(GlyphRun::new(window.text_system().layout_line(
                &entry.text,
                self.face.body,
                &[run],
                None,
            ))));
            drawn.font = Some(self.face.font.clone());
            drawn.pixels = self.face.body;
            drawn.generation = entry.generation;
            drawn.line.revision = entry.revision;
            drawn.line.right_to_left = crate::path::entry::right_to_left(&entry.text);
        }
        let line = &mut drawn.line;
        let caret = line.entry_caret(entry, entry.cursor());
        let room = (slot.size.width - px(3.)).max(px(0.));
        if caret - line.scroll > room {
            line.scroll = caret - room;
        } else if caret < line.scroll {
            line.scroll = caret;
        }
        let width = line.run.as_ref().map_or(px(0.), |run| run.line().width);
        line.scroll = line.scroll.min((width - room).max(px(0.))).max(px(0.));
        Palette::inherited(
            &window.text_style(),
            cx.try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default(),
        )
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut Self::RequestLayoutState,
        palette: &mut Palette,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(child) = child {
            child.paint(window, cx);
        }
        let host = self.host.read(cx);
        let active = host.active(window, cx);
        let entry = host.entry();
        let mut drawn = host.drawn().borrow_mut();
        let Some(bounds) = drawn.bounds else { return };
        let line = &drawn.line;
        let origin = bounds.left() - line.scroll;
        let filter = self.strip.is_some();
        let band = if filter { FILTER_BAND } else { FIELD_BAND };
        let top = bounds.top() + (bounds.size.height - px(band)) / 2.;
        // An editable line keeps its text, selection and caret together in a mirrored window.
        window.with_unmirrored(bounds, |window| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                if filter {
                    window.paint_quad(fill(bounds, palette.background));
                }
                if active && !entry.selected.is_empty() {
                    let (from, to) = line.entry_span(entry, &entry.selected);
                    window.paint_quad(fill(
                        Bounds::new(point(origin + from, top), size(to - from, px(band))),
                        palette.accent.opacity(WASH),
                    ));
                }
                if let Some(run) = &line.run {
                    crate::path::paint::paint_run(
                        run,
                        origin,
                        bounds.top(),
                        bounds.size.height,
                        palette.foreground,
                        window,
                    );
                }
                if active {
                    if let Some(marked) = &entry.marked {
                        let (from, to) = line.entry_span(entry, marked);
                        window.paint_quad(fill(
                            Bounds::new(
                                point(origin + from, top + px(band - 1.)),
                                size(to - from, px(1.)),
                            ),
                            palette.foreground,
                        ));
                    }
                    if entry.selected.is_empty() {
                        let height = if filter { 13. } else { 16. };
                        let caret = Bounds::new(
                            point(
                                origin + line.entry_caret(entry, entry.cursor()) + px(1.),
                                bounds.top() + (bounds.size.height - px(height)) / 2.,
                            ),
                            size(px(if filter { 2. } else { 1. }), px(height)),
                        );
                        window.paint_quad(fill(
                            caret,
                            if filter {
                                palette.accent
                            } else {
                                palette.foreground
                            },
                        ));
                    }
                }
            })
        });
        let focus = host.focus().clone();
        drawn.line.registered = active.then(|| entry.session()).flatten();
        drop(drawn);
        if active {
            window.handle_input(
                &focus,
                ElementInputHandler::new(bounds, self.host.clone()),
                cx,
            );
            // A double-click selects the word under the pointer and a third click the whole line, as every text field does.
            let host = self.host.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                if phase != DispatchPhase::Bubble
                    || event.button != MouseButton::Left
                    || event.click_count < 2
                    || !bounds.contains(&event.position)
                {
                    return;
                }
                host.update(cx, |host, cx| {
                    let at = {
                        let drawn = host.drawn().borrow();
                        let line = &drawn.line;
                        host.entry()
                            .word_index(line, event.position.x - bounds.left() + line.scroll)
                    };
                    let entry = host.entry_mut();
                    if event.click_count == 2 {
                        entry.select_word(at);
                    } else {
                        entry.select_all();
                    }
                    cx.notify();
                });
            });
        }
    }
}
