//! Crumb Element rendering; shaping and palette ownership stay in paint.
use super::*;

impl Element for CrumbElement {
    type RequestLayoutState = ();
    type PrepaintState = (Hitbox, Ink, bool);

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
    ) -> (Hitbox, Ink, bool) {
        let theme = crate::window::glass::producers::chrome_theme(window, cx);
        let slot = self.place.slot();
        let paint = self.view.read(cx).paint.clone();
        let mut paint = paint.borrow_mut();
        let pad = self.pad(&paint.scale());
        let ink = Ink::of(Palette::inherited(&window.text_style(), theme));
        // Dual draws each path on its own pane, so the chrome keeps its slot and builds no crumbs.
        if self.place == Place::Chrome && self.view.read(cx).inputs.dual {
            paint.area = Some(bounds);
            paint.chrome_end = None;
            paint.runs[slot].clear();
            return (
                window.insert_hitbox(bounds, HitboxBehavior::Normal),
                ink,
                false,
            );
        }
        let advance = paint.advance(Size::Caption, window);
        let label = &self.view.read(cx).inputs.git_label;
        let git_width = if label.is_empty() {
            0.
        } else {
            f32::from(
                paint
                    .shape(label, Size::Caption, false, window)
                    .line()
                    .width,
            ) + GIT_GAP
        };
        // The copy mark's slot stays out of the fit, so the mark that shows under the pointer never covers a crumb.
        let copy = self.place == Place::Chrome && self.view.read(cx).inputs.copy_mark;
        let copy_room = if copy {
            let sizes = super::super::chrome::Sizes::at(&paint.typeface);
            sizes.gap + sizes.slot
        } else {
            0.
        };
        let budget = ((f32::from(bounds.size.width) - 2. * pad - git_width - copy_room)
            / f32::from(advance))
        .floor()
        .max(0.) as usize;
        let focused = self.view.update(cx, |view, _| view.fit(slot, budget));
        paint.strips[slot] = Some(bounds);
        if self.place == Place::Chrome {
            paint.area = Some(bounds);
        }
        self.view.update(cx, |view, _| {
            paint.lay_strip(slot, &mut view.strips[slot], |typeface, text| {
                line(typeface, text, Size::Caption, false, window)
            });
            if self.place == Place::Chrome {
                paint.chrome_end = copy.then(|| view.strips[slot].width() + git_width);
            }
        });
        (
            window.insert_hitbox(bounds, HitboxBehavior::Normal),
            ink,
            focused,
        )
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        (hitbox, ink, focused): &mut (Hitbox, Ink, bool),
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = self.view.read(cx);
        if self.place == Place::Chrome && view.inputs.dual {
            return;
        }
        let slot = self.place.slot();
        let strip = &view.strips[slot];
        let mut paint = view.paint.borrow_mut();
        let pane = self.place != Place::Chrome;
        if pane {
            crate::window::glass::producers::paint(
                crate::window::glass::producers::Producer::Path,
                bounds,
                if *focused {
                    ink.surface
                } else {
                    ink.background
                },
                window,
                cx,
            );
            let rule = Bounds::new(
                point(bounds.origin.x, bounds.bottom() - px(1.)),
                size(bounds.size.width, px(1.)),
            );
            window.paint_quad(fill(rule, ink.muted_frame));
        }
        // The ancestors rest muted in the chrome and in the foreground on a dual pane; the leaf and a lit parent draw in the foreground.
        let rest = if pane { ink.foreground } else { ink.muted };
        let style = match strip.hovered.and_then(|at| strip.crumbs().get(at)) {
            Some(crumb) if crumb.parent() => CursorStyle::PointingHand,
            _ => CursorStyle::IBeam,
        };
        window.set_cursor_style(style, hitbox);
        let left = bounds.origin.x + px(self.pad(&paint.scale()));
        let height = if pane {
            bounds.size.height - px(1.)
        } else {
            bounds.size.height
        };
        // The git label trails the leaf, so it moves with the crumbs as part of the one line.
        let git = (!view.inputs.git_label.is_empty())
            .then(|| paint.shape(&view.inputs.git_label, Size::Caption, false, window));
        let trail = git
            .as_ref()
            .map_or(0., |run| GIT_GAP + f32::from(run.line().width));
        // A path keeps its reading order in a mirrored window: its crumbs move as one line.
        let line = Bounds::new(
            point(left, bounds.origin.y),
            size(px(strip.width() + trail), height),
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.with_unmirrored(line, |window| {
                for (index, (crumb, run)) in
                    strip.crumbs().iter().zip(&paint.runs[slot]).enumerate()
                {
                    let held = view.drop_target == Some((slot, index)) && cx.has_active_drag();
                    let lit = crumb.last
                        || held
                        || (crate::list::hover_wash::HoverWash::on(cx)
                            && strip.hovered == Some(index)
                            && !crumb.elided);
                    let extent = strip.extent(index);
                    let x = left + px(extent.map_or(0., |(left, _)| left));
                    if let Some((_, width)) = extent {
                        paint.pointer.painted(
                            format!("crumb:{slot}:{index}"),
                            Bounds::new(point(x, bounds.top()), size(px(width), height)),
                            window,
                        );
                    }
                    // The wash spans the whole crumb, separator included, the same span hit accepts, as v0.3.12 does.
                    if let Some((_, width)) = extent.filter(|_| held) {
                        let target = Bounds::new(point(x, bounds.top()), size(px(width), height));
                        window
                            .paint_quad(fill(target, ink.accent.opacity(crate::list::CURSOR_WASH)));
                        window.paint_quad(fill(
                            Bounds::new(target.origin, size(px(2.), height)),
                            ink.accent_frame,
                        ));
                    }
                    paint_run(
                        run,
                        x,
                        bounds.origin.y,
                        height,
                        if lit { ink.foreground } else { rest },
                        window,
                    );
                }
                if let Some(run) = &git {
                    paint_run(
                        run,
                        left + px(strip.end() + GIT_GAP),
                        bounds.origin.y,
                        height,
                        ink.muted,
                        window,
                    );
                }
            })
        });
    }
}
