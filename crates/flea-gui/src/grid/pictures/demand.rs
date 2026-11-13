//! One thumbnail demand owner for browser panes and the picker, with shared cancellation and background decode.
use super::surface::Want;
use super::{is_image, Key, Pictures, Policy, Slot, FIRST_SETTLE_MS, SETTLE_MS};
use crate::{
    grid::element::{Identity, PictureKind, Thumbnail},
    list::pane::Held,
};
use gpui::{Asset, Context, ImageAssetLoader, RenderImage, Resource, Window};
use std::{rc::Rc, sync::Arc, time::Duration};

pub(crate) struct Demand<'a> {
    pub held: &'a Rc<Held>,
    pub path: &'a str,
    pub live: bool,
    pub busy: bool,
    pub want: Option<Want>,
    pub policy: Policy,
}

// One request line naming rows, the way Backend.qml writes thumb and thumbcancel: {"c":"thumb","rows":[4,5],"cacheOnly":false}.
fn rows_line(command: &str, rows: &[usize], cache: Option<bool>) -> String {
    let rows: Vec<String> = rows.iter().map(usize::to_string).collect();
    let cache = cache.map_or(String::new(), |cache| format!(",\"cacheOnly\":{cache}"));
    format!(
        "{{\"c\":\"{command}\",\"rows\":[{}]{cache}}}",
        rows.join(",")
    )
}

impl Pictures {
    /// Every frame: the shown surface follows its listing and settles its asks; a surface not live lets every picture go.
    pub(crate) fn sync<T: 'static>(
        &self,
        input: Demand<'_>,
        window: &mut Window,
        cx: &mut Context<T>,
        settle: impl FnOnce(&mut T, Key, &mut Context<T>) + 'static,
    ) -> Vec<String> {
        let Demand {
            held,
            path,
            live,
            busy,
            want,
            policy,
        } = input;
        let mut map = self.map.borrow_mut();
        map.policy = Some(policy);
        let mut lines = Vec::new();
        if !live {
            let pending = map.release();
            if !pending.is_empty() && map.listing == held.listing {
                map.counters.cancelled += pending.len();
                lines.push(rows_line("thumbcancel", &pending, None));
            }
        } else if !busy {
            if map.listing != held.listing {
                map.follow(held, path);
            }
            let window_now = Rc::downgrade(held);
            if !map.seen.ptr_eq(&window_now) {
                // Re-plan newly held facts as well as changed rows; the viewport can stay still while its sparse window fills.
                map.key = None;
                map.restore(held);
                map.seen = window_now;
            }
            let cache = map.class_for(path).map(|class| policy.cache_only(class));
            let key = want.as_ref().map(|want| Key {
                listing: held.listing,
                span: want.span,
                only: want.only.as_ref().map(|(rows, first, end)| {
                    (Rc::as_ptr(rows).cast::<usize>() as usize, *first, *end)
                }),
                mode: policy.mode,
                cache,
            });
            // The settle starts again for a changed key, the first in a listing after 70 ms and later ones after 120 ms; Off still settles, so its plan cancels what waits, and a class turned on asks its misses again.
            if key != map.key {
                if map.key.is_some_and(|old| old.cache == Some(true))
                    && key.is_some_and(|new| new.cache == Some(false))
                {
                    let misses: Vec<usize> = map
                        .order
                        .iter()
                        .copied()
                        .filter(|row| matches!(map.slots.get(row), Some(Slot::Missed { .. })))
                        .collect();
                    for row in misses {
                        map.forget(row);
                    }
                }
                map.drawn = want;
                let decoding: Vec<_> = map
                    .order
                    .iter()
                    .copied()
                    .filter(|row| {
                        !map.drawn.as_ref().is_some_and(|want| want.contains(*row))
                            && matches!(map.slots.get(row), Some(Slot::Decoding { .. }))
                    })
                    .collect();
                for row in decoding {
                    map.forget(row);
                }
                map.trim();
                map.key = key;
                map.settle = None;
                if let Some(key) = key {
                    let delay = if map.settled {
                        SETTLE_MS
                    } else {
                        FIRST_SETTLE_MS
                    };
                    map.settle = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(delay))
                            .await;
                        let _ = this.update(cx, |owner, cx| settle(owner, key, cx));
                    }));
                }
            }
        }
        drop(map);
        self.flush_retired(window, cx);
        lines
    }

    pub(super) fn flush_retired<T: 'static>(&self, window: &mut Window, cx: &mut Context<T>) {
        let retired = std::mem::take(&mut self.map.borrow_mut().retired);
        for image in retired {
            cx.drop_image(image, Some(window));
        }
    }

    pub(crate) fn class(&self, path: &str, class: &str) {
        self.map.borrow_mut().class = Some((path.into(), class.into()));
    }

    // The settle fired: Thumbs.plan over the visible rows, cancelling what left them and asking what never was.
    pub(crate) fn ask(&self, held: &Held, busy: bool, key: Key, want: Option<Want>) -> Vec<String> {
        let mut map = self.map.borrow_mut();
        if map.key != Some(key) || held.listing != key.listing || busy {
            return Vec::new();
        }
        map.settle = None;
        let Some(policy) = map.policy else {
            return Vec::new();
        };
        // The footer asks fsinfo for each listing; only a class still unknown once the settle fires is asked here.
        let Some(cache) = key.cache else {
            // Off asks no picture, so it never needs the class either.
            if key.mode != super::Mode::Off && map.asked_fs != key.listing {
                map.asked_fs = key.listing;
                return vec![r#"{"c":"fsinfo"}"#.into()];
            }
            return Vec::new();
        };
        // The frame has drawn since the key was taken, so the plan covers exactly the rows on screen now; none drawn cancels what waits.
        let wanted = want.map_or_else(Vec::new, |want| want.rows());
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
            .filter(|row| !wanted.contains(row) || !allowed(*row) || changed.contains(row))
            .collect();
        for &row in changed.iter().chain(&gone) {
            map.forget(row);
        }
        let ask: Vec<usize> = wanted
            .iter()
            .copied()
            .filter(|row| !map.slots.contains_key(row) && allowed(*row))
            .collect();
        for &row in &ask {
            let Some(facts) = held.row(row) else {
                continue;
            };
            let directory = map.path.clone();
            super::quality::begin_trace(&mut map, row, &directory, &facts.name);
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
        lines
    }

    // PaneWire.onThumbed and ExtThumbs.thumbReply: an answer counts only for a row still asked in the listing it was asked in.
    pub(crate) fn thumbed<T: 'static>(
        self: &Rc<Self>,
        listing: usize,
        path: &str,
        row: usize,
        file: &str,
        cx: &mut Context<T>,
    ) {
        let mut map = self.map.borrow_mut();
        // While a new listing is in flight the held rows are still the ones asked, so the answer counts until it lands.
        if map.listing != listing || !map.drawn.as_ref().is_some_and(|want| want.contains(row)) {
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
                .class_for(path)
                .zip(map.policy)
                .map(|(class, policy)| policy.cache_only(class));
            if cache && cache_now != Some(true) {
                // A lost prefetch on a class that now generates is visible work again: it returns to unasked and the settle re-plans it.
                map.forget(row);
                map.key = None;
            } else {
                let slot = if cache {
                    Slot::Missed { observed: identity }
                } else {
                    Slot::Refused { observed: identity }
                };
                map.counters.refused += usize::from(!cache);
                map.remember(row, slot);
            }
            cx.notify();
            return;
        }
        let load = <ImageAssetLoader as Asset>::load(
            Resource::Path(Arc::from(&*flea::bytename::view(file))),
            cx,
        );
        let work = cx.background_executor().spawn(load);
        let (listing, answered) = (map.listing, file.to_owned());
        let pictures = Rc::downgrade(self);
        let task = cx.spawn(async move |this, cx| {
            let decoded = work.await.ok();
            let _ = this.update(cx, |_, cx| {
                if let Some(pictures) = pictures.upgrade() {
                    pictures.decoded(listing, row, &answered, decoded);
                }
                cx.notify();
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
    fn decoded(&self, listing: usize, row: usize, file: &str, decoded: Option<Arc<RenderImage>>) {
        let mut map = self.map.borrow_mut();
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
                    file: file.to_owned(),
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
                Slot::Failed {
                    identity,
                    file: file.to_owned(),
                }
            }
        };
        map.remember(row, slot);
    }
}
