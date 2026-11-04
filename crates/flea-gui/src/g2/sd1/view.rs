//! The ShareBrowser overlay as one GPUI Element over the listing slot, as ui/ShareBrowser.qml draws it.
use crate::g2::mn1::glyphs;
use crate::list::{Palette, Typeface};
use flea::ui_logic_mn1::fit::{GAP, PAD_X, ROW, SLOT};
use flea::ui_logic_sd1::share::ShareBrowser;
use gpui::{
    fill, point, px, relative, size, App, Bounds, ContentMask, Context, DispatchPhase, Element,
    ElementId, GlobalElementId, GlyphRun, HitboxBehavior, Hsla, InspectorElementId, IntoElement,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Render,
    ScrollWheelEvent, Style, TextRun, TransformationMatrix, Window,
};
use std::{cell::RefCell, rc::Rc};

// The cursor wash, the 2px accent edge and the hover lift, as design-hig/source/ui.py draws the rung (rule 7).
const CURSOR_WASH: f32 = 0.18;
const HOVER: f32 = 0.08;
const EDGE_W: f32 = 2.;
const ROW_H: f32 = ROW as f32;

/// What the overlay drew last frame, read by the tests and by the IPC read-back.
#[derive(Default)]
pub struct Probe {
    // None while the overlay is shut, as ui/ShareBrowser.qml is invisible then.
    pub bounds: Option<Bounds<Pixels>>,
    pub painted: usize,
    pub picked: Option<usize>,
    // The row under the pointer, set by the overlay's own pointer handling.
    pub hovered: Option<usize>,
    // The row a left press started on, picked only by a release over the same row.
    pub pressed: Option<usize>,
    // The share a click picked, published for the host to open; drained by ProviderSend.
    pub pick: Option<(String, String)>,
}

/// The share names shaped once. One entry, replaced whole when the list changes: a list is one bounded server answer.
#[derive(Default)]
pub struct ShapedShares {
    names: Vec<String>,
    runs: Vec<GlyphRun>,
    // How many times the names were shaped, so a still frame is proven to shape nothing.
    pub reshapes: usize,
}

impl ShapedShares {
    fn sync(&mut self, names: &[String], typeface: &Typeface, window: &Window) {
        if self.names.as_slice() == names {
            return;
        }
        self.names = names.to_vec();
        self.runs = names
            .iter()
            .map(|name| shape(name, typeface, window))
            .collect();
        self.reshapes += 1;
    }

    pub fn len(&self) -> usize {
        self.runs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

fn shape(text: &str, typeface: &Typeface, window: &Window) -> GlyphRun {
    let run = TextRun {
        len: text.len(),
        font: typeface.font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    GlyphRun::new(
        window
            .text_system()
            .layout_line(text, typeface.body, &[run], None),
    )
}

/// The row under a pointer height, counted from the overlay's top; None above it or past the last share.
pub fn row_at(bounds: Bounds<Pixels>, y: Pixels, count: usize) -> Option<usize> {
    let offset = f32::from(y - bounds.origin.y);
    if offset < 0. {
        return None;
    }
    let index = (offset / ROW_H) as usize;
    (index < count).then_some(index)
}

pub struct ShareBrowserElement {
    browser: Rc<RefCell<ShareBrowser>>,
    rows: Rc<RefCell<ShapedShares>>,
    probe: Rc<RefCell<Probe>>,
    typeface: Typeface,
    palette: Palette,
}

impl ShareBrowserElement {
    pub fn new(
        browser: Rc<RefCell<ShareBrowser>>,
        rows: Rc<RefCell<ShapedShares>>,
        probe: Rc<RefCell<Probe>>,
        typeface: Typeface,
        palette: Palette,
    ) -> Self {
        Self {
            browser,
            rows,
            probe,
            typeface,
            palette,
        }
    }
}

impl IntoElement for ShareBrowserElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn paint_text(run: &GlyphRun, x: Pixels, top: Pixels, color: Hsla, window: &mut Window) {
    let line = run.line();
    let baseline =
        window.pixel_snap(top + (px(ROW_H) - line.ascent - line.descent) / 2. + line.ascent);
    let _ = window.paint_glyph_run(point(x, baseline), run, color);
}

impl Element for ShareBrowserElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<gpui::Hitbox>;

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
        _: &mut App,
    ) -> Option<gpui::Hitbox> {
        let browser = self.browser.borrow();
        self.rows
            .borrow_mut()
            .sync(&browser.shares, &self.typeface, window);
        // An open overlay is the topmost surface: this hitbox sinks every press and the wheel.
        browser
            .active
            .then(|| window.insert_hitbox(bounds, HitboxBehavior::BlockMouse))
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        hitbox: &mut Option<gpui::Hitbox>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let browser = self.browser.borrow();
        let rows = self.rows.borrow();
        let mut probe = self.probe.borrow_mut();
        if !browser.active {
            *probe = Probe::default();
            return;
        }
        let palette = self.palette;
        let hovered = probe.hovered.filter(|index| *index < rows.len());
        let picked = browser.cursor;
        let (left, width) = (bounds.origin.x, bounds.size.width);
        window.paint_quad(fill(bounds, palette.background));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for (index, run) in rows.runs.iter().enumerate() {
                let top = bounds.origin.y + px(index as f32 * ROW_H);
                let row = Bounds::new(point(left, top), size(width, px(ROW_H)));
                let lifted = index == picked;
                if lifted {
                    window.paint_quad(fill(row, palette.accent.opacity(CURSOR_WASH)));
                    let edge = Bounds::new(point(left, top), size(px(EDGE_W), px(ROW_H)));
                    window.paint_quad(fill(edge, palette.accent_frame));
                } else if hovered == Some(index) {
                    window.paint_quad(fill(row, palette.foreground.opacity(HOVER)));
                }
                let ink = if lifted || hovered == Some(index) {
                    palette.foreground
                } else {
                    palette.metadata_frame
                };
                let mark = Bounds::new(
                    point(left + px(PAD_X), top + px((ROW_H - SLOT) / 2.)),
                    size(px(SLOT), px(SLOT)),
                );
                let (key, data) = glyphs::mark("server");
                let _ =
                    window.paint_svg(mark, key, Some(data), TransformationMatrix::unit(), ink, cx);
                paint_text(
                    run,
                    left + px(PAD_X + SLOT + GAP),
                    top,
                    palette.foreground,
                    window,
                );
            }
        });
        probe.bounds = Some(bounds);
        probe.painted = rows.len();
        probe.picked = (picked < rows.len()).then_some(picked);
        probe.hovered = hovered;
        drop(probe);
        drop(rows);
        drop(browser);
        // ui/ShareBrowser.qml sinks clicks and the wheel over the overlay; the prepaint hitbox makes this topmost.
        let Some(hitbox) = hitbox.take() else {
            return;
        };
        let move_box = hitbox.clone();
        let move_browser = Rc::clone(&self.browser);
        let move_probe = Rc::clone(&self.probe);
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            // The row under the pointer lifts; a pointer outside the overlay (or in its gap) lifts none.
            let row = if move_box.is_hovered(window) {
                row_at(bounds, event.position.y, move_browser.borrow().shares.len())
            } else {
                None
            };
            let mut probe = move_probe.borrow_mut();
            if probe.hovered != row {
                probe.hovered = row;
                window.refresh();
            }
        });
        let down_box = hitbox.clone();
        let down_browser = Rc::clone(&self.browser);
        let down_probe = Rc::clone(&self.probe);
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !down_box.is_hovered(window) {
                return;
            }
            cx.stop_propagation();
            if event.button != MouseButton::Left {
                return;
            }
            // A press only records its row, as ui/MenuRow.qml's ReleaseWithinBounds tap does.
            let row = row_at(bounds, event.position.y, down_browser.borrow().shares.len());
            let mut probe = down_probe.borrow_mut();
            probe.pressed = row;
            probe.hovered = row;
            window.refresh();
        });
        let up_box = hitbox.clone();
        let up_browser = Rc::clone(&self.browser);
        let up_probe = Rc::clone(&self.probe);
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            // Any left release ends the press, so a press dragged off is never matched later (ReleaseWithinBounds).
            let pressed = if event.button == MouseButton::Left {
                up_probe.borrow_mut().pressed.take()
            } else {
                None
            };
            if !up_box.is_hovered(window) {
                return;
            }
            cx.stop_propagation();
            // ReleaseWithinBounds: only a release over the pressed row picks, through the core rule browser_click uses.
            let row = row_at(bounds, event.position.y, up_browser.borrow().shares.len());
            up_probe.borrow_mut().hovered = row;
            if pressed.is_some() && pressed == row {
                let pick = up_browser.borrow_mut().pick(row);
                if let Some(pick) = pick {
                    up_probe.borrow_mut().pick = Some(pick);
                }
            }
            window.refresh();
        });
        let wheel_box = hitbox;
        window.on_mouse_event(move |_: &ScrollWheelEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && wheel_box.clone().should_handle_scroll(window) {
                cx.stop_propagation();
            }
        });
    }
}

/// The mountable overlay view for one `Slots.states` side; its pick flows through `Probe.pick` for the host drain.
pub struct ShareBrowserView {
    browser: Rc<RefCell<ShareBrowser>>,
    rows: Rc<RefCell<ShapedShares>>,
    probe: Rc<RefCell<Probe>>,
    typeface: Typeface,
    palette: Palette,
}

impl ShareBrowserView {
    pub fn new(
        browser: Rc<RefCell<ShareBrowser>>,
        rows: Rc<RefCell<ShapedShares>>,
        probe: Rc<RefCell<Probe>>,
        typeface: Typeface,
        palette: Palette,
    ) -> Self {
        Self {
            browser,
            rows,
            probe,
            typeface,
            palette,
        }
    }
}

impl Render for ShareBrowserView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ShareBrowserElement::new(
            Rc::clone(&self.browser),
            Rc::clone(&self.rows),
            Rc::clone(&self.probe),
            self.typeface.clone(),
            self.palette,
        )
    }
}
