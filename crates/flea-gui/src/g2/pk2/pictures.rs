//! The picker Grid borrows the browser's Pictures demand owner and backend thumbnail pool.
use super::view::ListingView;
use crate::grid::{
    element::pictures::{demand::Demand, surface::Want, Key},
    Shown,
};
use flea::jsondoc::Json;
use gpui::{Context, EventEmitter, Window};

pub(crate) struct RequestLine(pub String);
impl EventEmitter<RequestLine> for ListingView {}

impl ListingView {
    fn picture_want(&self) -> Option<Want> {
        if self.view_mode() != "grid" || self.listing().is_pending() {
            return None;
        }
        let total = self.shown().map_or(self.listing().total(), <[usize]>::len);
        let layout = self.grid_view().layout(Shown::All(total));
        let (first, last) = layout.viewport(self.scroll_y());
        let first = first.max(0) as usize;
        let end = usize::try_from(last).ok()?.saturating_add(1).min(total);
        if let Some(rows) = &self.shown {
            let first = first.min(end);
            let drawn = &rows[first..end];
            return Some(Want {
                span: (
                    drawn.iter().min().copied().unwrap_or(0),
                    drawn.iter().max().copied().unwrap_or(0),
                ),
                only: Some((rows.clone(), first, end)),
            });
        }
        (end > first).then_some(Want {
            span: (first, end.saturating_sub(1)),
            only: None,
        })
    }

    pub(super) fn sync_pictures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pictures = self.state.borrow().pictures.clone();
        let held = self.listing().held();
        let path = self.listing().location().to_owned();
        let lines = pictures.sync(
            Demand {
                held: &held,
                path: &path,
                live: self.view_mode() == "grid" && self.listing().state() == "ready",
                busy: self.listing().is_pending(),
                want: self.picture_want(),
                policy: self.picture_policy,
            },
            window,
            cx,
            |view, key, cx| view.ask_pictures(key, cx),
        );
        for line in lines {
            cx.emit(RequestLine(line));
        }
    }

    fn ask_pictures(&mut self, key: Key, cx: &mut Context<Self>) {
        let pictures = self.state.borrow().pictures.clone();
        let held = self.listing().held();
        for line in pictures.ask(&held, self.listing().is_pending(), key, self.picture_want()) {
            cx.emit(RequestLine(line));
        }
    }

    // {"t":"thumbed","row":2,"file":"/home/u/.cache/thumbnails/large/b98fa408.png"} or {"t":"fsinfo","path":"/d","class":"local"}; the picker reads fsinfo only for pictures.
    pub(crate) fn picture_line(&mut self, data: &Json, cx: &mut Context<Self>) -> bool {
        let text = |key| data.get(key).and_then(Json::as_str).unwrap_or_default();
        let pictures = self.state.borrow().pictures.clone();
        match text("t") {
            "fsinfo" => {
                let path = self.listing().location();
                if text("path") == path
                    || (flea::ui_logic_pk1::is_recent(path) && text("path") == "/")
                {
                    pictures.class(path, text("class"));
                }
            }
            "thumbed" => {
                if let Some(Json::Num(row)) = data.get("row") {
                    if let Ok(row) = row.parse() {
                        let held = self.listing().held();
                        let path = self.listing().location().to_owned();
                        pictures.thumbed(held.listing, &path, row, text("file"), cx);
                    }
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }
}
