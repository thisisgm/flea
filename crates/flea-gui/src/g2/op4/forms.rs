//! Printer fields and the three-line Combine form from the approved boards.
use super::{
    controls::{self as c, Control},
    host::OutputView,
    model::Focus,
    preview::Preview,
    view::CHEVRON,
};
use crate::g2::op1::glyph::Glyph;
use flea::ui_logic_op4::{
    composition::{Layout, Spacing},
    output,
};
use gpui::{div, prelude::*, px, AnyElement, Context};
pub(super) fn print(
    view: &OutputView,
    s: &super::printing::Print,
    cx: &mut Context<OutputView>,
) -> Vec<AnyElement> {
    let p = &view.palette;
    if s.queried && s.destinations.is_empty() {
        return vec![div()
            .line_height(px(20.))
            .child(view.catalog.tr("dialogs.output.no_printer"))
            .into_any_element()];
    }
    let ready = view.model.editable() && s.pending.is_none() && s.queried;
    let bounds = view.printer_menu.bounds.clone();
    let printer = c::wire(
        view,
        Control::Field(Focus::Printer),
        ready,
        div()
            .relative()
            .debug_selector(|| "op4-printer".into())
            .h(px(30.))
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(8.))
            .bg(p.background)
            .opacity(if ready { 1. } else { 0.55 })
            .border_1()
            .border_color(if view.model.focus == Some(Focus::Printer) {
                p.accent_frame
            } else {
                p.muted_frame
            })
            .rounded(px(p.radius))
            .child(div().flex_1().child(s.printer_name.clone()))
            .child(
                div().text_size(px(12.)).text_color(p.muted).child(
                    if s.destinations
                        .iter()
                        .any(|d| d.name == s.printer && d.default)
                    {
                        view.catalog.tr("dialogs.output.default")
                    } else {
                        String::new()
                    },
                ),
            )
            .child(Glyph::svg(CHEVRON, 12., p.muted_frame))
            .child(
                gpui::canvas(move |area, _, _| bounds.set(Some(area)), |_, _, _, _| {})
                    .absolute()
                    .left(px(0.))
                    .top(px(0.))
                    .size_full(),
            ),
        cx,
    );
    let copies = c::named_enabled(
        p,
        view.catalog.tr("dialogs.output.copies"),
        c::field(
            view,
            Focus::Copies,
            s.copies.clone(),
            String::new(),
            ready,
            cx,
        ),
        ready,
    );
    let mut paired = div().flex().gap(px(12.)).child(copies.flex_1());
    let paged = view
        .model
        .selection
        .as_ref()
        .is_some_and(|s| s.files.iter().any(|f| f.kind.paged()));
    if paged {
        paired = paired.child(
            c::named_enabled(
                p,
                view.catalog.tr("dialogs.output.pages"),
                c::field(
                    view,
                    Focus::Pages,
                    s.pages.clone(),
                    view.catalog.tr("dialogs.output.all"),
                    ready,
                    cx,
                ),
                ready,
            )
            .flex_1(),
        );
    }
    let checks = div()
        .flex()
        .gap(px(12.))
        .child(
            c::check(
                view,
                Focus::Duplex,
                view.catalog.tr("dialogs.output.two_sided"),
                s.two_sided,
                ready && s.capabilities.duplex,
                cx,
            )
            .flex_1(),
        )
        .child(
            c::check(
                view,
                Focus::Grayscale,
                view.catalog.tr("dialogs.output.grayscale"),
                s.grayscale,
                ready && s.capabilities.color,
                cx,
            )
            .flex_1(),
        );
    vec![div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(c::named_enabled(
            p,
            view.catalog.tr("dialogs.output.printer"),
            printer,
            ready,
        ))
        .child(paired)
        .child(checks)
        .into_any_element()]
}

pub(super) fn combine(
    view: &OutputView,
    s: &super::combine::Combine,
    cx: &mut Context<OutputView>,
) -> Vec<AnyElement> {
    let p = &view.palette;
    let enabled = view.model.editable();
    let count = view.model.selection.as_ref().map_or(0, |s| s.files.len());
    let preview = view.body_scroll.observe(
        Control::Field(Focus::Preview),
        div()
            .debug_selector(|| "op4-preview".into())
            .relative()
            .h(px(280.))
            .flex_none()
            .border_1()
            .border_color(p.muted_frame)
            .rounded(px(p.radius))
            .bg(p.background)
            .overflow_hidden()
            .child(Preview::new(view))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|view, event: &gpui::MouseDownEvent, _, cx| {
                    view.pointer_image(event.position, false, cx)
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|view, event: &gpui::MouseUpEvent, _, cx| {
                    view.pointer_image(event.position, true, cx)
                }),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.dragging = None;
                    cx.notify();
                }),
            ),
    );
    let layout = match s.settings.layout {
        Layout::Row => 0,
        Layout::Column => 1,
        Layout::Grid => 2,
    };
    let spacing = match s.settings.spacing {
        Spacing::None => 0,
        Spacing::Small => 1,
        Spacing::Large => 2,
    };
    let format = output::COMBINE_FORMATS
        .iter()
        .position(|f| *f == s.format)
        .unwrap_or(0);
    let backgrounds: &[&str] = if s.format == "jpg" {
        &["White", "Black"]
    } else {
        &["Transparent", "White", "Black"]
    };
    let ground = backgrounds
        .iter()
        .position(|s_| *s_ == format!("{:?}", s.settings.background))
        .unwrap_or(0);
    let layouts = c::named_enabled(
        p,
        view.catalog.tr("dialogs.output.layout"),
        c::choices(
            view,
            Focus::Layout,
            &["Row", "Column", "Grid"],
            layout,
            enabled,
            cx,
        ),
        enabled,
    );
    let spacings = c::named_enabled(
        p,
        view.catalog.tr("dialogs.output.spacing"),
        c::choices(
            view,
            Focus::Spacing,
            &["None", "Small", "Large"],
            spacing,
            enabled,
            cx,
        ),
        enabled,
    );
    let columns = c::named_enabled(
        p,
        view.catalog.tr("dialogs.output.columns"),
        c::field(
            view,
            Focus::Columns,
            s.columns_text.clone(),
            String::new(),
            enabled && s.grid(),
            cx,
        ),
        enabled && s.grid(),
    )
    .w(px(59.))
    .debug_selector(|| "op4-columns".into());
    let formats = c::named_enabled(
        p,
        view.catalog.tr("dialogs.output.format"),
        c::choices(
            view,
            Focus::Format,
            output::COMBINE_FORMATS,
            format,
            enabled,
            cx,
        ),
        enabled,
    );
    let ground = c::named_enabled(
        p,
        view.catalog.tr("dialogs.output.background"),
        c::choices(
            view,
            Focus::Background,
            backgrounds,
            ground,
            enabled && s.background_enabled(count),
            cx,
        ),
        enabled && s.background_enabled(count),
    )
    .debug_selector(|| "op4-background".into());
    // Both rows share the largest first-column width, as the board's max-content grid does.
    let form = div()
        .flex()
        .gap(px(24.))
        .child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(layouts)
                .child(spacings),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(div().flex().gap(px(24.)).child(columns).child(formats))
                .child(ground),
        );
    let name = c::named_enabled(
        p,
        view.catalog.tr("dialogs.output.name"),
        c::field(
            view,
            Focus::Name,
            s.name.clone(),
            String::new(),
            enabled,
            cx,
        ),
        enabled,
    );
    vec![
        preview.into_any_element(),
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(form)
            .child(name)
            .into_any_element(),
    ]
}
