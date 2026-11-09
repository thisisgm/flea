//! Contents search in the pane: held rows joined with their match lines, drawn as blocks over the list's frame.
use crate::{g2::file_search, state::Shell};
use gpui::{div, prelude::*, App, Window};
use std::{cell::RefCell, rc::Rc};

impl Shell {
    /// Query and columns shared by ordinary rows and their contents overlay.
    pub(crate) fn configure_search_rows(
        host: &crate::window::pane::PaneHost,
        data: &flea::jsondoc::Json,
        dual: bool,
    ) {
        use crate::list::columns::{Column, Switched};
        let query = match host.filter.shown {
            Some(_) if !host.pane.state.search => host.filter.owner.query(),
            _ => &host.pane.state.query,
        };
        let mut list = host.list.borrow_mut();
        list.view.set_query(query);
        list.view.set_searching(host.pane.state.search);
        list.view.set_lang_marks(crate::list::lang_marks(data));
        // A midnight under 23 h old (the shortest local day) is still today's; past that the list re-reads it.
        const SHORTEST_DAY_SECS: i64 = 23 * 3600;
        let highlight = data
            .get("highlightToday")
            .and_then(flea::jsondoc::Json::as_bool)
            == Some(true);
        list.view.today = highlight.then(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs() as i64);
            match list.view.today {
                Some(start) if now - start < SHORTEST_DAY_SECS => start,
                _ => crate::row::format::day_start(now),
            }
        });
        let switched = Switched::from_settings(data).for_dual(dual);
        list.view.switched = if host.pane.state.search {
            switched
                .with(Column::Mode, false)
                .with(Column::Date, false)
                .with(Column::Kind, false)
                .with(Column::Size, true)
        } else {
            switched
        };
    }

    /// ViewState.qml's midnight timer: an idle list repaints once at local midnight, so today's highlight moves on.
    pub(crate) fn keep_midnight_wake(&mut self, cx: &mut gpui::Context<Self>) {
        let data = &self.launch.settings.data;
        if data
            .get("highlightToday")
            .and_then(flea::jsondoc::Json::as_bool)
            != Some(true)
        {
            self.midnight = None;
            return;
        }
        if self.midnight.is_some() {
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs() as i64);
        // One second past midnight, so the repaint lands on the new day.
        let wait = crate::row::format::seconds_until_midnight(now).max(0) as u64 + 1;
        self.midnight = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(wait))
                .await;
            let _ = this.update(cx, |shell, cx| {
                shell.midnight = None;
                cx.notify();
            });
        }));
    }

    /// Before the lists' frames are prepared: a contents snapshot takes the block height, any other the list's own.
    pub(crate) fn sync_contents_rows(&mut self) {
        for host in self.panes.iter_mut().flatten() {
            let held = &host.pane.held;
            let contents = host.search.owner.active()
                && host
                    .search
                    .owner
                    .matches
                    .covers(held.listing, held.start, held.rows.len());
            let mut list = host.list.borrow_mut();
            // Only a change of snapshot kind moves the row height, so every other frame keeps the pane's own.
            if list.view.contents != contents {
                list.view.contents = contents;
                host.pane.set_row_height(list.file_metrics().row);
            }
        }
    }

    /// The pane's contents blocks: its held rows joined with their match lines, or none while any row lacks its line.
    pub(crate) fn contents_blocks(&self, side: usize) -> Option<file_search::contents::Held> {
        let host = self.panes.get(side)?.as_ref()?;
        let search = &host.search.owner;
        if !search.active() {
            return None;
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let scope = search.session.scope;
        let folder = if search.session.from.is_empty() {
            &host.path
        } else {
            &search.session.from
        };
        let root = flea::ui_logic_file_search::Session::scope_root(folder, &home, scope)?;
        let held = &host.pane.held;
        let place = file_search::matches::Place {
            root: &root,
            home: &home,
            scope,
        };
        search
            .matches
            .blocks(held.listing, held.total, held.start, &held.rows, &place)
    }

    /// The contents blocks drawn over the list's own frame, which keeps its stamps; none for any other snapshot.
    pub(crate) fn contents_element(
        host: &mut crate::window::pane::PaneHost,
        data: &flea::jsondoc::Json,
        held: Option<Rc<file_search::contents::Held>>,
        rows: crate::list::RowStates,
        scroll: f32,
        window: &Window,
        cx: &App,
    ) -> Option<gpui::AnyElement> {
        let held = held?;
        let list = host.list.borrow();
        let words = Rc::clone(&list.view.words);
        if host.search.contents.as_ref().is_none_or(|(made, state)| {
            !Rc::ptr_eq(made, &words) || state.borrow().typeface != list.typeface
        }) {
            let state =
                file_search::contents::ContentsState::new(list.typeface.clone(), (*words).clone());
            host.search.contents = Some((words, Rc::new(RefCell::new(state))));
        }
        drop(list);
        let state = Rc::clone(&host.search.contents.as_ref()?.1);
        {
            let mut shaped = state.borrow_mut();
            shaped.apply_settings(data);
            shaped.query.clone_from(&host.search.owner.session.query);
        }
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = file_search::contents::Palette::from_list(&crate::list::Palette::inherited(
            &window.text_style(),
            theme,
        ));
        let blocks = file_search::contents::ContentsElement::new(
            held,
            scroll,
            host.pane.cursor,
            palette,
            state,
        )
        .with_states(rows);
        Some(
            div()
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .child(blocks)
                .into_any_element(),
        )
    }
}
