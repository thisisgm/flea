//! Paint-only overlay: no hitbox, input cancellation or row-sized layout children.
use super::{Candidate, Owner, Shaped, REST_MS};
use gpui::{
    canvas, fill, outline, point, px, size, App, BorderStyle, Bounds, Canvas, ContentMask,
    EntityId, GlyphRun, IntoElement, Styled, TextRun, Window,
};
use std::{rc::Rc, time::Duration};

pub(super) type Source = Rc<dyn Fn(&Window, &App) -> Option<Candidate>>;

pub(crate) struct Overlay {
    owner: Owner,
    source: Source,
    entity: EntityId,
}

impl Overlay {
    pub(crate) fn new(
        owner: Owner,
        entity: EntityId,
        source: impl Fn(&Window, &App) -> Option<Candidate> + 'static,
    ) -> Self {
        Self {
            owner,
            source: Rc::new(source),
            entity,
        }
    }
}

impl IntoElement for Overlay {
    type Element = Canvas<Owner>;
    fn into_element(self) -> Self::Element {
        let Self {
            owner,
            source,
            entity,
        } = self;
        let prepaint_source = source.clone();
        canvas(
            move |_, window, cx| prepaint(&owner, &prepaint_source, entity, window, cx),
            move |_, owner, window, cx| paint(owner, source, entity, window, cx),
        )
        .absolute()
        .size_full()
    }
}

fn prepare(candidate: &Candidate, window: &Window) -> Shaped {
    let shape = |label: &str| {
        GlyphRun::new(window.text_system().layout_line(
            label,
            candidate.size,
            &[TextRun {
                len: label.len(),
                font: candidate.font.clone(),
                color: candidate.ink,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        ))
    };
    let full = shape(&candidate.name);
    let room = (candidate.row.size.width - candidate.pad - px(1.)).max(px(0.));
    if full.line().width <= room {
        return Shaped { run: full };
    }
    let width = |label: &str| f32::from(shape(label).line().width);
    let total = crate::row::cell_count(&candidate.name);
    let advance = f32::from(full.line().width) / total.max(1) as f32;
    let budget = crate::row::fit_budget(
        (f32::from(room) / advance.max(1.)).floor() as usize,
        total,
        |cells| width(&crate::row::middle_elide(&candidate.name, cells)) <= f32::from(room),
    );
    let label = crate::row::middle_elide(&candidate.name, budget);
    let run = shape(&label);
    Shaped { run }
}

fn prepaint(
    owner: &Owner,
    source: &Source,
    entity: EntityId,
    window: &mut Window,
    cx: &mut App,
) -> Owner {
    let owner = owner.clone();
    let mut state = owner.borrow_mut();
    let candidate = if state.pointer.is_some()
        && !state.suppressed
        && !state.pressed
        && window.is_window_active()
        && !cx.has_active_drag()
    {
        source(window, cx)
    } else {
        None
    };
    let arm = state.reconcile(candidate);
    if arm {
        let weak = Rc::downgrade(&owner);
        let generation = state.generation;
        state.timer = Some(cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(REST_MS))
                .await;
            cx.update(|cx| {
                if let Some(owner) = weak.upgrade() {
                    let mut owner = owner.borrow_mut();
                    if owner.generation == generation && owner.candidate.is_some() {
                        owner.ready = true;
                        cx.notify(entity);
                    }
                }
            });
        }));
    }
    if state.ready && state.shaped.is_none() {
        if let Some(candidate) = state.candidate.as_ref() {
            state.shaped = Some(prepare(candidate, window));
            state.names_shaped += 1;
            state.tips_built += 1;
        }
    }
    drop(state);
    #[cfg(test)]
    if let Some(probe) = cx.try_global::<super::tests::Probe>() {
        *probe.0.borrow_mut() = Rc::downgrade(&owner);
    }
    owner
}

fn paint(owner: Owner, source: Source, entity: EntityId, window: &mut Window, cx: &mut App) {
    super::events::listen(&owner, source, entity, window);
    let state = owner.borrow();
    let (Some(candidate), Some(shaped)) = (&state.candidate, &state.shaped) else {
        return;
    };
    let y = candidate.row.top() + (candidate.row.size.height - candidate.height) / 2.;
    let left = candidate.text_x - candidate.pad - px(1.);
    let plate = Bounds::from_corners(
        point(left, y),
        point(candidate.row.right(), y + candidate.height),
    );
    let theme = cx
        .try_global::<crate::palette::Theme>()
        .copied()
        .unwrap_or_default();
    let ground = if candidate.framed {
        theme.surface
    } else {
        candidate.ground
    };
    let mask = Bounds::new(
        point(left, candidate.clip.top()),
        size(plate.size.width, candidate.clip.size.height),
    );
    window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
        let radius = super::plate_radius(crate::compositor::corner(cx));
        let mut quad = if candidate.framed {
            let mut quad = outline(plate, candidate.hair, BorderStyle::Solid);
            quad.background = ground.into();
            quad
        } else {
            fill(plate, ground)
        };
        quad.corner_radii = radius.into();
        window.paint_quad(quad);
        let line = shaped.run.line();
        let baseline = candidate.row.top()
            + (candidate.row.size.height - line.ascent - line.descent) / 2.
            + line.ascent;
        let scale = window.scale_factor();
        let baseline = px((f32::from(baseline) * scale).floor() / scale);
        let _ = window.paint_glyph_run(
            point(candidate.text_x, baseline),
            &shaped.run,
            candidate.ink,
        );
    });
}
