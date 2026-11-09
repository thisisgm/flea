//! A rested file builds the closed card; cursor sweeps cancel the pending warm.
use super::*;
use crate::{list::Typeface, palette::Theme};
use gpui::App;
use std::sync::Arc;

#[path = "quick_look_prepare.rs"]
mod prepare;

#[derive(Clone, PartialEq)]
struct Key {
    directory: String,
    listing: usize,
    cursor: usize,
    row: RowFacts,
    theme: Theme,
    face: Typeface,
    locale: u64,
    storage: Option<String>,
}

#[derive(Default)]
pub(crate) struct Warm {
    key: Option<Key>,
    seen: Option<(String, usize)>,
    moved: bool,
    timer: Option<Task<()>>,
    prepare_timer: Option<Task<()>>,
    prepared_watch: Option<Subscription>,
    storage: Option<(String, String)>,
    pub(crate) prepared: Option<crate::g2::quick_look_overlay::doc::Doc>,
    /// The Markdown view the open card shows, taken back on close in place of a second read.
    lent: Option<(crate::g2::quick_look_overlay::doc::Doc, Key)>,
}

impl Warm {
    pub(crate) fn set_catalog(&self, catalog: &Arc<flea::i18n::I18n>, cx: &mut App) {
        for doc in self
            .prepared
            .iter()
            .chain(self.lent.iter().map(|(doc, _)| doc))
        {
            doc.view.set_catalog(catalog, cx);
        }
    }
}

impl Shell {
    pub(crate) fn sync_quick_look_warm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.focus_side;
        for other in 0..2 {
            if other != side {
                if let Some(host) = &mut self.panes[other] {
                    host.look_warm.key = None;
                    host.look_warm.timer = None;
                    host.look_warm.prepare_timer = None;
                    host.look_warm.prepared = None;
                    host.look_warm.prepared_watch = None;
                    host.look_warm.lent = None;
                }
            }
        }
        let theme = cx.try_global::<Theme>().copied().unwrap_or_default();
        let face = self.active_typeface();
        let open = self.quick_look_open(cx);
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let position = (host.pane.held.listing, host.pane.cursor);
        let warm = &mut host.look_warm;
        let moved = warm
            .seen
            .as_ref()
            .is_some_and(|(path, cursor)| path != &host.path || *cursor != position.1);
        warm.moved |= moved;
        if moved || warm.seen.is_none() {
            warm.seen = Some((host.path.clone(), position.1));
        }
        let row = host.pane.held.row(host.pane.cursor).filter(|row| {
            !row.dir
                && !row.name.is_empty()
                && (warm.moved || kinds::is_markdown(&row.name))
                && (kinds::is_markdown(&row.name)
                    || matches!(
                        kinds::quick_look_kind(&row.icon, &row.name, false),
                        State::Pdf
                            | State::Video
                            | State::Audio
                            | State::Image
                            | State::Text
                            | State::Code
                    ))
        });
        let row = row.filter(|_| !open && !host.stopped && !host.pane.busy());
        let Some(row) = row else {
            warm.key = None;
            if !open {
                warm.lent = None;
            }
            warm.timer = None;
            warm.prepare_timer = None;
            warm.prepared = None;
            warm.prepared_watch = None;
            return;
        };
        let storage = warm
            .storage
            .as_ref()
            .filter(|(path, _)| path == &host.path)
            .map(|(_, class)| class.as_str());
        if warm.key.as_ref().is_some_and(|key| {
            key.directory == host.path
                && key.listing == position.0
                && key.cursor == position.1
                && key.row == *row
                && key.theme == theme
                && key.face == face
                && key.locale == self.locale.revision
                && key.storage.as_deref() == storage
        }) {
            return;
        }
        let key = Key {
            directory: host.path.clone(),
            listing: position.0,
            cursor: position.1,
            row: row.clone(),
            theme,
            face,
            locale: self.locale.revision,
            storage: storage.map(str::to_owned),
        };
        warm.prepared = None;
        warm.prepared_watch = None;
        warm.prepare_timer = None;
        warm.key = Some(key.clone());
        let card_key = key.clone();
        warm.timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1000))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if !shell.warm_matches(side, &card_key, cx) {
                    return;
                }
                shell.quick_look_host(side, window, cx);
                shell.panes[side].as_mut().unwrap().look_warm.timer = None;
                cx.notify();
            });
        }));
        self.arm_quick_prepare(side, &key, window, cx);
    }

    fn warm_matches(&self, side: usize, key: &Key, cx: &App) -> bool {
        side == self.focus_side
            && !self.quick_look_open(cx)
            && self.panes[side].as_ref().is_some_and(|host| {
                !host.stopped
                    && !host.pane.busy()
                    && host.look_warm.key.as_ref() == Some(key)
                    && host.path == key.directory
                    && host.pane.held.listing == key.listing
                    && host.pane.cursor == key.cursor
                    && host.pane.held.row(key.cursor) == Some(&key.row)
                    && self.locale.revision == key.locale
                    && cx.try_global::<Theme>().copied().unwrap_or_default() == key.theme
                    && self.active_typeface() == key.face
                    && host
                        .look_warm
                        .storage
                        .as_ref()
                        .filter(|(path, _)| path == &host.path)
                        .map(|(_, class)| class.as_str())
                        == key.storage.as_deref()
            })
    }
}
