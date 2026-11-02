//! The panel's standing search: its hits, the highlighted result and the keys typed while the field is open.
use super::{Panel, Press, Side};
use crate::g2::st1::{
    draw::{Line, Rows},
    view::SettingsView,
};
use flea::ui_logic_st1::{
    edit::{self, Edit},
    pages::SECTIONS,
    rows::{focusable, Kind, Row},
};
use gpui::{div, prelude::*, px, Context, Div, KeyDownEvent, MouseMoveEvent, Stateful, Window};

impl Panel {
    pub(super) fn search_selection(&self) -> Option<(usize, String)> {
        let (section, row) = self.hit_target()?;
        Some((
            section,
            self.everything_rows(section).get(row)?.id.to_string(),
        ))
    }

    pub(super) fn restore_search_selection(&mut self, selected: Option<(usize, String)>) {
        let Some((section, id)) = selected else {
            return;
        };
        for hit in 0..self.hit_count() {
            if let Some((found, row)) = self.target_at(hit) {
                if section == found
                    && self
                        .everything_rows(found)
                        .get(row)
                        .is_some_and(|row| row.id == id)
                {
                    self.hit = hit;
                    break;
                }
            }
        }
    }

    pub(in crate::g2::st1) fn query_changed(&mut self, query: String) {
        self.query = Some(query);
        self.hit = 0;
        self.results_scroll = 0.;
        self.rebuild();
    }

    pub(crate) fn searching(&self) -> bool {
        self.query.as_deref().is_some_and(|query| !query.is_empty())
    }

    /// A section's rows as the standing search read them.
    pub fn everything_rows(&self, section: usize) -> &[Row] {
        self.everything
            .iter()
            .find(|(index, _)| *index == section)
            .map_or(&[], |(_, rows)| rows)
    }

    pub(super) fn hit_count(&self) -> usize {
        self.results
            .iter()
            .map(|group| group.hits.len().max(1))
            .sum()
    }

    /// The section and row the highlighted result opens at.
    pub fn hit_target(&self) -> Option<(usize, usize)> {
        self.target_at(self.hit)
    }

    fn target_at(&self, mut left: usize) -> Option<(usize, usize)> {
        for group in &self.results {
            let span = group.hits.len().max(1);
            if left < span {
                let row = group.hits.get(left).map(|hit| hit.row).or(group.heading)?;
                return Some((group.section, row));
            }
            left -= span;
        }
        None
    }

    pub fn open_hit(&mut self) {
        let Some((section, row)) = self.hit_target() else {
            return;
        };
        self.query = None;
        self.results.clear();
        self.show_section(SECTIONS[section].id);
        self.side = Side::Pane;
        if row < self.rows.len() {
            self.cursor = row;
        }
        self.reveal();
    }

    /// A filtered control edits its original row against the same store as the unfiltered pane.
    pub(crate) fn press_hit(&mut self, hit: usize, target: Press) {
        let Some((section, index)) = self.target_at(hit) else {
            return;
        };
        let Some(row) = self.everything_rows(section).get(index) else {
            return;
        };
        let data = self.store.document();
        let change = match target {
            Press::Row => edit::activate(SECTIONS[section].id, row, data, self.facts.base_size),
            Press::Step => edit::step(row, data, self.facts.base_size, 1),
            Press::Pick(stop) => edit::pick(row, data, self.facts.base_size, stop),
            _ => return,
        };
        if matches!(change, Edit::Open(_)) {
            self.query = None;
            self.results.clear();
        }
        self.apply(change);
    }

    pub(crate) fn press_shown(&mut self, index: usize, hit: Option<usize>, target: Press) {
        if let Some(hit) = hit {
            self.press_hit(hit, target);
        } else {
            self.press(index, target);
        }
    }

    pub(super) fn search_key(&mut self, key: &str, text: &str) {
        let query = self.query.get_or_insert_with(String::new);
        match key {
            "escape" => {
                self.query = None;
                self.results.clear();
                return;
            }
            "backspace" if query.is_empty() => {
                self.query = None;
                return;
            }
            "backspace" => {
                query.pop();
            }
            "down" => self.hit = (self.hit + 1).min(self.hit_count().saturating_sub(1)),
            "up" => self.hit = self.hit.saturating_sub(1),
            "enter" => return self.open_hit(),
            _ if flea::ui_logic_st1::sheetkeys::is_printable(text) => query.push_str(text),
            _ => return,
        }
        if matches!(key, "up" | "down") {
            self.hit_moved = true;
            return;
        }
        self.hit = 0;
        self.results_scroll = 0.;
        self.rebuild();
    }
}

impl Rows<'_> {
    /// Result hover moves the highlight; its real controls retain their normal writers.
    pub(in crate::g2::st1) fn pointer(
        &self,
        base: Stateful<Div>,
        line: &Line<'_>,
        cx: &mut Context<SettingsView>,
    ) -> Stateful<Div> {
        let row = line.row;
        let clicks =
            matches!(
                row.kind,
                Kind::Check | Kind::Action | Kind::Group | Kind::Favourite
            ) || (row.kind == Kind::Choice && row.labels.is_empty() && row.values.is_empty());
        if let Some(hit) = line.hit {
            return base
                .on_mouse_move(cx.listener(move |view, _: &MouseMoveEvent, _, cx| {
                    if view.panel.hit != hit {
                        view.panel.hit = hit;
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(move |view, _, _, cx| {
                    if clicks {
                        view.panel.press_hit(hit, Press::Row);
                    } else {
                        view.panel.hit = hit;
                        view.panel.open_hit();
                    }
                    view.flush(cx);
                }));
        }
        let Some(index) = line.index.filter(|_| focusable(row)) else {
            return base;
        };
        base.cursor_pointer()
            .on_mouse_move(cx.listener(move |view, _: &MouseMoveEvent, _, cx| {
                if view.panel.cursor != index || view.panel.side != Side::Pane {
                    view.panel.pointer(index);
                    cx.notify();
                }
            }))
            .when(clicks, |base| {
                base.on_click(cx.listener(move |view, _, _, cx| {
                    view.panel.press(index, Press::Row);
                    view.flush(cx);
                }))
            })
    }
}

impl SettingsView {
    pub(in crate::g2::st1) fn search_footer<'a>(&'a self, lines: &[Line<'_>]) -> Option<Line<'a>> {
        if self.panel.save_error.is_none() || lines.iter().any(|line| line.row.footer) {
            return None;
        }
        self.panel
            .rows
            .iter()
            .find(|row| row.footer)
            .or_else(|| self.panel.everything_rows(0).iter().find(|row| row.footer))
            .map(|row| Line::result(row, None, None, false))
    }

    pub(in crate::g2::st1) fn search_clear(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        self.probe
            .attach(div(), "search-clear")
            .id("settings-search-clear")
            .absolute()
            .right_0()
            .top_0()
            .bottom_0()
            .w(px(30.))
            .cursor_pointer()
            .on_click(cx.listener(|view, _, _, cx| {
                view.editor.clear();
                view.panel.query = Some(String::new());
                view.panel.hit = 0;
                view.panel.results_scroll = 0.;
                view.panel.rebuild();
                view.flush(cx);
                cx.stop_propagation();
            }))
    }

    /// A plugin card keeps URL input; the standing search otherwise owns slash and its query keys.
    pub(in crate::g2::st1) fn search_capture(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stroke = &event.keystroke;
        let text = stroke.key_char.as_deref().unwrap_or("");
        let plain =
            !stroke.modifiers.control && !stroke.modifiers.alt && !stroke.modifiers.platform;
        let slash = plain
            && text == "/"
            && self.panel.section == "plugins"
            && self.plugins.as_ref().is_some_and(|plugin| {
                *plugin.read(cx).page().card() == crate::g2::st2::page::Card::None
            });
        if self.query_key(event, window, cx) {
            return;
        }
        if slash {
            self.panel.key(&stroke.key, text, stroke.modifiers.shift);
            window.focus(&self.focus, cx);
            self.flush(cx);
            cx.stop_propagation();
        }
    }
}
