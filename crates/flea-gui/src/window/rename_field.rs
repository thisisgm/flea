//! The rename editor drawn over its row's name: the accent frame on the label column, the stem's selection wash, the muted extension, the caret and the refusal under it.
use super::Shell;
use crate::path::{field::Line, paint::paint_run};
use gpui::{
    fill, point, px, quad, relative, size, App, BorderStyle, Bounds, ContentMask, CursorStyle,
    DispatchPhase, Element, ElementId, ElementInputHandler, Entity, GlobalElementId, GlyphRun,
    Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    Pixels, Position, Style, TextRun, Window,
};
use std::rc::Rc;

// The selected run sits on a 24% accent wash with foreground text, and the caret is one pixel wide.
const SELECTION: f32 = 0.24;

/// The draft shaped at its last change, the refusal shaped once per sentence, and where the last frame drew the field.
#[derive(Default)]
pub(crate) struct Shaped {
    pub line: Line,
    error: Option<(String, Rc<GlyphRun>)>,
    pub frame: Option<Bounds<Pixels>>,
    inset: Pixels,
}

impl Shaped {
    pub(super) fn origin(&self) -> Pixels {
        self.frame
            .map_or(px(0.), |frame| frame.origin.x + self.inset)
            - self.line.scroll
    }
}

pub(crate) struct FieldElement {
    shell: Entity<Shell>,
    side: usize,
}

impl FieldElement {
    pub(crate) fn new(shell: Entity<Shell>, side: usize) -> Self {
        Self { shell, side }
    }
}

impl IntoElement for FieldElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

/// The frame, its text band and the refusal's line, in window coordinates.
pub(crate) struct Laid {
    frame: Bounds<Pixels>,
    error: Option<Bounds<Pixels>>,
    /// The list's viewport: a row scrolled partly out draws and takes presses only inside it.
    clip: Bounds<Pixels>,
    hitbox: Hitbox,
    ink: crate::path::paint::Ink,
    corner: f32,
}

fn shape(window: &mut Window, text: &str, font: gpui::Font, pixels: Pixels) -> Rc<GlyphRun> {
    let run = TextRun {
        len: text.len(),
        font,
        color: Default::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    Rc::new(GlyphRun::new(window.text_system().layout_line(
        text,
        pixels,
        &[run],
        None,
    )))
}

impl Element for FieldElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<Laid>;

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
            position: Position::Absolute,
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Laid> {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let side = self.side;
        self.shell.update(cx, |shell, cx| {
            let index = shell.renaming_row(side)?;
            let corner = shell.corner;
            let host = shell.panes[side].as_ref()?;
            let list = host.list.borrow();
            let drawn = &list.drawn;
            let metrics = drawn.metrics;
            let font = list.typeface.font.clone();
            let pixels = list.typeface.body;
            let caption = list.typeface.caption;
            let top = drawn.bounds.origin.y + px(index as f32 * metrics.row - drawn.scroll_y);
            let frame = Bounds::new(
                point(
                    drawn.bounds.origin.x + px(drawn.layout.name.x),
                    top + px((metrics.row - metrics.slot) / 2.),
                ),
                size(px(drawn.layout.name.width.max(0.)), px(metrics.slot)),
            );
            let inset = px(metrics.gap);
            let clip = drawn.bounds;
            drop(list);
            // RenameField.qml: a row scrolled wholly out of the list drops the edit, with no commit; a list not yet drawn drops nothing.
            if clip.size.height <= px(0.) {
                return None;
            }
            if frame.bottom() <= clip.top() || frame.top() >= clip.bottom() {
                cx.defer_in(window, move |shell, window, cx| {
                    shell.rename_scrolled_out(side, index, window, cx)
                });
                return None;
            }
            let edit = shell.rename.edit.as_mut()?;
            let line = &mut edit.line.line;
            if line.run.is_none() || line.revision != edit.entry.revision {
                line.run = Some(shape(window, &edit.entry.text, font.clone(), pixels));
                line.revision = edit.entry.revision;
            }
            // The caret stays inside the frame: the line slides under it the way a text input scrolls.
            let room = (frame.size.width - inset * 2.).max(px(0.));
            let caret = line.x_for(edit.entry.cursor());
            if caret - line.scroll > room {
                line.scroll = caret - room;
            } else if caret < line.scroll {
                line.scroll = caret;
            }
            let width = line.run.as_ref().map_or(px(0.), |run| run.line().width);
            line.scroll = line.scroll.min((width - room).max(px(0.))).max(px(0.));
            let error = (!edit.error.is_empty()).then(|| {
                if edit
                    .line
                    .error
                    .as_ref()
                    .is_none_or(|(text, _)| *text != edit.error)
                {
                    let run = shape(window, &edit.error, font, caption);
                    edit.line.error = Some((edit.error.clone(), run));
                }
                Bounds::new(
                    point(frame.origin.x, top + px(metrics.row)),
                    size(frame.size.width, px(metrics.row)),
                )
            });
            edit.line.frame = Some(frame);
            edit.line.inset = inset;
            let palette = crate::list::Palette::inherited(&window.text_style(), theme);
            Some(Laid {
                frame,
                error,
                clip,
                hitbox: window.insert_hitbox(frame.intersect(&clip), HitboxBehavior::Normal),
                ink: crate::path::paint::Ink::of(palette),
                corner,
            })
        })
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        laid: &mut Option<Laid>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(laid) = laid else {
            return;
        };
        let (ink, frame, clip) = (laid.ink, laid.frame, laid.clip);
        window.set_cursor_style(CursorStyle::IBeam, &laid.hitbox);
        let shell = self.shell.read(cx);
        let Some(edit) = shell.rename.edit.as_ref() else {
            return;
        };
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
            // The small field follows half of the compositor's corner radius.
            window.paint_quad(quad(
                frame,
                px(laid.corner / 2.),
                ink.background,
                px(1.),
                ink.accent_frame,
                BorderStyle::Solid,
            ));
            let shaped = &edit.line;
            let line = &shaped.line;
            let origin = shaped.origin();
            let band = Bounds::new(
                point(frame.origin.x + px(1.), frame.origin.y + px(1.)),
                size(
                    (frame.size.width - px(2.)).max(px(0.)),
                    (frame.size.height - px(2.)).max(px(0.)),
                ),
            );
            let entry = &edit.entry;
            let text = &entry.text;
            // The extension starts at the last dot past the first character, read off the draft as it stands.
            let stem = text.rfind('.').filter(|&at| at > 0).unwrap_or(text.len());
            let boundary = origin + line.x_for(stem);
            window.with_content_mask(Some(ContentMask { bounds: band }), |window| {
                if !entry.selected.is_empty() {
                    let from = origin + line.x_for(entry.selected.start);
                    let to = origin + line.x_for(entry.selected.end);
                    window.paint_quad(fill(
                        Bounds::new(
                            point(from, band.origin.y),
                            size(to - from, band.size.height),
                        ),
                        ink.wash(SELECTION),
                    ));
                }
                if let Some(run) = &line.run {
                    let ends = |from: Pixels, to: Pixels| {
                        Bounds::from_corners(point(from, band.top()), point(to, band.bottom()))
                    };
                    for (part, color) in [
                        (ends(band.left(), boundary.max(band.left())), ink.foreground),
                        (ends(boundary.min(band.right()), band.right()), ink.muted),
                    ] {
                        window.with_content_mask(Some(ContentMask { bounds: part }), |window| {
                            paint_run(
                                run,
                                origin,
                                frame.origin.y,
                                frame.size.height,
                                color,
                                window,
                            );
                        });
                    }
                }
                if let Some(marked) = &entry.marked {
                    let from = origin + line.x_for(marked.start);
                    let to = origin + line.x_for(marked.end);
                    window.paint_quad(fill(
                        Bounds::new(point(from, band.bottom() - px(1.)), size(to - from, px(1.))),
                        ink.foreground,
                    ));
                }
                if entry.selected.is_empty() {
                    let at = origin + line.x_for(entry.cursor());
                    window.paint_quad(fill(
                        Bounds::new(
                            point(at, band.origin.y + px(2.)),
                            size(px(1.), band.size.height - px(4.)),
                        ),
                        ink.foreground,
                    ));
                }
            });
            // The refusal takes the band under the row, on the window ground, in the error role.
            if let (Some(area), Some((_, run))) = (laid.error, &shaped.error) {
                window.paint_quad(fill(area, ink.background));
                paint_run(
                    run,
                    frame.origin.x,
                    area.origin.y,
                    area.size.height,
                    ink.accent,
                    window,
                );
            }
        });
        let entity = self.shell.clone();
        let focus = shell.focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(frame, entity.clone()), cx);
        // Tap.js: a press anywhere else commits the open rename first and then does its own work.
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                return;
            }
            entity.update(cx, |shell, cx| {
                let Some(edit) = shell.rename.edit.as_mut().filter(|edit| edit.entry.editing)
                else {
                    return;
                };
                if !frame.intersect(&clip).contains(&event.position) {
                    shell.rename_commit(cx);
                    shell.rename_sync_gate(window, cx);
                    return;
                }
                let at = edit
                    .line
                    .line
                    .index_for(event.position.x - edit.line.origin());
                match event.click_count {
                    1 if event.modifiers.shift => edit.entry.select_to(at),
                    1 => edit.entry.move_to(at),
                    _ => edit.entry.select_all(),
                }
                cx.stop_propagation();
                cx.notify();
            });
        });
    }
}
