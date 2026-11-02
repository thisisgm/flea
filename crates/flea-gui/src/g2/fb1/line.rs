//! The status line's message lane: the window's one message service, drawn as primary, hint, the rest of the lane and the undo target.
use super::{bump, now_ms, Counters, Ink};
use crate::list::{Palette, Typeface};
use flea::i18n::I18n;
use flea::ui_logic_fb1::messages::{Line, Messages};
use flea::ui_logic_fb1::status::{Arm, SEP};
use gpui::{
    div, prelude::*, px, App, Context, EventEmitter, IntoElement, Render, RenderOnce, SharedString,
    Task, Window,
};
use std::{rc::Rc, sync::Arc, time::Instant};

#[path = "line_busy.rs"]
mod busy;

/// The bar ended an arm on its own clock or by a move, so its owner must zero that stamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmLost(pub Arm);

/// What the undo target runs; the caller gates it through the same predicate as the undo key.
pub type Undo = Rc<dyn Fn(&mut Window, &mut App)>;

/// The drawn line, translated and joined when the service changes, never per render.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drawn {
    pub primary: SharedString,
    pub error: bool,
    pub hint: SharedString,
    pub extra: SharedString,
    pub undo: SharedString,
    pub detail: SharedString,
    pub busy: bool,
}

impl From<Line> for Drawn {
    fn from(line: Line) -> Self {
        // StatusBar.qml draws one line: an error's later lines join its detail strip, any other break is a space.
        let (primary, rest) = match line.primary.split_once('\n') {
            Some((first, rest)) if line.error => (first.to_owned(), rest.trim_end()),
            _ => (one_line(&line.primary), ""),
        };
        let detail = match (rest.is_empty(), line.detail.is_empty()) {
            (true, _) => line.detail,
            (false, true) => rest.to_owned(),
            (false, false) => format!("{rest}\n{}", line.detail),
        };
        Self {
            primary: primary.into(),
            error: line.error,
            hint: one_line(&line.hint).into(),
            extra: one_line(&line.extra).into(),
            undo: one_line(&line.undo).into(),
            detail: detail.into(),
            busy: line.busy,
        }
    }
}

fn one_line(text: &str) -> String {
    if !text.contains('\n') {
        return text.to_owned();
    }
    text.split('\n')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub struct MessageLine {
    messages: Messages,
    catalog: Arc<I18n>,
    state_message: String,
    retry: String,
    drawn: Drawn,
    spinner: Rc<crate::feedback::spinner::Crawl>,
    typeface: Typeface,
    base: Instant,
    timer: Option<(u64, Task<()>)>,
    undo: Option<Undo>,
    pub counters: Rc<Counters>,
    pub painted: super::observe::Paint,
}

impl EventEmitter<ArmLost> for MessageLine {}

/// The notice reached its display time and left the line, so a status that quoted it must let go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoticeEnded;

impl EventEmitter<NoticeEnded> for MessageLine {}

impl MessageLine {
    pub fn new(preset: &str, catalog: Arc<I18n>, cx: &mut Context<Self>) -> Self {
        Self {
            messages: Messages::new(preset),
            catalog,
            state_message: String::new(),
            retry: String::new(),
            drawn: Drawn::default(),
            spinner: Rc::default(),
            typeface: Typeface::omarchy_default(),
            base: cx.background_executor().now(),
            timer: None,
            undo: None,
            counters: Rc::default(),
            painted: Default::default(),
        }
    }

    pub fn messages(&self) -> &Messages {
        &self.messages
    }

    pub fn drawn(&self) -> &Drawn {
        &self.drawn
    }

    /// The face the last render drew with.
    pub fn typeface(&self) -> &Typeface {
        &self.typeface
    }

    pub fn now(&self, cx: &App) -> u64 {
        now_ms(self.base, cx)
    }

    /// The head error's detail for the status composition to draw under the strip; None when there is none.
    pub fn detail(&self, window: &Window, cx: &App) -> Option<Detail> {
        (!self.drawn.detail.is_empty()).then(|| Detail {
            text: self.drawn.detail.clone(),
            typeface: Typeface::for_window(window, cx),
        })
    }

    pub fn on_undo(&mut self, undo: Undo) {
        self.undo = Some(undo);
    }

    /// A new language re-joins the hint words; standing sentences keep the words they were said in.
    pub fn translated(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.catalog = catalog;
        self.refresh(cx);
    }

    /// The pane's own state sentence, under which the dismiss key is no news.
    pub fn set_state_message(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.state_message != text {
            self.state_message = text.into();
            self.refresh(cx);
        }
    }

    /// The retry-selection line under the activity, re-joined only when its text moves.
    pub fn set_retry_line(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.retry != text {
            self.retry = text.into();
            self.messages.set_retry(text);
            self.refresh(cx);
        }
    }

    /// Every change goes through here with the service's clock, then the line is joined and the next wake armed.
    pub fn update<R>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Messages, u64) -> R,
    ) -> R {
        let now = self.now(cx);
        let result = change(&mut self.messages, now);
        self.refresh(cx);
        result
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let drawn = Drawn::from(self.messages.line(&self.catalog, &self.state_message));
        if drawn != self.drawn {
            self.drawn = drawn;
            cx.notify();
        }
        self.arm(cx);
    }

    fn arm(&mut self, cx: &mut Context<Self>) {
        let Some(deadline) = self.messages.next_deadline() else {
            self.timer = None;
            return;
        };
        if self
            .timer
            .as_ref()
            .is_some_and(|(armed, _)| *armed == deadline)
        {
            return;
        }
        bump(&self.counters.timers);
        let wait = std::time::Duration::from_millis(deadline.saturating_sub(self.now(cx)));
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |line, cx| line.due(cx));
        });
        self.timer = Some((deadline, task));
    }

    fn due(&mut self, cx: &mut Context<Self>) {
        self.timer = None;
        let now = self.now(cx);
        let tick = self.messages.tick(now);
        if let Some(arm) = tick.arm_lost {
            cx.emit(ArmLost(arm));
        }
        if tick.notice_cleared {
            cx.emit(NoticeEnded);
        }
        self.refresh(cx);
    }
}

impl Render for MessageLine {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.typeface = Typeface::for_window(window, cx);
        bump(&self.counters.renders);
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let ink = Ink::on_surface(&palette);
        self.spinner.show(self.drawn.busy, palette.accent);
        let drawn = &self.drawn;
        self.painted.clear();
        let text = |name, value: SharedString, color| {
            self.painted.watch(
                div()
                    .relative()
                    .font_family(self.typeface.font.family.clone())
                    .text_size(self.typeface.caption)
                    .text_color(color)
                    .whitespace_nowrap()
                    .child(value.clone()),
                name,
                value,
            )
        };
        let width = |text: &str| {
            let font = self.typeface.font.clone();
            super::caption::measure(
                text.to_owned().into(),
                font,
                self.typeface.caption,
                px(0.),
                window,
            )
            .ceil()
        };
        let undo = self.undo.clone();
        div()
            .size_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .min_w_0()
            .overflow_hidden()
            .when(drawn.busy, |row| {
                row.child(busy::element(self.spinner.clone()))
            })
            .when(!drawn.primary.is_empty(), |row| {
                row.child(
                    text(
                        "primary",
                        drawn.primary.clone(),
                        if drawn.error {
                            ink.error
                        } else {
                            ink.foreground
                        },
                    )
                    .flex_shrink(1.)
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis_middle()
                    .text_color(if drawn.error {
                        ink.error
                    } else {
                        ink.foreground
                    }),
                )
            })
            .when(!drawn.hint.is_empty(), |row| {
                row.child(text("hint", drawn.hint.clone(), ink.muted).flex_none())
            })
            .when(!drawn.extra.is_empty(), |row| {
                // StatusBar.qml: the rest gives way before the notice does, down to " · …", and never draws past its own width.
                let whole = width(&drawn.extra);
                row.child(
                    text("extra", drawn.extra.clone(), ink.muted)
                        .flex_1()
                        .min_w(px(width(&format!("{SEP}\u{2026}")).min(whole)))
                        .max_w(px(whole))
                        .overflow_hidden()
                        .text_ellipsis(),
                )
            })
            .when(!drawn.undo.is_empty(), |row| {
                row.child(text("undo-dot", SEP.into(), ink.muted).flex_none())
                    .child(
                        text("undo", drawn.undo.clone(), ink.muted)
                            .id("fb1-undo")
                            .debug_selector(|| "fb1-undo".into())
                            .flex_none()
                            .cursor_pointer()
                            .text_color(ink.muted)
                            .hover(move |style| style.text_color(ink.foreground))
                            // A press and its release on the target, as the oracle's ReleaseWithinBounds tap; a drag ending here does nothing.
                            .on_click(move |_, window, cx| {
                                if let Some(undo) = &undo {
                                    undo(window, cx);
                                }
                            }),
                    )
            })
    }
}

/// StatusBar.qml's detailView: the whole width under the strip, the row padding round wrapped caption text, at most a third of the window, scrolling past that.
#[derive(IntoElement)]
pub struct Detail {
    text: SharedString,
    typeface: Typeface,
}

/// Row padding, Theme.spacing.rowPaddingX and rowPaddingY at body 14.
pub const DETAIL_PAD_X: f32 = 14.;
pub const DETAIL_PAD_Y: f32 = 7.;
/// A caption line, as the hero's caption draws it.
pub const DETAIL_LINE: f32 = 16.;

impl RenderOnce for Detail {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = Palette::inherited(&window.text_style(), theme);
        let ink = Ink::interim(&palette);
        let cap = window.viewport_size().height / 3.;
        #[cfg(test)]
        let observed_text = self.text.clone();
        let text = div()
            .debug_selector(|| "fb1-detail-text".into())
            .mx(px(DETAIL_PAD_X))
            .my(px(DETAIL_PAD_Y))
            .font_family(self.typeface.font.family.clone())
            .text_size(self.typeface.caption)
            .line_height(px(DETAIL_LINE))
            .text_color(ink.error)
            .child(self.text);
        #[cfg(test)]
        let text = super::detail_tests::typography::watch(text, observed_text, cx);
        div()
            .id("fb1-detail")
            .debug_selector(|| "fb1-detail".into())
            .w_full()
            .max_h(cap)
            .overflow_y_scroll()
            .bg(palette.background)
            .child(text)
    }
}
