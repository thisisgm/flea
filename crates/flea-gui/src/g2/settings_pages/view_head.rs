use super::{
    geometry::{self as g, PAD_X},
    marks::{mark, washed},
    seams::{Ink, Request},
    view::SettingsView,
};
use crate::g2::status_feedback::caption::caseless;
use flea::ui_logic_settings_pages::{
    caps,
    rows::{Kind, Row},
};
use gpui::{
    div, prelude::*, px, Context, Div, Font, FontWeight, MouseButton, Pixels, SharedString,
    TextRun, Window,
};

impl SettingsView {
    /// The head: the title's sliders mark in its own ink, the pane's name, esc and the close mark.
    pub(super) fn head(&self, ink: &Ink, cx: &mut Context<Self>) -> Div {
        let s = self.panel.scale;
        div()
            .h(px(s.head))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(s.of(g::GAP)))
            .px(px(s.of(PAD_X)))
            .border_b_1()
            .border_color(ink.rule)
            .text_size(self.typeface.caption)
            .text_color(ink.foreground)
            .child(mark("sliders", s.of(16.), ink.foreground))
            .child(
                // The probe boxes the title text itself, so its centre is the drawn title's, not the head row's.
                div().flex_1().flex().child(
                    self.probe.attach(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::BOLD)
                            .whitespace_nowrap()
                            .child(SharedString::from(
                                self.plugins
                                    .as_ref()
                                    .filter(|_| {
                                        self.panel.section == "plugins" && !self.panel.searching()
                                    })
                                    .map_or_else(
                                        || self.panel.title(),
                                        |plugin| plugin.read(cx).page().title(),
                                    ),
                            )),
                        "title",
                    ),
                ),
            )
            .child(div().text_color(ink.muted).child("esc"))
            .child(
                div()
                    .id("settings-close")
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(mark("x", s.of(16.), ink.foreground))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.panel.requests.push(Request::Close);
                        view.flush(cx);
                    })),
            )
    }

    /// A heading's letter spacing: the boards' 0.14em, or none in a caseless language, whose tracking would cut its script apart.
    pub(super) fn heading_spacing(&self) -> f32 {
        if caseless(self.panel.catalog.language()) {
            0.
        } else {
            f32::from(self.typeface.caption) * 0.14
        }
    }

    /// A heading in the capitals its language writes, spaced as the boards space it; a typed run keeps its wash.
    pub(super) fn heading(
        &self,
        label: &str,
        run: Option<(usize, usize)>,
        lifted: bool,
        ink: &Ink,
    ) -> Div {
        let (text, run) = heading_text(label, run, self.panel.catalog.language());
        washed(&text, run, lifted, ink.wash14, self.heading_spacing())
    }
}

/// A heading as its language writes it, with its typed run in characters: capitals, or in a caseless language the plain string, even where a string falls back to English.
pub(super) fn heading_text(
    label: &str,
    run: Option<(usize, usize)>,
    language: &str,
) -> (String, Option<(usize, usize)>) {
    if caseless(language) {
        return (label.to_owned(), run);
    }
    let upper = |text: &str| caps::heading(text, language);
    let Some((start, length)) = run else {
        return (upper(label), None);
    };
    // Capitals can change a run's length, as ß does, so each part is set in capitals on its own.
    let at = |chars: usize| {
        label
            .char_indices()
            .nth(chars)
            .map_or(label.len(), |(byte, _)| byte)
    };
    let (from, to) = (at(start), at(start + length));
    let (head, piece, tail) = (
        upper(&label[..from]),
        upper(&label[from..to]),
        upper(&label[to..]),
    );
    let run = (head.chars().count(), piece.chars().count());
    (format!("{head}{piece}{tail}"), Some(run))
}

impl SettingsView {
    /// One line of `text` at `size` in the pane's face, as shaped widths decide every fit.
    pub(super) fn shaped(&self, text: String, size: Pixels, window: &Window) -> f32 {
        shaped_in(&self.typeface.font, text, size, window)
    }

    /// A control row's room, with its label's longest word kept whole even in the bold a lifted search run draws.
    pub(super) fn room(&self, row: &Row, segment: bool, width: f32, window: &Window) -> g::Room {
        let catalog = &self.panel.catalog;
        let word = self.longest_word(&row.label.resolve(catalog), window);
        g::control_room(
            row,
            segment,
            width,
            self.panel.scale,
            word,
            |text| self.shaped(text.resolve(catalog), self.typeface.body, window),
            |text| self.shaped(text.resolve(catalog), self.typeface.caption, window),
        )
    }

    /// The label's longest unbroken piece, shaped in the bold a lifted search run draws.
    pub(super) fn longest_word(&self, label: &str, window: &Window) -> f32 {
        let bold = Font {
            weight: FontWeight::BOLD,
            ..self.typeface.font.clone()
        };
        super::wrap::unbroken(label)
            .into_iter()
            .map(|word| shaped_in(&bold, word.to_owned(), self.typeface.body, window))
            .fold(0., f32::max)
    }

    /// A control row's label lines: one, or as GPUI wraps it in the room its control leaves; a room narrower than its longest word, or a wrap past LABEL_LINES, keeps one elided line, since a wrap there stacks a word or a letter a line; `bold` is a search hit's run in characters, shaped in the bold the lifted hit draws.
    pub(super) fn label_lines(
        &self,
        row: &Row,
        bold: Option<(usize, usize)>,
        width: f32,
        window: &Window,
    ) -> usize {
        let segment = row.kind == Kind::Choice && self.fits(row, width, window);
        let room = self.room(row, segment, width, window).label;
        let label = row.label.resolve(&self.panel.catalog);
        if room < self.longest_word(&label, window) {
            return 1;
        }
        let at = |chars: usize| {
            label
                .char_indices()
                .nth(chars)
                .map_or(label.len(), |(byte, _)| byte)
        };
        let (from, to) = bold.map_or((0, 0), |(start, length)| (at(start), at(start + length)));
        let run = |len: usize, weight: FontWeight| TextRun {
            len,
            font: Font {
                weight,
                ..self.typeface.font.clone()
            },
            color: Default::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs: Vec<TextRun> = [
            run(from, self.typeface.font.weight),
            run(to - from, FontWeight::BOLD),
            run(label.len() - to, self.typeface.font.weight),
        ]
        .into_iter()
        .filter(|run| run.len > 0)
        .collect();
        let one = window.text_system().shape_line(
            SharedString::from(label.clone()),
            self.typeface.body,
            &runs,
            None,
        );
        if f32::from(one.width) <= room {
            return 1;
        }
        window
            .text_system()
            .shape_text(
                label.into(),
                self.typeface.body,
                &runs,
                Some(px(room.max(1.))),
                None,
            )
            .map(|lines| {
                let count = lines
                    .iter()
                    .map(|line| line.wrap_boundaries().len() + 1)
                    .sum::<usize>();
                if count > g::LABEL_LINES {
                    1
                } else {
                    count
                }
            })
            .unwrap_or_else(|error| {
                // A label that cannot be shaped is measured as one line, and the log names the row and the cause.
                eprintln!(
                    "flea-gui: Settings row {} label did not shape at {room}px: {error}",
                    row.id
                );
                1
            })
    }
}

impl SettingsView {
    /// Plugin dialogs center over the Settings window and retain the plugin keyboard owner.
    pub(super) fn plugin_card(&self, cx: &gpui::App) -> Option<Div> {
        self.plugins
            .as_ref()
            .filter(|_| self.panel.section == "plugins" && !self.panel.searching())
            .and_then(|plugin| plugin.read(cx).card(plugin.clone()))
            .map(|card| {
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(card)
            })
    }
}

/// One line of `text` at `size` in `font`.
fn shaped_in(font: &Font, text: String, size: Pixels, window: &Window) -> f32 {
    let text = SharedString::from(text);
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: Default::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    f32::from(
        window
            .text_system()
            .shape_line(text, size, &[run], None)
            .width,
    )
}
