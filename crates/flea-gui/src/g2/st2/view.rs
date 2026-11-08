//! The Plugins pane as GPUI draws it: Settings rows at the boards' metrics, the Add and Remove cards, keys and clicks.
use super::mark::{metrics as m, tracked, value as row_value, Mark};
use super::page::{Card, Key, PluginsPage};
use super::seam::{Effect, PluginsWire, Tokens};
use crate::g2::st1::input::{Editor, Host};
use flea::i18n::I18n;
use flea::ui_logic_st2::pages::{Row, UPDATE_ID};
use gpui::{
    div, prelude::*, px, Context, Div, EventEmitter, FocusHandle, Hsla, SharedString, Stateful,
    Window,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

#[cfg(test)]
use super::fact_tests;

pub struct PluginsPane {
    pub(super) editor: Editor,
    pub(super) input_focus: FocusHandle,
    pub(super) input_enabled: bool,
    pub(super) input_typeface: crate::list::Typeface,
    pub(super) page: PluginsPage,
    pub(super) wire: Rc<dyn PluginsWire>,
    pub(super) tokens: Tokens,
    pub(super) radius: f32,
    pub(super) marks: HashMap<String, Option<Arc<[u8]>>>,
    focus: FocusHandle,
    pub probe: crate::g2::st1::probe::Probe,
    /// How far the rows are scrolled up under the pane.
    pub scroll: f32,
}

impl EventEmitter<Effect> for PluginsPane {}

impl PluginsPane {
    /// radius is the compositor's corner radius in pixels, the boards' --r.
    pub fn new(
        i18n: Arc<I18n>,
        wire: Rc<dyn PluginsWire>,
        tokens: Tokens,
        radius: f32,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut page = PluginsPage::new(i18n);
        page.start(wire.as_ref());
        let focus = cx.focus_handle();
        Self {
            editor: Editor::default(),
            input_focus: focus.clone(),
            input_enabled: true,
            input_typeface: crate::list::Typeface::omarchy_default(),
            page,
            wire,
            tokens,
            radius,
            marks: HashMap::new(),
            focus,
            probe: Default::default(),
            scroll: 0.,
        }
    }

    pub fn page(&self) -> &PluginsPage {
        &self.page
    }

    /// Community marks read so far; a mark still loading or unreadable is not counted.
    pub fn loaded_marks(&self) -> usize {
        self.marks.values().filter(|m| m.is_some()).count()
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    pub fn set_tokens(&mut self, tokens: Tokens, radius: f32, cx: &mut Context<Self>) {
        self.tokens = tokens;
        self.radius = radius;
        cx.notify();
    }

    pub fn set_catalog(&mut self, i18n: Arc<I18n>, cx: &mut Context<Self>) {
        self.page.set_catalog(i18n);
        cx.notify();
    }

    /// One plugins or pluginop line from the backend.
    pub fn receive(&mut self, line: &str, cx: &mut Context<Self>) {
        if self.page.receive(line) {
            self.sync_editor();
            self.load_marks(cx);
            cx.notify();
        }
    }

    pub fn backend_failed(&mut self, cx: &mut Context<Self>) {
        self.page.failed();
        self.sync_editor();
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.page.start(self.wire.as_ref());
        cx.notify();
    }

    pub fn key(&mut self, key: Key, cx: &mut Context<Self>) {
        let title = self.page.title();
        let effect = self.page.key(key, self.wire.as_ref());
        self.sync_editor();
        self.keyed(&title);
        self.after(effect, cx);
    }

    fn after(&mut self, effect: Effect, cx: &mut Context<Self>) {
        let effect = match effect {
            Effect::None => self.page.take_effect(),
            effect => effect,
        };
        if effect != Effect::None {
            cx.emit(effect);
        }
        cx.notify();
    }

    fn checkbox(&self, on: bool, index: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = &self.tokens;
        let frame = div()
            .id(("st2-box", index))
            .size(px(m::BOX))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .border(px(m::BOX_BORDER))
            .rounded(px(self.radius / 2.))
            .border_color(if on { t.fg } else { t.muted_frame })
            .on_click(cx.listener(move |pane, _, _, cx| {
                cx.stop_propagation();
                pane.page.toggle(index, pane.wire.as_ref());
                cx.notify();
            }));
        if on {
            frame.bg(t.fg).child(Mark::tick(t.surface))
        } else {
            frame
        }
    }

    fn slot(&self, glyph: &str, size: f32, mark_path: &str) -> Div {
        let ink = self.tokens.muted_frame;
        let owned = self
            .marks
            .get(mark_path)
            .cloned()
            .flatten()
            .map(|data| Mark::owned(mark_path, data, size, ink));
        div()
            .w(px(m::SLOT))
            .h(px(m::BOX))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .children(owned.or_else(|| Mark::named(glyph, size, ink)))
    }

    fn heading(
        &self,
        label: &str,
        switch: Option<bool>,
        action: Option<&str>,
        index: usize,
        cursor: bool,
        cx: &mut Context<Self>,
    ) -> AnyRow {
        let (t, first) = (&self.tokens, index == 0);
        let heading = super::mark::heading(label, &self.probe, self.page.rows()[index].id());
        let mut row = self
            .probe
            .attach(div(), format!("row:{}", self.page.rows()[index].id()))
            .relative()
            .id(("st2-row", index))
            .flex()
            .items_center()
            .px(px(m::PAD_X))
            .pt(px(if first {
                m::FIRST_HEADING_TOP
            } else {
                m::HEADING_TOP
            }))
            .pb(px(m::HEADING_BOTTOM))
            .line_height(gpui::relative(1.2))
            .text_size(px(m::CAPTION))
            .text_color(t.muted)
            .child(heading)
            .child(div().flex_1());
        if !first {
            row = row
                .mt(px(m::HEADING_GAP))
                .border_t_1()
                .border_color(t.rule());
        }
        if let Some(on) = switch {
            let word = self.page.catalog().tr(if on {
                "plugins.switch.on"
            } else {
                "plugins.switch.off"
            });
            row = row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(m::GAP))
                    .h(px(14.))
                    .child(tracked(&word, m::TRACKING))
                    .child(self.checkbox(on, index, cx)),
            );
        }
        if let Some(action) = action {
            row = row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(m::ACTION_GAP))
                    .h(px(14.))
                    .text_color(t.fg)
                    .children(Mark::named("plus", m::ACTION_GLYPH, t.fg))
                    .child(SharedString::from(action.to_string())),
            );
        }
        self.interactive(row, index, cursor, switch.is_some() || action.is_some(), cx)
    }

    // Hover and the pane cursor both draw the boards' --hover fill; a click acts as Enter.
    fn interactive(
        &self,
        row: Stateful<Div>,
        index: usize,
        cursor: bool,
        live: bool,
        cx: &mut Context<Self>,
    ) -> AnyRow {
        if !live {
            return row;
        }
        let hover = self.tokens.hover();
        row.when(cursor, |row| row.bg(hover))
            .hover(move |style| style.bg(hover))
            .on_click(cx.listener(move |pane, _, _, cx| {
                let effect = pane.page.click(index, pane.wire.as_ref());
                pane.after(effect, cx);
            }))
    }

    fn line(&self, index: usize) -> Stateful<Div> {
        let row = self
            .probe
            .attach(div(), format!("row:{}", self.page.rows()[index].id()))
            .relative();
        #[cfg(test)]
        let row = fact_tests::measure(row, index, self.probe.clone());
        row.id(("st2-row", index))
            .h(px(m::ROW))
            .flex()
            .items_center()
            .gap(px(m::GAP))
            .px(px(m::PAD_X))
            .text_size(px(m::BODY))
            .whitespace_nowrap()
    }

    fn label(text: &str, ink: Hsla) -> Div {
        div()
            .flex_1()
            .min_w(px(0.))
            .overflow_hidden()
            .text_ellipsis()
            .text_color(ink)
            .child(SharedString::from(text.to_string()))
    }

    fn chevron(&self) -> Option<Mark> {
        let mirrored =
            crate::locale_geometry::Direction::from_locale(self.page.catalog().locale()).is_rtl();
        Mark::named(
            crate::locale_geometry::trailing_chevron(mirrored),
            m::CHEVRON,
            self.tokens.muted_frame,
        )
    }

    fn chips(
        &self,
        index: usize,
        values: &[String],
        labels: &[String],
        selected: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = self.tokens;
        let radius = self.radius;
        div().flex().flex_none().gap(px(m::CHIP_GAP)).children(
            values
                .iter()
                .zip(labels)
                .enumerate()
                .map(|(n, (value, label))| {
                    let on = value == selected;
                    let value = value.clone();
                    div()
                        .id(("st2-chip", index * 64 + n))
                        .h(px(m::CHIP))
                        .min_w(px(m::CHIP))
                        .px(px(m::CHIP_PAD))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(m::CAPTION))
                        .text_color(if on { t.fg } else { t.muted })
                        .border_1()
                        .border_color(if on { t.fg } else { t.frame() })
                        .rounded(px(radius))
                        .child(SharedString::from(label.clone()))
                        .on_click(cx.listener(move |pane, _, _, cx| {
                            cx.stop_propagation();
                            pane.page.pick(index, &value, pane.wire.as_ref());
                            pane.after(Effect::None, cx);
                        }))
                }),
        )
    }

    fn row(&self, index: usize, row: &Row, cursor: bool, cx: &mut Context<Self>) -> AnyRow {
        let t = self.tokens;
        let note = |text: &str, pad: &[f32]| {
            let mut d = div()
                .text_size(px(m::CAPTION))
                .line_height(gpui::relative(1.5))
                .text_color(t.muted)
                .pt(px(pad[0]))
                .pr(px(pad[1]))
                .pb(px(pad[2]));
            d = d.pl(px(*pad.get(3).unwrap_or(&pad[1])));
            d.id(("st2-row", index))
                .child(SharedString::from(text.to_string()))
        };
        match row {
            Row::Group {
                label,
                switch,
                action,
                ..
            } => self.heading(label, *switch, action.as_deref(), index, cursor, cx),
            Row::Rule => div()
                .id(("st2-row", index))
                .mt(px(m::RULE_ABOVE))
                .h(px(m::RULE_BELOW))
                .border_t_1()
                .border_color(t.rule()),
            Row::Description { text } => note(text, &m::DESCRIPTION),
            Row::Hint { text } => note(text, &m::HINT),
            Row::Plugin {
                name,
                glyph,
                mark,
                on,
                failure,
                ..
            } => {
                let line = self
                    .line(index)
                    .child(self.slot(glyph, m::MARK, mark))
                    .child(Self::label(name, t.fg))
                    .children(failure.as_deref().map(|word| row_value(word, t.error)))
                    .child(self.checkbox(*on, index, cx))
                    .children(self.chevron());
                self.interactive(line, index, cursor, true, cx)
            }
            Row::Check {
                label,
                glyph,
                on,
                available,
                ..
            } => {
                let line = self
                    .line(index)
                    .child(self.slot(glyph, m::ROW_GLYPH, ""))
                    .child(Self::label(label, t.fg))
                    .child(self.checkbox(*on, index, cx));
                self.interactive(line, index, cursor, *available, cx)
                    .when(!available, |row| row.opacity(m::DISABLED))
            }
            Row::Choice {
                label,
                glyph,
                values,
                labels,
                selected,
                available,
                ..
            } => {
                let line = self
                    .line(index)
                    .child(self.slot(glyph, m::ROW_GLYPH, ""))
                    .child(Self::label(label, t.fg))
                    .child(self.chips(index, values, labels, selected, cx));
                self.interactive(line, index, cursor, *available, cx)
                    .when(!available, |row| row.opacity(m::DISABLED))
            }
            Row::Number {
                label,
                value,
                unit,
                available,
                ..
            } => {
                let count = self.page.catalog().locale().format_count(*value);
                let shown = if unit.is_empty() {
                    count
                } else {
                    self.page
                        .catalog()
                        .tr_args("plugins.value.unit", &[&count, unit])
                };
                let line = self
                    .line(index)
                    .child(self.slot("", m::ROW_GLYPH, ""))
                    .child(Self::label(label, t.fg))
                    .child(row_value(&shown, t.fg));
                self.interactive(line, index, cursor, *available, cx)
                    .when(!available, |row| row.opacity(m::DISABLED))
            }
            Row::Field {
                label,
                value,
                placeholder,
                available,
                ..
            } => {
                let (shown, ink) = if value.is_empty() {
                    (placeholder, t.muted)
                } else {
                    (value, t.fg)
                };
                let line = self
                    .line(index)
                    .child(self.slot("", m::ROW_GLYPH, ""))
                    .child(Self::label(label, t.fg))
                    .child(row_value(shown, ink));
                self.interactive(line, index, cursor, *available, cx)
                    .when(!available, |row| row.opacity(m::DISABLED))
            }
            Row::Fact { text, value } => self
                .line(index)
                .child(self.slot("", m::ROW_GLYPH, ""))
                .child(
                    Self::label(text, t.muted).when(!value.is_empty(), |label| label.flex_none()),
                )
                .when(!value.is_empty(), |line| {
                    line.child(
                        row_value(value, t.muted)
                            .flex_1()
                            .min_w_0()
                            .when(self.page.facts_right(), |value| value.text_right())
                            .text_ellipsis_middle(),
                    )
                }),
            Row::Action {
                id,
                label,
                glyph,
                value,
                chevron,
            } => {
                let ink = if id == UPDATE_ID { t.fg } else { t.muted };
                let line = self
                    .line(index)
                    .child(self.slot(glyph, m::ROW_GLYPH, ""))
                    .child(Self::label(label, t.fg))
                    .when(!value.is_empty(), |line| line.child(row_value(value, ink)))
                    .when(*chevron, |line| line.children(self.chevron()));
                self.interactive(line, index, cursor, true, cx)
            }
            Row::Favourite {
                label,
                glyph,
                error,
                available,
                ..
            } => {
                let line = self
                    .line(index)
                    .child(self.slot(glyph, m::ROW_GLYPH, ""))
                    .child(Self::label(label, t.fg))
                    .when(!error.is_empty(), |line| {
                        line.child(row_value(error, t.error))
                    });
                self.interactive(line, index, cursor, true, cx)
                    .when(!available, |row| row.opacity(m::DISABLED))
            }
        }
    }
}

type AnyRow = Stateful<Div>;

impl Render for PluginsPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_editor();
        self.laid_out(window, cx);
        let cursor = if self.focus.is_focused(window) || *self.page.card() != Card::None {
            self.page.cursor()
        } else {
            None
        };
        let rows: Vec<Row> = self.page.rows().to_vec();
        let drawn: Vec<AnyRow> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| self.row(index, row, cursor == Some(index), cx))
            .collect();
        let rows = self.probe.attach(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(-self.scroll))
                .flex()
                .flex_col()
                .pb(px(8.))
                .children(drawn),
            "rows",
        );
        self.probe
            .attach(div(), "pane")
            .id("st2-plugins")
            .size_full()
            .relative()
            .overflow_hidden()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_scroll_wheel(cx.listener(Self::wheel))
            .child(rows)
    }
}
