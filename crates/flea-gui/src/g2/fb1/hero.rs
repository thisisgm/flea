//! The empty folder's hero view: mark over a rotating caption, a frame loop only while something moves and a timer between beats.
use super::caption::{caseless, tracking_for, Tracked, TRACKING};
use super::{bump, empty_omi, now_ms, Counters, Ink};
use crate::feedback::spinner::Crawl;
use crate::list::{Palette, Typeface};
use flea::i18n::I18n;
use flea::ui_logic_fb1::empty::{Rotation, PHRASES};
use gpui::{
    div, prelude::*, px, Context, FontWeight, IntoElement, Render, SharedString, Task, Window,
};
use std::{rc::Rc, time::Instant};

/// The gap from the mark to its caption and the caption's line box, which each hero board states on its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeroMetrics {
    pub gap: f32,
    pub caption_line: f32,
}

/// empty.html, the list's first phrase: gap 10 and the caption in a 14px line box at 1.2 (16.8px).
pub const LIST_FIRST: HeroMetrics = HeroMetrics {
    gap: 10.,
    caption_line: 16.8,
};

/// empty-beat, empty-rest, grid-empty and the column boards: gap 9 and a 16px caption line.
pub const BOARDS: HeroMetrics = HeroMetrics {
    gap: 9.,
    caption_line: 16.,
};

impl HeroMetrics {
    fn at(self, face: &Typeface) -> Self {
        let body = f32::from(face.body);
        Self {
            gap: (8. * body / 12.).round() + self.gap - BOARDS.gap,
            caption_line: if self == LIST_FIRST {
                body * 1.2
            } else {
                f32::from(face.caption) * 4. / 3.
            },
        }
    }
}

pub struct EmptyHero {
    pub(crate) omi: Rc<Crawl>,
    column_painted: Option<(bool, f32, f32)>,
    rotation: Rotation,
    phrases: Vec<SharedString>,
    typeface: Typeface,
    base: Instant,
    reduced: bool,
    in_list: bool,
    tracking: f32,
    timer: Option<(u64, Task<()>)>,
    pub counters: Rc<Counters>,
    pub painted: super::observe::Paint,
    pub shown_frame: Option<flea::ui_logic_fb1::empty::Frame>,
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
            typeface: Typeface::omarchy_default(),
            base: cx.background_executor().now(),
            reduced: cx.reduce_motion(),
            in_list: false,
            tracking: TRACKING,
            timer: None,
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

    pub fn phrases(&self) -> &[SharedString] {
        &self.phrases
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
            .fold(super::mark::HERO, f32::max)
    }

    /// The pane says whether the hero stands in the list view, where empty.html draws its first phrase.
    pub fn set_in_list(&mut self, in_list: bool, cx: &mut Context<Self>) {
        self.in_list = in_list;
        cx.notify();
    }

    /// The list draws empty.html's numbers until a beat's swap lands, unseen; every other view draws the shared ones.
    pub fn metrics(&self) -> HeroMetrics {
        if self.in_list && !self.rotation.landed() {
            LIST_FIRST.at(&self.typeface)
        } else {
            BOARDS.at(&self.typeface)
        }
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

    fn arm(&mut self, deadline: u64, now: u64, cx: &mut Context<Self>) {
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
                cx.notify();
            });
        });
        self.timer = Some((deadline, task));
    }
}

impl Render for EmptyHero {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        bump(&self.counters.renders);
        let now = now_ms(self.base, cx);
        if cx.reduce_motion() != self.reduced {
            self.reduced = cx.reduce_motion();
            self.rotation.set_reduced(self.reduced, now);
        }
        self.rotation.advance(now);
        let frame = self.rotation.frame(now);
        self.shown_frame = Some(frame);
        self.painted.clear();
        self.column_painted = Some((self.rotation.visible(), frame.opacity, 1.));
        if self.rotation.animating() {
            bump(&self.counters.frames);
            window.request_animation_frame();
        } else if let Some(deadline) = self.rotation.next_deadline() {
            self.arm(deadline, now, cx);
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
        let caption = self.phrases.get(frame.phrase).cloned().unwrap_or_default();
        let metrics = self.metrics();
        let mut font = self.typeface.font.clone();
        font.weight = FontWeight::BOLD;
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
                                        side: (f32::from(self.typeface.body_small) * 3.7).round(),
                                    }),
                                "mark",
                                SharedString::default(),
                            ),
                        )
                        .child(
                            self.painted.watch_tracked(
                                div()
                                    .relative()
                                    .text_color(ink.foreground)
                                    .debug_selector(|| "fb1-hero-caption".into())
                                    .opacity(frame.caption),
                                "caption",
                                Tracked {
                                    text: caption,
                                    font,
                                    size: self.typeface.caption,
                                    line: px(metrics.caption_line),
                                    tracking: px(self.tracking),
                                    color: ink.foreground,
                                },
                            ),
                        ),
                )
            });
        self.painted.watch(root, "frame", SharedString::default())
    }
}
