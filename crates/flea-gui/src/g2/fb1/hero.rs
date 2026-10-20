//! The empty folder's hero view: mark over a rotating caption, a frame loop only while something moves and a timer between beats.
use super::caption::{caseless, tracking_for, Tracked, TRACKING};
use super::{bump, mark::Spiral, now_ms, Counters, Ink};
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

pub struct EmptyHero {
    rotation: Rotation,
    phrases: Vec<SharedString>,
    typeface: Typeface,
    base: Instant,
    reduced: bool,
    in_list: bool,
    tracking: f32,
    timer: Option<(u64, Task<()>)>,
    pub counters: Rc<Counters>,
}

impl EmptyHero {
    pub fn new(catalog: &I18n, cx: &mut Context<Self>) -> Self {
        let mut hero = Self {
            rotation: Rotation::default(),
            phrases: Vec::new(),
            typeface: Typeface::omarchy_default(),
            base: cx.background_executor().now(),
            reduced: cx.reduce_motion(),
            in_list: false,
            tracking: TRACKING,
            timer: None,
            counters: Rc::default(),
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

    /// The pane says whether the hero stands in the list view, where empty.html draws its first phrase.
    pub fn set_in_list(&mut self, in_list: bool, cx: &mut Context<Self>) {
        self.in_list = in_list;
        cx.notify();
    }

    /// The list draws empty.html's numbers until a beat's swap lands, unseen; every other view draws the shared ones.
    pub fn metrics(&self) -> HeroMetrics {
        if self.in_list && !self.rotation.landed() {
            LIST_FIRST
        } else {
            BOARDS
        }
    }

    pub fn rotation(&self) -> &Rotation {
        &self.rotation
    }

    /// The pane reached the empty state; a folder that landed held draws whole.
    pub fn show(&mut self, animate_entrance: bool, cx: &mut Context<Self>) {
        let now = now_ms(self.base, cx);
        self.rotation.show(now, animate_entrance);
        cx.notify();
    }

    /// The caller cuts the hero away; its timer goes with it.
    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.rotation.hide();
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
        let ink = Ink::interim(&Palette::inherited(&window.text_style(), theme));
        let caption = self.phrases.get(frame.phrase).cloned().unwrap_or_default();
        let metrics = self.metrics();
        let mut font = self.typeface.font.clone();
        font.weight = FontWeight::BOLD;
        div()
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
                            div()
                                .debug_selector(|| "fb1-hero-mark".into())
                                .child(Spiral {
                                    drawn: frame.mark_drawn,
                                    opacity: frame.mark_opacity,
                                    ink: ink.muted_frame,
                                }),
                        )
                        .child(
                            div()
                                .debug_selector(|| "fb1-hero-caption".into())
                                .opacity(frame.caption)
                                .child(Tracked {
                                    text: caption,
                                    font,
                                    size: self.typeface.caption,
                                    line: px(metrics.caption_line),
                                    tracking: px(self.tracking),
                                    color: ink.foreground,
                                }),
                        ),
                )
            })
    }
}
