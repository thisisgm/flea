//! The existing card button grammar, checkboxes and named fields, with no local palette.
use super::{host::OutputView, model::Focus};
use crate::g2::op1::{
    glyph::{Chrome, Glyph, Tracked},
    palette::CardPalette,
};
use flea::ui_logic_op1::card::{self, ButtonState};
use gpui::{
    div, prelude::*, px, relative, Context, Div, ElementId, MouseButton, SharedString, Stateful,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Field(Focus),
    Choice(Focus, usize),
    Check(Focus),
    Image(usize),
    Cancel,
    Submit,
}

impl Control {
    fn code(self) -> u64 {
        match self {
            Self::Field(f) => 100 + f as u64,
            Self::Choice(f, i) => 1000 + f as u64 * 300 + i as u64,
            Self::Check(f) => 10000 + f as u64,
            Self::Image(i) => 11000 + i as u64,
            Self::Cancel => 12000,
            Self::Submit => 12001,
        }
    }
}

pub fn wire(
    view: &OutputView,
    control: Control,
    enabled: bool,
    element: Div,
    cx: &mut Context<OutputView>,
) -> Stateful<Div> {
    let element = view.body_scroll.observe(control, element);
    let out = element.id(ElementId::NamedInteger(
        SharedString::new_static("op4-control"),
        control.code(),
    ));
    if !enabled {
        return out;
    }
    out.cursor_pointer()
        .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
            view.hovered = if *hovered {
                Some(control)
            } else {
                view.hovered.filter(|h| *h != control)
            };
            cx.notify();
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _, _, cx| {
                view.pressed = Some(control);
                view.motion = Some((control, true, cx.background_executor().now()));
                if let Control::Image(i) = control {
                    view.dragging = Some(i);
                }
                cx.notify();
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |view, _, _, cx| {
                if let (Some(from), Control::Image(to)) = (view.dragging.take(), control) {
                    view.model.move_image(from, to, &view.catalog);
                }
                view.pressed = None;
                view.motion = Some((control, false, cx.background_executor().now()));
                view.flush(cx);
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(move |view, _, _, cx| {
                if view.pressed == Some(control) {
                    view.pressed = None;
                    view.motion = Some((control, false, cx.background_executor().now()));
                    view.dragging = None;
                    cx.notify();
                }
            }),
        )
        .on_click(cx.listener(move |view, _, window, cx| view.activate(control, window, cx)))
}

pub fn button(
    view: &OutputView,
    control: Control,
    label: String,
    primary: bool,
    focused: bool,
    enabled: bool,
    cx: &mut Context<OutputView>,
) -> Stateful<Div> {
    let p = &view.palette;
    let look = card::look(ButtonState {
        primary,
        focused,
        disabled: !enabled,
        hovered: view.hovered == Some(control),
        pressed: view.hovered == Some(control) && view.pressed == Some(control),
        rest_is_foreground: p.accent_frame == p.foreground,
        ..Default::default()
    });
    let chrome = Chrome {
        frame: p.ink(look.frame).unwrap_or(p.muted_frame),
        frame_width: look.frame_width,
        wash: p.ink(look.wash).map(|c| c.opacity(look.wash_alpha)),
        overlay: p.ink(look.overlay).map(|c| c.opacity(look.overlay_alpha)),
        radius: p.radius,
        motion: view
            .motion
            .filter(|m| m.0 == control && enabled)
            .map(|m| (m.1, m.2)),
    };
    wire(
        view,
        control,
        enabled,
        div()
            .debug_selector(move || format!("op4-control-{control:?}"))
            .relative()
            .flex_none()
            .h(px(30.))
            .px(px(10.))
            .flex()
            .items_center()
            .text_size(px(14.))
            .text_color(p.foreground)
            .opacity(look.alpha)
            .child(chrome)
            .child(div().relative().child(label)),
        cx,
    )
}

pub fn named_enabled(
    p: &CardPalette,
    label: String,
    control: impl IntoElement,
    enabled: bool,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(Tracked {
            text: label.to_uppercase().into(),
            size: 12.,
            line: 14.,
            track: 1.2,
            ink: p.muted.opacity(if enabled { 1. } else { 0.55 }),
        })
        .child(control)
}

pub fn field(
    view: &OutputView,
    focus: Focus,
    value: String,
    placeholder: String,
    enabled: bool,
    cx: &mut Context<OutputView>,
) -> Stateful<Div> {
    let p = &view.palette;
    let field = div()
        .debug_selector(move || format!("op4-control-Field({focus:?})"))
        .h(px(30.))
        .px(px(9.))
        .flex()
        .items_center()
        .overflow_hidden()
        .border_1()
        .border_color(if view.model.focus == Some(focus) {
            p.accent_frame
        } else {
            p.muted_frame
        })
        .bg(p.background)
        .rounded(px(p.radius))
        .text_size(px(14.))
        .text_color(if value.is_empty() {
            p.muted
        } else {
            p.foreground
        })
        .opacity(if enabled { 1. } else { 0.55 });
    if enabled && view.editor.owner == Some(focus) {
        if let Some(face) = &view.editor.face {
            let input =
                crate::g2::op1::field::mount(cx.entity(), div().flex_1().min_w_0().h_full(), face);
            let field = field
                .relative()
                .when(value.is_empty() && !placeholder.is_empty(), |field| {
                    field.child(
                        div()
                            .absolute()
                            .left(px(10.))
                            .top_0()
                            .h_full()
                            .flex()
                            .items_center()
                            .text_color(p.muted)
                            .child(placeholder),
                    )
                })
                .child(input);
            return wire(view, Control::Field(focus), enabled, field, cx);
        }
    }
    wire(
        view,
        Control::Field(focus),
        enabled,
        field.child(if value.is_empty() { placeholder } else { value }),
        cx,
    )
}

pub fn check(
    view: &OutputView,
    focus: Focus,
    label: String,
    value: bool,
    enabled: bool,
    cx: &mut Context<OutputView>,
) -> Stateful<Div> {
    let p = &view.palette;
    let focused = view.model.focus == Some(focus);
    let mut box_ = div()
        .relative()
        .flex_none()
        .size(px(18.))
        .border_2()
        .border_color(if value || focused {
            p.foreground
        } else {
            p.muted_frame
        })
        .rounded(px(p.radius / 2.));
    if value {
        const CHECK: (&str, &[u8]) = ("op4-check", br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="square" stroke-linejoin="miter"><path d="m5 12 4 4L19 6"/></svg>"#);
        box_ = box_
            .bg(p.foreground)
            .child(Glyph::svg(CHECK, 14., p.surface));
        if focused {
            box_ = box_.child(
                div()
                    .absolute()
                    .inset(px(-4.))
                    .border_1()
                    .border_color(p.foreground)
                    .rounded(px(p.radius / 2.)),
            );
        }
    }
    wire(
        view,
        Control::Check(focus),
        enabled,
        div()
            .debug_selector(move || format!("op4-control-Check({focus:?})"))
            .flex()
            .items_center()
            .gap(px(9.))
            .h(px(18.))
            .text_size(px(14.))
            .text_color(p.foreground)
            .opacity(if enabled { 1. } else { 0.55 })
            .child(box_)
            .child(label),
        cx,
    )
}

pub fn choices(
    view: &OutputView,
    focus: Focus,
    labels: &[&str],
    selected: usize,
    enabled: bool,
    cx: &mut Context<OutputView>,
) -> Div {
    labels
        .iter()
        .enumerate()
        .fold(div().flex().gap(px(4.)), |row, (i, label)| {
            let p = &view.palette;
            let text = view.catalog.tr(&format!("dialogs.output.choice.{label}"));
            let item = div()
                .h(px(30.))
                .px(px(9.))
                .flex()
                .items_center()
                .border_1()
                .rounded(px(p.radius))
                .border_color(
                    if enabled && view.model.focus == Some(focus) && i == selected {
                        p.accent_frame
                    } else if i == selected {
                        p.foreground
                    } else {
                        p.muted_frame
                    },
                )
                .text_color(if i == selected { p.foreground } else { p.muted })
                .text_size(px(14.))
                .line_height(relative(1.2))
                .opacity(if enabled { 1. } else { 0.55 })
                .child(text);
            row.child(wire(view, Control::Choice(focus, i), enabled, item, cx))
        })
}
