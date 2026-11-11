//! The board card bodies; the shared operation grammar supplies every frame and ink.
use super::{
    controls::{self as c, Control},
    host::OutputView,
    model::{Card, Focus},
};
use crate::g2::file_ops::glyph::Glyph;
use flea::ui_logic_print_convert::output::Kind;
use gpui::{div, prelude::*, px, relative, AnyElement, Context, Div, FontWeight};

fn rule(view: &OutputView, bleed: bool) -> Div {
    div()
        .flex_none()
        .h(px(1.))
        .mx(px(if bleed { -16. } else { 0. }))
        .bg(view.palette.muted_frame.opacity(0.4))
}

fn output(view: &OutputView, text: String) -> Div {
    view.observed
        .probe
        .attach(div(), "output")
        .flex()
        .flex_row()
        .mt(px(3.))
        .text_size(px(12.))
        .line_height(px(OUTPUT_LINE_HEIGHT))
        .child(
            div()
                .flex_none()
                .text_color(view.palette.muted)
                .child(view.catalog.tr("dialogs.output.output")),
        )
        .child(
            div()
                .min_w_0()
                .text_color(view.palette.foreground)
                .child(text),
        )
}

pub fn card(view: &OutputView, room: (f32, f32), cx: &mut Context<OutputView>) -> Option<Div> {
    let p = &view.palette;
    let model = &view.model;
    let card = model.card.as_ref()?;
    let width: f32 = match card {
        Card::Convert(_) => 394.,
        Card::Print(s) if s.queried && s.destinations.is_empty() => 448.,
        _ => 552.,
    };
    let bleed = matches!(card, Card::Convert(s) if s.kind != Kind::Office);
    let title = view
        .observed
        .title(div())
        .debug_selector(|| "op4-title".into())
        .flex()
        .items_center()
        .gap(px(9.))
        .line_height(relative(1.2))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(16.))
                .font_weight(FontWeight::BOLD)
                .child(model.title(&view.catalog)),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(12.))
                .text_color(p.muted)
                .child(view.catalog.tr("dialogs.output.esc")),
        );
    let body = match card {
        Card::Convert(s) => convert(view, s, cx),
        Card::Print(s) => super::forms::print(view, s, cx),
        Card::Combine(s) => super::forms::combine(view, s, cx),
    };
    let label = match card {
        Card::Print(s) if s.queried && s.destinations.is_empty() => "setup",
        Card::Print(_) => "print",
        Card::Convert(_) => "convert",
        Card::Combine(_) => "combine",
    };
    let buttons = div()
        .debug_selector(|| "op4-buttons".into())
        .flex()
        .justify_end()
        .gap(px(9.))
        .mt(px(10.))
        .child(c::button(
            view,
            Control::Cancel,
            view.catalog.tr("dialogs.output.cancel"),
            false,
            model.focus == Some(Focus::Cancel),
            model.can_cancel(),
            cx,
        ))
        .child(c::button(
            view,
            Control::Submit,
            view.catalog.tr(&format!("dialogs.output.{label}")),
            true,
            model.focus == Some(Focus::Submit),
            model.can_submit(),
            cx,
        ));
    let max_body = flea::ui_logic_file_ops::card::frame(19.2, f32::MAX, room.1).body;
    Some(
        view.observed
            .card(div())
            .debug_selector(|| "op4-card".into())
            .w(px(width.min((room.0 - 32.).max(0.))))
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .bg(p.surface)
            .border_1()
            .border_color(p.muted_frame)
            .rounded(px(p.radius))
            .text_size(px(14.))
            .text_color(p.foreground)
            .child(title)
            .child(rule(view, bleed).debug_selector(|| "op4-rule".into()))
            .child(
                view.body_scroll.body(
                    view.observed
                        .probe
                        .attach(div(), "body")
                        .id("op4-body")
                        .debug_selector(|| "op4-body".into())
                        .flex()
                        .flex_col()
                        .mx(px(-16.))
                        .px(px(16.))
                        .gap(px(12.))
                        .max_h(px(max_body))
                        .overflow_y_scroll()
                        .track_scroll(&view.body_scroll.handle)
                        .children(body),
                ),
            )
            .child(buttons)
            .when(matches!(card, Card::Convert(_)), |element| {
                view.observed.retain_convert(element)
            }),
    )
}

fn convert(
    view: &OutputView,
    s: &super::convert::Convert,
    cx: &mut Context<OutputView>,
) -> Vec<AnyElement> {
    let p = &view.palette;
    let mut parts = vec![];
    if s.kind != Kind::Office {
        let mut formats = div().flex().flex_col().mx(px(-16.));
        for (i, format) in s.kind.formats().iter().enumerate() {
            let selected = *format == s.format;
            let focused = view.model.focus == Some(Focus::Formats);
            let (name, data) = if *format == "pdf" { FILE } else { IMAGE };
            let mut row = div()
                .debug_selector(move || format!("review-op4-format-{i}"))
                .relative()
                .h(px(37.))
                .flex_none()
                .flex()
                .items_center()
                .px(px(14.))
                .gap(px(9.))
                .mt(px(if i == 0 {
                    0.
                } else {
                    [5., 5., 4., 5., 4., 5., 5.][i - 1]
                }))
                .opacity(if view.model.editable() { 1. } else { 0.55 })
                .child(Glyph::svg(
                    (name, data),
                    18.,
                    if selected {
                        p.foreground
                    } else {
                        p.muted_frame
                    },
                ))
                .child(view.catalog.tr(&format!("dialogs.output.choice.{format}")));
            if (focused && s.cursor == i) || (!focused && selected) {
                row = row.bg(if focused {
                    p.accent.opacity(0.18)
                } else {
                    p.foreground.opacity(0.08)
                });
                if focused {
                    row = row.child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(px(2.))
                            .bg(p.accent_frame),
                    );
                }
            }
            let row = row.on_mouse_move(cx.listener(
                move |view, event: &gpui::MouseMoveEvent, _, cx| {
                    if view.model.editable() && view.pointer_position != Some(event.position) {
                        view.pointer_position = Some(event.position);
                        if let Some(Card::Convert(c)) = &mut view.model.card {
                            c.cursor = i;
                        }
                        view.model.focus = Some(Focus::Formats);
                        cx.notify();
                    }
                },
            ));
            formats = formats.child(c::wire(
                view,
                Control::Choice(Focus::Formats, i),
                view.model.editable(),
                row,
                cx,
            ));
        }
        parts.push(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(formats)
                .child(rule(view, true))
                .into_any_element(),
        );
    }
    match s.kind {
        Kind::Image => {
            let combine = s.format == "pdf"
                && view
                    .model
                    .selection
                    .as_ref()
                    .is_some_and(|s| s.files.len() > 1);
            parts.push(
                c::check(
                    view,
                    Focus::Check,
                    view.catalog.tr(if combine {
                        "dialogs.output.combine_pdf"
                    } else {
                        "dialogs.output.metadata"
                    }),
                    if combine { s.combine } else { s.strip },
                    view.model.editable() && (s.format != "pdf" || combine),
                    cx,
                )
                .mt(px(2.))
                .into_any_element(),
            );
        }
        Kind::Pdf => parts.push(
            c::named_enabled(
                p,
                view.catalog.tr("dialogs.output.pages"),
                c::field(
                    view,
                    Focus::Pages,
                    s.pages.clone(),
                    view.catalog.tr("dialogs.output.all"),
                    view.model.editable(),
                    cx,
                ),
                view.model.editable(),
            )
            .into_any_element(),
        ),
        _ => (),
    }
    parts.push(output(view, convert_path(view, s)).into_any_element());
    parts
}

pub(super) fn convert_path(view: &OutputView, s: &super::convert::Convert) -> String {
    if s.kind == Kind::Pdf && s.outputs.len() > 1 {
        let first = s.outputs.first().unwrap();
        let last = s.outputs.last().unwrap().rsplit('/').next().unwrap_or("");
        view.catalog
            .tr_args("dialogs.output.output_range", &[first, last])
    } else {
        s.outputs.first().cloned().unwrap_or_default()
    }
}

const IMAGE: (&str, &[u8]) = ("op4-image", br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M3 3h18v18H3z"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="m21 15-5-5L5 21"/></svg>"#);
const FILE: (&str, &[u8]) = ("op4-file", br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M14 2H4v20h16V8zM14 2v6h6M8 13h8M8 17h8M8 9h2"/></svg>"#);
// The output line's own line height, which the Convert reader divides the painted box by.
pub(super) const OUTPUT_LINE_HEIGHT: f32 = 18.;
pub(super) const CHEVRON: (&str, &[u8]) = ("op4-chevron", br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="m6 9 6 6 6-6"/></svg>"#);
