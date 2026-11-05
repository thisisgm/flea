//! The rename editor drawn over its row's name: the accent frame on the label column, the stem's selection wash, the muted extension, the caret and the refusal under it.
use super::{
    place::{Place, Spot},
    Shell,
};
use crate::path::{field::Line, paint::paint_run};
use gpui::{
    fill, point, px, quad, relative, size, App, BorderStyle, Bounds, ContentMask, CursorStyle,
    Element, ElementId, ElementInputHandler, Entity, GlobalElementId, GlyphRun, Hitbox,
    HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Position, Style,
    Window,
};
use std::rc::Rc;

#[path = "rename_field_pointer.rs"]
mod pointer;
#[path = "rename_field_shaping.rs"]
mod shaping;
pub(crate) use shaping::caption_line;
use shaping::shape;

// The selected run sits on a 24% accent wash with foreground text, and the caret is one pixel wide.
const SELECTION: f32 = 0.24;

/// The draft shaped at its last change, the refusal shaped once per sentence, and where the last frame drew the field.
#[derive(Default)]
pub(crate) struct Shaped {
    pub line: Line,
    error: Option<(String, Rc<GlyphRun>)>,
    pub frame: Option<Bounds<Pixels>>,
    /// The refusal's band as the last frame drew it, and the inks the frame and the refusal drew with.
    pub error_area: Option<Bounds<Pixels>>,
    pub frame_ink: Option<Hsla>,
    pub error_ink: Option<Hsla>,
    /// The ground the refusal paints under itself, where no grown row covers its band (Grid never grows).
    pub error_ground: Option<Bounds<Pixels>>,
    inset: Pixels,
    selecting: bool,
}

impl Shaped {
    /// Ends the pointer gesture while keeping the text selection for the next action.
    pub(crate) fn end_selection_drag(&mut self) {
        self.selecting = false;
    }

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
    frame_ink: Hsla,
    error: Option<Bounds<Pixels>>,
    /// The ground under the refusal, where no grown row's plate covers its band.
    ground: Option<Bounds<Pixels>>,
    /// The list's viewport: a row scrolled partly out draws and takes presses only inside it.
    clip: Bounds<Pixels>,
    hitbox: Hitbox,
    ink: crate::path::paint::Ink,
    corner: f32,
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
            let grown = shell.grows[side].band > 0.;
            let corner = shell.corner;
            let (font, pixels, caption) = {
                let list = shell.panes[side].as_ref()?.list.borrow();
                let face = &list.typeface;
                (face.font.clone(), face.body, face.caption)
            };
            let Place {
                frame,
                clip,
                inset,
                below,
            } = match shell.rename_place(side, index) {
                Spot::At(place) => place,
                // RenameField.qml: a row scrolled wholly out of its view drops the edit, with no commit; a view not yet drawn drops nothing.
                Spot::Out => {
                    cx.defer_in(window, move |shell, window, cx| {
                        shell.rename_scrolled_out(side, index, window, cx)
                    });
                    return None;
                }
                Spot::Unready => return None,
            };
            let edit = shell.rename.edit.as_mut()?;
            let line = &mut edit.line.line;
            if line.run.is_none() || line.revision != edit.entry.revision {
                line.run = Some(shape(window, &edit.entry.text, font.clone(), pixels));
                line.set_text_direction(&edit.entry.text);
                line.revision = edit.entry.revision;
            }
            // The caret stays inside the frame: the line slides under it the way a text input scrolls.
            let room = (frame.size.width - inset * 2.).max(px(0.));
            let caret = line.entry_caret(&edit.entry, edit.entry.cursor());
            if caret - line.scroll > room {
                line.scroll = caret - room;
            } else if caret < line.scroll {
                line.scroll = caret;
            }
            let width = line.run.as_ref().map_or(px(0.), |run| run.line().width);
            line.scroll = line.scroll.min((width - room).max(px(0.))).max(px(0.));
            // RenameField.qml: the refusal sits one gap under the field, at its caption line's height.
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
                let height = edit.line.error.as_ref().map_or(px(0.), |(_, run)| {
                    let line = run.line();
                    line.ascent + line.descent
                });
                Bounds::new(point(frame.origin.x, below), size(frame.size.width, height))
            });
            edit.line.frame = Some(frame);
            edit.line.inset = inset;
            let palette = crate::list::Palette::inherited(&window.text_style(), theme);
            let ink = crate::path::paint::Ink::of(palette);
            // RenameField.qml: a refused name takes the error role on its frame and on its refusal.
            let frame_ink = if error.is_some() {
                ink.error
            } else {
                ink.accent_frame
            };
            // A grown row's plate covers the band; a row that did not grow (Grid) leaves the refusal on the window ground.
            let ground = if grown { None } else { error };
            edit.line.error_area = error;
            edit.line.error_ground = ground;
            edit.line.frame_ink = Some(frame_ink);
            edit.line.error_ink = Some(ink.error);
            Some(Laid {
                frame,
                frame_ink,
                error,
                ground,
                clip,
                hitbox: window.insert_hitbox(frame.intersect(&clip), HitboxBehavior::Normal),
                ink,
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
        let (ink, frame, frame_ink, clip) = (laid.ink, laid.frame, laid.frame_ink, laid.clip);
        window.set_cursor_style(CursorStyle::IBeam, &laid.hitbox);
        let shell = self.shell.read(cx);
        let Some(edit) = shell.rename.edit.as_ref() else {
            return;
        };
        let entity = self.shell.clone();
        let focus = shell.focus.clone();
        let active = !shell.operations.archive.showing()
            && shell.rename.bulk.is_none()
            && shell.menu.text.is_none()
            && focus.is_focused(window);
        window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
            // The small field follows half of the compositor's corner radius.
            window.paint_quad(quad(
                frame,
                px(laid.corner / 2.),
                ink.background,
                px(1.),
                frame_ink,
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
            // A file name keeps its order in a mirrored window: line, washes and caret move as one.
            window.with_unmirrored(frame, |window| {
                window.with_content_mask(Some(ContentMask { bounds: band }), |window| {
                    let washed = entry.washed();
                    if let Some(selected) = &washed {
                        let (from, to) = line.entry_span(entry, selected);
                        let (from, to) = (origin + from, origin + to);
                        window.paint_quad(fill(
                            Bounds::new(
                                point(from, band.origin.y),
                                size(to - from, band.size.height),
                            ),
                            ink.wash(SELECTION),
                        ));
                    }
                    if let Some(run) = &line.run {
                        let layout = run.line();
                        let baseline = frame.origin.y
                            + (frame.size.height - layout.ascent - layout.descent) / 2.
                            + layout.ascent;
                        let scale = window.scale_factor();
                        let baseline = px((f32::from(baseline) * scale).floor() / scale);
                        let ends = |from: Pixels, to: Pixels| {
                            Bounds::from_corners(point(from, band.top()), point(to, band.bottom()))
                        };
                        for (part, color) in [
                            (ends(band.left(), boundary.max(band.left())), ink.foreground),
                            (ends(boundary.min(band.right()), band.right()), ink.muted),
                        ] {
                            window.with_content_mask(
                                Some(ContentMask { bounds: part }),
                                |window| {
                                    let _ =
                                        window.paint_glyph_run(point(origin, baseline), run, color);
                                },
                            );
                        }
                    }
                    if let Some(marked) = &entry.marked {
                        let (from, to) = line.entry_span(entry, marked);
                        let (from, to) = (origin + from, origin + to);
                        window.paint_quad(fill(
                            Bounds::new(
                                point(from, band.bottom() - px(1.)),
                                size(to - from, px(1.)),
                            ),
                            ink.foreground,
                        ));
                    }
                    if washed.is_none() {
                        let at = origin + line.entry_caret(entry, entry.cursor());
                        window.paint_quad(fill(
                            Bounds::new(
                                point(at, band.origin.y + px(2.)),
                                size(px(1.), band.size.height - px(4.)),
                            ),
                            ink.foreground,
                        ));
                    }
                });
                // Registered in the field's own scope, so a candidate box that clears the frame still moves with it.
                if active {
                    window.handle_input(
                        &focus,
                        ElementInputHandler::new(frame, entity.clone()),
                        cx,
                    );
                }
            });
            // The refusal sits in the band under its field, in the error role, on the ground its row or the window keeps.
            if let (Some(area), Some((_, run))) = (laid.error, &shaped.error) {
                if let Some(ground) = laid.ground {
                    window.paint_quad(fill(ground, ink.background));
                }
                paint_run(
                    run,
                    frame.origin.x,
                    area.origin.y,
                    area.size.height,
                    ink.error,
                    window,
                );
            }
        });
        entity.update(cx, |shell, _| {
            if let Some(edit) = &mut shell.rename.edit {
                edit.line.line.registered = if active { edit.entry.session() } else { None };
            }
        });
        pointer::register(entity, frame, clip, window);
    }
}

/// Columns and Grid draw the rename field over their own element, as the list does over its rows.
pub(crate) fn with_field(
    pane: gpui::AnyElement,
    renaming: Option<usize>,
    side: usize,
    cx: &mut gpui::Context<Shell>,
) -> gpui::AnyElement {
    use gpui::prelude::*;
    if renaming.is_none() {
        return pane;
    }
    gpui::div()
        .relative()
        .size_full()
        .child(pane)
        .child(FieldElement::new(cx.entity(), side))
        .into_any_element()
}
