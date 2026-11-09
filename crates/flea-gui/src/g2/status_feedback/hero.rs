//! The empty folder's hero view: captions loop while shown; a fade repaints only the caption, and a timer waits between beats.
use super::caption::{caseless, tracking_for, Place, Slot, TRACKING};
use super::{bump, empty_omi, now_ms, Counters, Ink};
use crate::feedback::spinner::Crawl;
use crate::list::{Palette, Typeface};
use flea::i18n::I18n;
use flea::ui_logic_status_feedback::empty::{Rotation, PHRASES};
use gpui::{
    div, prelude::*, px, App, Context, EntityId, FontWeight, IntoElement, Render, ShapedLine,
    SharedString, Subscription, Task, Window,
};
use std::{rc::Rc, time::Instant};

#[path = "hero_paint.rs"]
mod paint;
use paint::{Face, Layer};

/// The gap from the mark to its caption and the caption's line box, which each hero board states on its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeroMetrics {
    pub gap: f32,
    pub caption_line: f32,
}

/// empty-beat, empty-rest, grid-empty and the column boards: gap 9 and a 16px caption line, shared by every beat.
pub const BOARDS: HeroMetrics = HeroMetrics {
    gap: 9.,
    caption_line: 16.,
};

impl HeroMetrics {
    fn at(self, face: &Typeface) -> Self {
        let body = f32::from(face.body);
        Self {
            gap: (8. * body / 12.).round() + self.gap - BOARDS.gap,
            caption_line: f32::from(face.caption) * 4. / 3.,
        }
    }
}

pub struct EmptyHero {
    pub(crate) omi: Rc<Crawl>,
    column_painted: Option<(bool, f32, f32)>,
    rotation: Rotation,
    phrases: Vec<SharedString>,
    /// PaneStates.qml's hint under the caption: sentence case in the metadata ink, empty when key hints are off.
    hint: SharedString,
    typeface: Typeface,
    base: Instant,
    reduced: bool,
    tracking: f32,
    timer: Option<(u64, Task<()>)>,
    visibility: Option<Subscription>,
    overlaid: bool,
    place: Place,
    face: Option<Face>,
    shaped: Option<(usize, ShapedLine)>,
    pub counters: Rc<Counters>,
    pub painted: super::observe::Paint,
    pub shown_frame: Option<flea::ui_logic_status_feedback::empty::Frame>,
    pub painted_ink: Option<Ink>,
}

impl EmptyHero {
    pub fn set_typeface(&mut self, typeface: Typeface, cx: &mut Context<Self>) {
        if self.typeface != typeface {
            self.typeface = typeface;
            cx.notify();
        }
    }
    pub fn new(catalog: &I18n, cx: &mut Context<Self>) -> Self {
        let mut hero = Self {
            omi: Rc::default(),
            column_painted: None,
            rotation: Rotation::default(),
            phrases: Vec::new(),
            hint: SharedString::default(),
            typeface: Typeface::omarchy_default(),
            base: cx.background_executor().now(),
            reduced: cx.reduce_motion(),
            tracking: TRACKING,
            timer: None,
            visibility: None,
            overlaid: false,
            place: Rc::default(),
            face: None,
            shaped: None,
            counters: Rc::default(),
            painted: Default::default(),
            shown_frame: None,
            painted_ink: None,
        };
        hero.rotation.set_reduced(hero.reduced, 0);
        hero.translated(catalog);
        hero
    }

    /// Phrases are translated, raised and tracked once per language (caseless scripts neither), never in render or paint.
    pub fn translated(&mut self, catalog: &I18n) {
        self.shaped = None;
        let raised = !caseless(catalog.language());
        self.tracking = tracking_for(catalog.language());
        self.phrases = PHRASES
            .iter()
            .map(|key| {
                let phrase = catalog.tr(key);
                if raised {
                    phrase.to_uppercase().into()
                } else {
                    phrase.into()
                }
            })
            .collect();
    }

    #[cfg(test)]
    pub fn hint(&self) -> &SharedString {
        &self.hint
    }

    pub fn phrases(&self) -> &[SharedString] {
        &self.phrases
    }

    /// The caller's resolved hint line, or empty for none; only a change repaints.
    pub fn set_hint(&mut self, hint: SharedString, cx: &mut Context<Self>) {
        if self.hint != hint {
            self.hint = hint;
            cx.notify();
        }
    }

    pub fn reduced(&self) -> bool {
        self.reduced
    }

    /// PaneStates places the complete hero using its widest translated phrase.
    pub fn natural_width(&self, window: &Window) -> f32 {
        let mut font = self.typeface.font.clone();
        font.weight = FontWeight::BOLD;
        self.phrases
            .iter()
            .map(|phrase| {
                super::caption::measure(
                    phrase.clone(),
                    font.clone(),
                    self.typeface.caption,
                    px(self.tracking),
                    window,
                )
            })
            .chain((!self.hint.is_empty()).then(|| {
                super::caption::measure(
                    self.hint.clone(),
                    self.typeface.font.clone(),
                    self.typeface.caption,
                    px(0.),
                    window,
                )
            }))
            .fold(super::mark::HERO, f32::max)
    }

    /// Every view and every beat draws one caption line, so a rotation never moves the caption.
    pub fn metrics(&self) -> HeroMetrics {
        BOARDS.at(&self.typeface)
    }

    /// A pane's hero: under the window root its overlay paints this caption above the cached pane, so a fade never renders the hero.
    pub(crate) fn set_overlaid(&mut self) {
        self.overlaid = true;
    }

    pub fn rotation(&self) -> &Rotation {
        &self.rotation
    }

    /// The visibility and alphas used by the last frame's hero and mark.
    pub fn painted(&self) -> Option<(bool, f32, f32)> {
        self.column_painted
    }

    /// The pane reached the empty state; a folder that landed held draws whole.
    pub fn show(&mut self, animate_entrance: bool, cx: &mut Context<Self>) {
        let now = now_ms(self.base, cx);
        self.timer = None;
        self.rotation.show(now, animate_entrance);
        cx.notify();
    }

    /// The caller cuts the hero away; its timer goes with it.
    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.column_painted = None;
        self.rotation.hide();
        self.omi.show(false, gpui::Hsla::default());
        self.painted.clear();
        self.shown_frame = None;
        self.timer = None;
        cx.notify();
    }

    fn arm(&mut self, deadline: u64, now: u64, wake: EntityId, cx: &mut Context<Self>) {
        if self
            .timer
            .as_ref()
            .is_some_and(|(armed, _)| *armed == deadline)
        {
            return;
        }
        bump(&self.counters.timers);
        let wait = std::time::Duration::from_millis(deadline.saturating_sub(now));
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |hero, cx| {
                hero.timer = None;
                let app: &mut App = cx;
                app.notify(wake);
            });
        });
        self.timer = Some((deadline, task));
    }

    fn sync_motion(&mut self, now: u64, cx: &mut Context<Self>) {
        if cx.reduce_motion() != self.reduced {
            self.reduced = cx.reduce_motion();
            self.timer = None;
            self.rotation.set_reduced(self.reduced, now);
        }
    }

    fn follow_visibility(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.visibility.is_some() {
            return;
        }
        let weak = cx.entity().downgrade();
        self.visibility = Some(window.observe_window_visibility(move |_, window, cx| {
            let _ = weak.update(cx, |hero, cx| {
                hero.timer = None;
                if window.is_visible() && hero.rotation.visible() {
                    hero.rotation.show(now_ms(hero.base, cx), false);
                    cx.notify();
                }
            });
        }));
    }
}

impl Render for EmptyHero {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        bump(&self.counters.renders);
        self.follow_visibility(window, cx);
        let now = now_ms(self.base, cx);
        self.sync_motion(now, cx);
        self.rotation.advance(now);
        let frame = self.rotation.frame(now);
        self.shown_frame = Some(frame);
        self.painted.clear();
        self.column_painted = Some((self.rotation.visible(), frame.opacity, 1.));
        if window.is_visible() && self.rotation.entering() {
            bump(&self.counters.frames);
            window.request_animation_frame();
        }
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let ink = Ink::interim(&palette);
        self.omi.show(
            self.rotation.visible(),
            palette.accent.opacity(frame.opacity),
        );
        self.typeface = Typeface::for_window(window, cx);
        self.painted_ink = Some(ink);
        // Only the shipped window root carries the overlay that paints a mounted pane's caption.
        let overlay = self.overlaid
            && window
                .root::<crate::window::root::Root>()
                .flatten()
                .is_some();
        let metrics = self.metrics();
        let mut font = self.typeface.font.clone();
        font.weight = FontWeight::BOLD;
        let face = Face {
            font,
            size: self.typeface.caption,
            line: px(metrics.caption_line),
            tracking: px(self.tracking),
            color: ink.foreground,
        };
        if self.face.as_ref() != Some(&face) {
            self.face = Some(face.clone());
            self.shaped = None;
        }
        let widest = self
            .phrases
            .iter()
            .map(|phrase| {
                super::caption::measure(
                    phrase.clone(),
                    face.font.clone(),
                    face.size,
                    face.tracking,
                    window,
                )
            })
            .fold(0., f32::max);
        let mark_side = super::mark::HERO;
        let hint_line = if self.hint.is_empty() {
            0.
        } else {
            metrics.gap + metrics.caption_line
        };
        let root = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .when(self.rotation.visible(), |root| {
                root.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .h(px(mark_side
                            + metrics.gap
                            + metrics.caption_line
                            + hint_line))
                        .flex_shrink_0()
                        .gap(px(metrics.gap))
                        .mt(px(2. * frame.offset))
                        .opacity(frame.opacity)
                        .child(
                            self.painted.watch(
                                div()
                                    .text_color(palette.accent)
                                    .relative()
                                    .debug_selector(|| "fb1-hero-mark".into())
                                    .child(empty_omi::Slot {
                                        mark: Rc::clone(&self.omi),
                                        side: mark_side,
                                    }),
                                "mark",
                                SharedString::default(),
                            ),
                        )
                        .child(
                            div()
                                .relative()
                                .debug_selector(|| "fb1-hero-caption".into())
                                .child(Slot {
                                    place: Rc::clone(&self.place),
                                    width: widest,
                                    line: face.line,
                                }),
                        )
                        .when(!self.hint.is_empty(), |column| {
                            column.child(
                                self.painted.watch(
                                    div()
                                        .relative()
                                        .debug_selector(|| "fb1-empty-key-hint".into())
                                        .h(face.line)
                                        .text_size(self.typeface.caption)
                                        .text_color(ink.metadata)
                                        .whitespace_nowrap()
                                        .child(self.hint.clone()),
                                    "hint",
                                    self.hint.clone(),
                                ),
                            )
                        }),
                )
            })
            .when(!overlay, |root| root.child(Layer(cx.entity())));
        self.painted.watch(root, "frame", SharedString::default())
    }
}
