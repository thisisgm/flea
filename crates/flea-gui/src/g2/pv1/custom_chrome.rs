//! Window chrome and a bounded filmstrip. All strokes remain inside their cards.
use super::{bar, glyph::Glyph};
use super::{custom::Custom, Event, QuickLook};
use gpui::{
    div, img, prelude::*, px, AnyElement, Context, Hsla, MouseButton, ObjectFit, SharedString,
    Window,
};

fn button(label: &str, action: &'static str, cx: &mut Context<QuickLook>) -> AnyElement {
    div()
        .id(SharedString::from(format!("preview-{action}")))
        .px(px(9.))
        .h(px(30.))
        .flex()
        .items_center()
        .cursor_pointer()
        .debug_selector(|| format!("preview-{action}"))
        .child(SharedString::from(label.to_owned()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, _, cx| {
                cx.stop_propagation();
                view.custom_action(action, cx);
            }),
        )
        .into_any_element()
}

fn icon_button(
    glyph: Glyph,
    label: &'static str,
    action: &'static str,
    ink: Hsla,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    div()
        .id(SharedString::from(format!("preview-{action}")))
        .w(px(34.))
        .h(px(30.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .debug_selector(|| format!("preview-{action}"))
        .aria_label(label)
        .child(bar::mark(glyph, 16., ink))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, _, cx| {
                cx.stop_propagation();
                view.custom_action(action, cx);
            }),
        )
        .into_any_element()
}

fn sheet_tab(
    index: usize,
    name: &str,
    active: bool,
    theme: crate::palette::Theme,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    div()
        .id(SharedString::from(format!("preview-sheet-{index}")))
        .max_w(px(88.))
        .h(px(26.))
        .px(px(7.))
        .flex_none()
        .flex()
        .items_center()
        .overflow_hidden()
        .cursor_pointer()
        .border_b_2()
        .border_color(if active {
            theme.foreground
        } else {
            theme.foreground.opacity(0.15)
        })
        .text_color(if active {
            theme.foreground
        } else {
            theme.foreground.opacity(0.7)
        })
        .text_size(px(12.))
        .child(SharedString::from(name.to_owned()))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, _, cx| {
                cx.stop_propagation();
                if let Some(doc) = view.subject.as_ref().and_then(|s| s.body.doc()).cloned() {
                    doc.word(index, cx);
                    cx.notify();
                }
            }),
        )
        .into_any_element()
}

fn sheet_strip(
    sheets: &[(String, bool)],
    theme: crate::palette::Theme,
    cx: &mut Context<QuickLook>,
) -> AnyElement {
    let active = sheets.iter().position(|(_, on)| *on).unwrap_or(0);
    let count = sheets.len().min(3);
    let start = active
        .saturating_sub(1)
        .min(sheets.len().saturating_sub(count));
    let end = (start + count).min(sheets.len());
    let mut strip = div()
        .id("preview-sheet-strip")
        .max_w(px(300.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(2.))
        .overflow_hidden();
    if start > 0 {
        strip = strip.child(button("‹", "previousSheet", cx));
    }
    for (index, (name, on)) in sheets.iter().enumerate().take(end).skip(start) {
        strip = strip.child(sheet_tab(index, name, *on, theme, cx));
    }
    if end < sheets.len() {
        strip = strip.child(button("›", "nextSheet", cx));
    }
    strip.into_any_element()
}

/// Whole cards only, centered on the selection and clamped at both ends.
pub(super) fn band(total: usize, current: usize, width: f32) -> std::ops::Range<usize> {
    let count = (((width - 24.) / 92.).floor() as usize).max(1).min(total);
    let start = current
        .saturating_sub(count / 2)
        .min(total.saturating_sub(count));
    start..start + count
}

impl QuickLook {
    pub(super) fn custom_chrome(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let mut result = Vec::new();
        self.custom_thumbnails(window, cx);
        self.custom_preload(window, cx);
        let page_counter = self
            .subject
            .as_ref()
            .and_then(|subject| subject.body.doc())
            .and_then(|doc| doc.pages(cx))
            .and_then(|pdf| {
                let pdf = pdf.read(cx);
                let count = pdf.surface.page_count();
                (count > 0).then(|| {
                    self.i18n.tr_args(
                        "document.pdf_page_count",
                        &[
                            &(pdf.surface.drawn_page() + 1).to_string(),
                            &count.to_string(),
                        ],
                    )
                })
            });
        let office_sheets = self
            .subject
            .as_ref()
            .and_then(|subject| subject.body.doc())
            .and_then(|doc| match &doc.view {
                super::doc::View::Office(office) => {
                    Some(office.read(cx).bar(&self.i18n, cx).sheets)
                }
                _ => None,
            });
        let Some(custom) = &self.custom else {
            return result;
        };
        if custom.navbar {
            let name = self
                .subject
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_default();
            result.push(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top_0()
                    .h(px(36.))
                    .bg(theme.background)
                    .border_b_1()
                    .border_color(theme.foreground.opacity(0.15))
                    .flex()
                    .items_center()
                    .text_size(px(13.))
                    .child(button("‹", "previousFile", cx))
                    .child(button("›", "nextFile", cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(name),
                    )
                    .when_some(
                        office_sheets.filter(|sheets| !sheets.is_empty()),
                        |bar, sheets| bar.child(sheet_strip(&sheets, theme, cx)),
                    )
                    .when_some(page_counter, |bar, counter| {
                        bar.child(icon_button(
                            Glyph::ChevronLeft,
                            "Previous page",
                            "previousPage",
                            theme.foreground,
                            cx,
                        ))
                        .child(
                            div()
                                .id("preview-page-counter")
                                .flex_none()
                                .text_color(theme.foreground)
                                .child(counter),
                        )
                        .child(icon_button(
                            Glyph::ChevronRight,
                            "Next page",
                            "nextPage",
                            theme.foreground,
                            cx,
                        ))
                    })
                    .child(icon_button(
                        Glyph::Info,
                        "Info",
                        "toggleInfo",
                        theme.foreground,
                        cx,
                    ))
                    .child(icon_button(
                        Glyph::Filmstrip,
                        "Filmstrip",
                        "toggleFilmstrip",
                        theme.foreground,
                        cx,
                    ))
                    .child(icon_button(
                        Glyph::Maximize,
                        "Fullscreen",
                        "fullscreen",
                        theme.foreground,
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        if custom.filmstrip_visible() {
            let info_width = if custom.info {
                (f32::from(window.viewport_size().width) * 0.3).clamp(180., 300.)
            } else {
                0.
            };
            let room = f32::from(window.viewport_size().width) - info_width;
            let range = band(custom.items.len(), custom.current, room);
            let cards = range
                .map(|index| {
                    let item = &custom.items[index];
                    let active = index == custom.current;
                    let border = if active {
                        theme.foreground
                    } else {
                        theme.foreground.opacity(0.18)
                    };
                    let mut card = div()
                        .id(SharedString::from(format!("filmstrip-{index}")))
                        .w(px(84.))
                        .h(px(64.))
                        .flex_none()
                        .border_1()
                        .border_color(border)
                        .rounded(px(4.))
                        .p(px(3.))
                        .overflow_hidden()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center();
                    if let Some(Some(image)) = custom.thumbnails.get(&item.path) {
                        card = card.child(
                            img(image.clone())
                                .h(px(56.))
                                .w(px(76.))
                                .object_fit(ObjectFit::Contain),
                        );
                    } else if custom.thumbnails.contains_key(&item.path) {
                        card = card.child(div().h(px(56.)));
                    } else {
                        card = card.child(
                            div()
                                .h(px(56.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(bar::mark(
                                    Glyph::kind(&item.row.icon, item.row.mode),
                                    28.,
                                    theme.foreground,
                                )),
                        );
                    }
                    card.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_, _, _, cx| {
                            cx.stop_propagation();
                            cx.emit(Event::Navigate(index));
                        }),
                    )
                    .into_any_element()
                })
                .collect::<Vec<_>>();
            result.push(
                div()
                    .absolute()
                    .left_0()
                    .right(px(info_width))
                    .bottom_0()
                    .h(px(92.))
                    .bg(theme.background)
                    .border_t_1()
                    .border_color(theme.foreground.opacity(0.15))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .px(px(12.))
                    .overflow_hidden()
                    .children(cards)
                    .into_any_element(),
            );
        }
        if custom.info {
            result.push(info(custom, window, theme).into_any_element());
        }
        result
    }
}

fn info(custom: &Custom, window: &Window, theme: crate::palette::Theme) -> impl IntoElement {
    let width = (f32::from(window.viewport_size().width) * 0.3).clamp(180., 300.);
    let mut panel = div()
        .id("preview-info-panel")
        .overflow_y_scroll()
        .absolute()
        .right_0()
        .top(px(36.))
        .bottom(px(if custom.filmstrip_visible() { 92. } else { 0. }))
        .w(px(width))
        .p(px(14.))
        .bg(theme.background)
        .border_l_1()
        .border_color(theme.foreground.opacity(0.15))
        .flex()
        .flex_col()
        .gap(px(10.))
        .text_size(px(12.));
    let Some(item) = custom.items.get(custom.current) else {
        return panel;
    };
    let mut facts = vec![
        ("Name", item.row.name.clone()),
        ("Path", item.path.clone()),
        ("Size", format!("{} bytes", item.row.size)),
        ("Mode", format!("{:o}", item.row.mode & 0o7777)),
    ];
    if let Some(meta) = &custom.metadata {
        if meta.w > 0 {
            facts.push(("Dimensions", format!("{} × {}", meta.w, meta.h)));
        }
        if meta.duration_ms > 0 {
            facts.push((
                "Duration",
                format!("{:.1} s", meta.duration_ms as f64 / 1000.),
            ));
        }
        if meta.sample_rate > 0. {
            facts.push(("Sample rate", format!("{} Hz", meta.sample_rate)));
        }
        if !meta.owner.is_empty() {
            facts.push(("Owner", meta.owner.clone()));
        }
    }
    if custom.fps > 0. {
        facts.push(("Frame rate", format!("{:.2} fps", custom.fps)));
    }
    if custom.bitrate > 0 {
        facts.push(("Bit rate", format!("{} kb/s", custom.bitrate / 1000)));
    }
    for (label, value) in facts {
        panel = panel.child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(div().text_color(theme.foreground.opacity(0.6)).child(label))
                .child(div().overflow_hidden().text_ellipsis().child(value)),
        );
    }
    panel
}
