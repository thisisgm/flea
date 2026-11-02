//! An initial card is atomic, with the existing hold and drawn minimum for slow producers.
use super::keys::Key;
use super::shell::{Block, Button, Card, Ink};
use crate::pane::swap::{Clock, Progress};
use gpui::{Context, Task};
use std::{time::Duration, time::Instant};

#[derive(Default)]
pub(crate) struct Initial {
    clock: Clock,
    progress: Progress<Option<Card>>,
    published: bool,
    beat: Option<(u64, Task<()>)>,
}

impl Initial {
    pub fn at(now: Instant) -> Self {
        let mut initial = Self {
            clock: Clock::at(now),
            ..Self::default()
        };
        initial.progress.restart(0);
        initial
    }

    pub fn published(&self) -> bool {
        self.published
    }

    pub fn key(&self, key: Key) -> Option<Key> {
        if self.published || key == Key::Escape {
            Some(key)
        } else if self.progress.shown().is_some() && matches!(key, Key::Enter | Key::Space) {
            Some(Key::Escape)
        } else {
            None
        }
    }

    pub fn present<T: 'static>(
        &mut self,
        ready: bool,
        card: Card,
        waiting: Card,
        cx: &mut Context<T>,
    ) -> Option<Card> {
        if self.published {
            return Some(card);
        }
        let now = self.clock.elapsed(cx.background_executor().now());
        self.progress
            .observe(&Some(if ready { card } else { waiting }), !ready, now);
        let shown = self.progress.commit(now).clone();
        if ready && !self.progress.holding(now) {
            self.published = true;
            self.beat = None;
        } else if let Some(due) = self.progress.due(now) {
            if !self.beat.as_ref().is_some_and(|(at, _)| *at == due) {
                let task = cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(due.saturating_sub(now)))
                        .await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                });
                self.beat = Some((due, task));
            }
        }
        shown
    }
}

pub(crate) fn loading(catalog: &flea::i18n::I18n, title: &str, width: f32) -> Card {
    Card {
        language: catalog.language().to_owned(),
        title: catalog.tr(title),
        esc: catalog.tr("dialogs.file.esc"),
        width,
        bleed: false,
        body: vec![Block::Note {
            text: catalog.tr("dialogs.file.properties.working"),
            inset: 0.,
            ink: Ink::Muted,
            lift: 0.,
        }],
        buttons: vec![Button {
            label: catalog.tr("dialogs.file.cancel"),
            primary: false,
            focused: true,
            enabled: true,
        }],
    }
}
