//! The Shell side of Grid pictures: the per-frame follow and settle, the thumb and thumbcancel lines, and the thumbed answer.
use super::{is_image, Key, Map, Pictures, Policy, Slot, FIRST_SETTLE_MS, SETTLE_MS};
use crate::grid::{element::Identity, element::PictureKind, element::Thumbnail, Shown};
use crate::{backend::Event, state::Shell, window::startup::View};
use gpui::{Asset, Context, ImageAssetLoader, RenderImage, Resource, Window};
use std::{path::Path, rc::Rc, sync::Arc, time::Duration};

// One request line naming rows, the way Backend.qml writes thumb and thumbcancel.
fn rows_line(command: &str, rows: &[usize], cache: Option<bool>) -> String {
    let rows: Vec<String> = rows.iter().map(usize::to_string).collect();
    let cache = cache.map_or(String::new(), |cache| format!(",\"cacheOnly\":{cache}"));
    format!("{{\"c\":\"{command}\",\"rows\":[{}]{cache}}}", rows.join(","))
}

impl Shell {
    fn grid_pictures(&self, side: usize) -> Option<&Pictures> {
        self.panes[side].as_ref()?.grid.as_ref()?.decor.pictures()
    }

    /// Every frame: the shown Grid pane lends its Pictures, follows its listing and settles its asks; a hidden one lets every picture go.
    pub fn sync_grid_pictures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let policy = Policy::read(&self.launch.settings.data);
        for side in 0..2 {
            let shown = self.view == View::Grid && side == 0 && self.slots.covers[side].is_none();
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            let Some(grid) = host.grid.as_mut() else {
                continue;
            };
            if grid.decor.pictures().is_none() {
                if !shown {
                    continue;
                }
                // Plain stands in until the first shown frame; from here on the pane's tiles borrow these, and the marks the old decor drew ride inside.
                let pictures = Pictures::default();
                pictures.set_clip(Rc::clone(&grid.decor));
                grid.decor = Rc::new(pictures);
            }
            let pictures = grid.decor.pictures().expect("installed above");
            let mut map = pictures.map.borrow_mut();
            map.policy = Some(policy);
            let backend = host.backend.as_ref().filter(|_| !host.stopped);
            let live = shown && grid.view.visible && !host.pane.state.search;
            let mut lines = Vec::new();
            if !live || backend.is_none() {
                let pending = map.release();
                if backend.is_some() && !pending.is_empty() && map.listing == host.pane.held.listing {
                    map.counters.cancelled += pending.len();
                    lines.push(rows_line("thumbcancel", &pending, None));
                }
            } else if !host.pane.busy() {
                let held = &host.pane.held;
                if map.listing != held.listing {
                    map.follow(held, &host.path);
                }
                let window_now = Rc::downgrade(held);
                if !map.seen.ptr_eq(&window_now) {
                    // Rows replaced in place: a changed row is re-planned at once, its old picture already refused by the paint guard.
                    if map.order.iter().any(|&row| map.stale(held, row)) {
                        map.key = None;
                    }
                    map.seen = window_now;
                }
                let span = grid
                    .view
                    .wanted_thumbnails(&host.pane, Shown::All(held.total))
                    .map(|span| (*span.start(), *span.end()));
                let cache = map.class_for(&host.path).map(|class| policy.cache_only(class));
                let key = span.map(|span| Key {
                    listing: held.listing,
                    span,
                    mode: policy.mode,
                    cache,
                });
                if key != map.key {
                    restart(&mut map, key, side, window, cx);
                }
            }
            let retired = std::mem::take(&mut map.retired);
            drop(map);
            for line in lines {
                if let Err(error) = host.backend.as_ref().map_or(Ok(()), |b| b.line(line)) {
                    host.record_error(error, &self.locale);
                    cx.notify();
                }
            }
            for image in retired {
                cx.drop_image(image, Some(window));
            }
        }
    }

    // The settle fired: Thumbs.plan over the visible rows, cancelling what left them and asking what never was.
    fn ask_grid_pictures(&mut self, side: usize, key: Key, cx: &mut Context<Self>) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(pictures) = host.grid.as_ref().and_then(|grid| grid.decor.pictures()) else {
            return;
        };
        let mut map = pictures.map.borrow_mut();
        let held = &host.pane.held;
        if map.key != Some(key) || held.listing != key.listing || host.pane.busy() {
            return;
        }
        map.settle = None;
        let Some(policy) = map.policy else { return };
        // The footer asks fsinfo for each listing; only a class still unknown once the settle fires is asked here.
        let Some(cache) = key.cache else {
            if map.asked_fs != key.listing {
                map.asked_fs = key.listing;
                drop(map);
                if let Err(error) = host.backend.as_ref().map_or(Ok(()), |b| b.fsinfo()) {
                    host.record_error(error, &self.locale);
                    cx.notify();
                }
            }
            return;
        };
        let (first, last) = key.span;
        // A row whose facts changed under its record is forgotten, so it is asked again for what it is now.
        let changed: Vec<usize> = map
            .order
            .iter()
            .copied()
            .filter(|&row| map.stale(held, row))
            .collect();
        let allowed = |row: usize| held.row(row).is_some_and(|facts| policy.allowed(facts));
        let gone: Vec<usize> = map
            .pending()
            .filter(|&row| row < first || row > last || !allowed(row) || changed.contains(&row))
            .collect();
        for &row in changed.iter().chain(&gone) {
            map.forget(row);
        }
        let ask: Vec<usize> = (first..=last)
            .filter(|row| !map.slots.contains_key(row) && allowed(*row))
            .collect();
        for &row in &ask {
            let facts = held.row(row).expect("an allowed row is held");
            let slot = Slot::Asked {
                identity: Identity::of(facts),
                image: is_image(facts),
                cache,
            };
            map.remember(row, slot);
        }
        map.counters.cancelled += gone.len();
        map.counters.asked += ask.len();
        map.settled |= !ask.is_empty();
        drop(map);
        let mut lines = Vec::new();
        if !gone.is_empty() {
            lines.push(rows_line("thumbcancel", &gone, None));
        }
        if !ask.is_empty() {
            lines.push(rows_line("thumb", &ask, Some(cache)));
        }
        for line in lines {
            if let Err(error) = host.backend.as_ref().map_or(Ok(()), |b| b.line(line)) {
                host.record_error(error, &self.locale);
                cx.notify();
            }
        }
    }

    /// One backend event before the Shell routes it: a thumbed line is the Grid's and taken, an fsinfo class is noted and passed on.
    pub fn grid_pictures_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (path, class) = match event {
            Event::FilesystemDetails { facts, class } => (&facts.path, class),
            Event::ListDetails(crate::list::sizes::Reply::Filesystem { path, class }) => {
                (path, class)
            }
            Event::Producer(line) if flea::json::field_str(line, "t").as_deref() == Some("thumbed") => {
                if let Ok(document) = flea::jsondoc::parse(line) {
                    let row = document.get("row").and_then(|row| match row {
                        flea::jsondoc::Json::Num(value) => value.parse::<usize>().ok(),
                        _ => None,
                    });
                    let file = document.get("file").and_then(flea::jsondoc::Json::as_str);
                    if let (Some(row), Some(file)) = (row, file) {
                        self.grid_thumbed(side, row, file, window, cx);
                    }
                }
                return true;
            }
            _ => return false,
        };
        if let Some(pictures) = self.grid_pictures(side) {
            pictures.map.borrow_mut().class = Some((path.clone(), class.clone()));
            cx.notify();
        }
        false
    }

    // PaneWire.onThumbed and ExtThumbs.thumbReply: an answer counts only for a row still asked in the listing it was asked in.
    fn grid_thumbed(
        &mut self,
        side: usize,
        row: usize,
        file: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        let Some(pictures) = host.grid.as_ref().and_then(|grid| grid.decor.pictures()) else {
            return;
        };
        let mut map = pictures.map.borrow_mut();
        // While a new listing is in flight the held rows are still the ones asked, so the answer counts until it lands.
        if map.listing != host.pane.held.listing {
            return;
        }
        let Some(Slot::Asked {
            identity,
            image,
            cache,
        }) = map.slots.get(&row)
        else {
            return;
        };
        let (identity, image, cache) = (identity.clone(), *image, *cache);
        map.counters.answered += 1;
        if file.is_empty() {
            let cache_now = map
                .class_for(&host.path)
                .zip(map.policy)
                .map(|(class, policy)| policy.cache_only(class));
            if cache && cache_now != Some(true) {
                // A lost prefetch on a class that now generates is visible work again: it returns to unasked and the settle re-plans it.
                map.forget(row);
                map.key = None;
            } else {
                let slot = if cache { Slot::Missed } else { Slot::Refused };
                map.counters.refused += usize::from(!cache);
                map.remember(row, slot);
            }
            cx.notify();
            return;
        }
        let load = <ImageAssetLoader as Asset>::load(Resource::Path(Arc::from(Path::new(file))), cx);
        let work = cx.background_executor().spawn(load);
        let (listing, answered) = (map.listing, file.to_owned());
        let task = cx.spawn_in(window, async move |this, cx| {
            let decoded = work.await.ok();
            let _ = cx.update(|_, cx| {
                this.update(cx, |shell, cx| {
                    shell.grid_decoded(side, listing, row, &answered, decoded, cx)
                })
            });
        });
        let slot = Slot::Decoding {
            identity,
            image,
            file: file.to_owned(),
            _task: task,
        };
        map.remember(row, slot);
    }

    // The decode finished: the picture is lent only if its row still waits for this very file.
    fn grid_decoded(
        &mut self,
        side: usize,
        listing: usize,
        row: usize,
        file: &str,
        decoded: Option<Arc<RenderImage>>,
        cx: &mut Context<Self>,
    ) {
        let Some(pictures) = self.grid_pictures(side) else {
            return;
        };
        let mut map = pictures.map.borrow_mut();
        let waiting = match map.slots.get(&row) {
            Some(Slot::Decoding {
                identity,
                image,
                file: wanted,
                ..
            }) if map.listing == listing && wanted == file => Some((identity.clone(), *image)),
            _ => None,
        };
        let Some((identity, image)) = waiting else {
            return;
        };
        let slot = match decoded {
            Some(decoded) => {
                map.counters.decoded += 1;
                Slot::Ready {
                    picture: Arc::new(Thumbnail {
                        identity,
                        kind: PictureKind::Image,
                        image: decoded,
                    }),
                    image,
                }
            }
            None => {
                map.counters.refused += 1;
                Slot::Refused
            }
        };
        map.remember(row, slot);
        cx.notify();
    }
}

// The settle starts again for a changed key, the first in a listing after 70 ms and later ones after 120 ms; Off still settles, so its plan cancels what waits, and a class turned on asks its misses again.
fn restart(map: &mut Map, key: Option<Key>, side: usize, window: &Window, cx: &mut Context<Shell>) {
    if map.key.is_some_and(|old| old.cache == Some(true)) && key.is_some_and(|new| new.cache == Some(false)) {
        let misses: Vec<usize> = map
            .order
            .iter()
            .copied()
            .filter(|row| matches!(map.slots.get(row), Some(Slot::Missed)))
            .collect();
        for row in misses {
            map.forget(row);
        }
    }
    map.key = key;
    map.settle = None;
    let Some(key) = key else {
        return;
    };
    let delay = if map.settled { SETTLE_MS } else { FIRST_SETTLE_MS };
    map.settle = Some(cx.spawn_in(window, async move |this, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(delay))
            .await;
        let _ = cx.update(|_, cx| this.update(cx, |shell, cx| shell.ask_grid_pictures(side, key, cx)));
    }));
}
