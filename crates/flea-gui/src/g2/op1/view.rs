//! The dialog shell, its scrim and every operations card, drawn from the model with theme roles only.
use super::glyph::{self, Chrome, Glyph, Paint, Tracked};
use super::model::{CopyFocus, Dialog, Operations};
use super::palette::CardPalette;
use super::press::Press;
use crate::row::marks::Mark;
use flea::i18n::I18n;
use flea::ui_logic_op1::{
    card::{self, ButtonState, Kind},
    collide::{self, Choice},
    jobs::{self, Block, State},
    trash, Part,
};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Div, ElementId, FontWeight, Hsla, SharedString,
    Stateful,
};
use std::time::Instant;

/// The pointer's state over the card's buttons, kept by the host and read by every button it draws.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pointer {
    pub hovered: Option<Press>,
    pub pressed: Option<Press>,
    // The last press edge: which button, held or released, and when.
    pub motion: Option<(Press, bool, Instant)>,
}

/// What every draw call reads: the roles, the words, the pointer, and how a button is wired to its press.
pub struct Scene<'a> {
    pub palette: &'a CardPalette,
    pub catalog: &'a I18n,
    pub pointer: &'a Pointer,
    pub wire: &'a dyn Fn(Press, Stateful<Div>) -> Stateful<Div>,
    // A still phase for every running spiral, as a capture pins it; None crawls.
    pub pinned: Option<f32>,
    pub since: Instant,
}

// One button, with its look decided by the shared rule before any colour is picked.
pub fn button(scene: &Scene, press: Press, label: String, mut state: ButtonState) -> Stateful<Div> {
    let (palette, pointer) = (scene.palette, scene.pointer);
    // A held press shows only while the pointer stays over its button, since only a release there activates.
    state.hovered = pointer.hovered == Some(press);
    state.pressed = state.hovered && pointer.pressed == Some(press);
    state.rest_is_foreground = palette.accent_frame == palette.foreground;
    let look = card::look(state);
    let tint = |ink, alpha: f32| palette.ink(ink).map(|c: Hsla| c.opacity(alpha));
    let chrome = Chrome {
        frame: palette.ink(look.frame).unwrap_or(palette.muted_frame),
        frame_width: look.frame_width,
        wash: tint(look.wash, look.wash_alpha),
        overlay: tint(look.overlay, look.overlay_alpha),
        radius: palette.radius,
        motion: pointer
            .motion
            .filter(|m| m.0 == press && !state.disabled)
            .map(|m| (m.1, m.2)),
    };
    let out = div()
        .id(ElementId::NamedInteger(
            SharedString::new_static("op1-press"),
            press.code(),
        ))
        .relative()
        .flex_none()
        .h(px(card::BUTTON_HEIGHT))
        .px(px(card::BUTTON_PAD_X + card::BORDER))
        .flex()
        .items_center()
        .whitespace_nowrap()
        .text_size(px(card::BODY_SIZE))
        .text_color(palette.ink(look.label).unwrap_or(palette.foreground))
        .opacity(look.alpha)
        .child(chrome)
        .child(div().relative().child(label));
    (scene.wire)(press, out)
}

fn runs(palette: &CardPalette, parts: &[Part]) -> Div {
    parts
        .iter()
        .fold(div().flex().flex_row().whitespace_nowrap(), |row, part| {
            let ink = if part.figure {
                palette.foreground
            } else {
                palette.muted
            };
            row.child(div().text_color(ink).child(part.text.clone()))
        })
}

fn rule(palette: &CardPalette) -> Div {
    div()
        .flex_none()
        .h(px(card::RULE))
        .bg(palette.muted_frame.opacity(card::RULE_ALPHA))
}

fn mark(icon: &str, dir: bool, ink: Hsla) -> Glyph {
    let (key, data) = Mark::for_row(if dir { "folder" } else { icon }, 0).svg();
    Glyph {
        paint: Paint::Svg(key, data),
        side: card::NAME_MARK,
        ink,
    }
}

// The shell every card shares: title and esc, the hairline inside the padding, the body, the right-aligned buttons.
// Title and buttons stay pinned; only the body scrolls when the window is too short for it.
pub fn shell(
    scene: &Scene,
    kind: Kind,
    window: (f32, f32),
    title: String,
    body: Vec<AnyElement>,
    buttons: Vec<Stateful<Div>>,
) -> Div {
    let palette = scene.palette;
    let header = div()
        .debug_selector(|| "op1-title".into())
        .flex()
        .flex_row()
        .items_center()
        .gap(px(card::NAME_GAP))
        .line_height(relative(card::LINE))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(card::TITLE_SIZE))
                .font_weight(FontWeight::BOLD)
                .text_color(palette.foreground)
                .child(title),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(card::ESC_SIZE))
                .text_color(palette.muted)
                .child(scene.catalog.tr("jobs.esc")),
        );
    let row = buttons.into_iter().enumerate().fold(
        div()
            .debug_selector(|| "op1-buttons".into())
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap(px(card::BUTTON_GAP))
            .mt(px(card::BUTTONS_MARGIN)),
        |row, (at, b)| row.child(b.debug_selector(move || format!("op1-button-{at}"))),
    );
    let room = card::frame(card::TITLE_LINE, f32::MAX, window.1).body;
    let body = div()
        .id("op1-body")
        .debug_selector(|| "op1-body".into())
        .flex()
        .flex_col()
        .gap(px(card::GAP))
        .max_h(px(room))
        .overflow_y_scroll()
        .children(body);
    div()
        .debug_selector(|| "op1-card".into())
        .w(px(card::fitted_width(kind, window.0)))
        .p(px(card::PADDING))
        .flex()
        .flex_col()
        .gap(px(card::GAP))
        .bg(palette.surface)
        .border_1()
        .border_color(palette.muted_frame)
        .rounded(px(palette.radius))
        .text_size(px(card::BODY_SIZE))
        .line_height(relative(card::LINE))
        .text_color(palette.foreground)
        .child(header)
        .child(rule(palette).debug_selector(|| "op1-rule".into()))
        .child(body)
        .child(row)
}

// The scrim dims the window once and takes hover, both buttons and the wheel; the card sits centred on it.
pub fn scrim(palette: &CardPalette, cards_below: usize, card: Div) -> Stateful<Div> {
    div()
        .id("op1-scrim")
        .debug_selector(|| "op1-scrim".into())
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .occlude()
        .bg(palette.background.opacity(card::scrim_alpha(cards_below)))
        .flex()
        .items_center()
        .justify_center()
        .child(card)
}

fn state(primary: bool, focused: bool, destructive: bool, disabled: bool) -> ButtonState {
    ButtonState {
        primary,
        focused,
        destructive,
        disabled,
        ..ButtonState::default()
    }
}

// The Delete card's title: the oracle's own while a menu refresh is out, the review's otherwise.
pub(super) fn delete_title(catalog: &I18n, review: &trash::Review, busy: bool) -> String {
    if busy {
        catalog.tr("jobs.delete.busy_title")
    } else {
        trash::confirm_title(catalog, review)
    }
}

fn caption(text: String, ink: Hsla) -> Div {
    div()
        .text_size(px(card::EXPLAIN_SIZE))
        .line_height(px(card::EXPLAIN_LINE))
        .text_color(ink)
        .child(text)
}

pub fn dialog(model: &Operations, scene: &Scene, window: (f32, f32)) -> Option<Div> {
    let (palette, catalog) = (scene.palette, scene.catalog);
    let dialog = model.dialog.as_ref()?;
    let card = match dialog {
        Dialog::Collide {
            title,
            names,
            more,
            merge,
            focus,
        } => {
            // The names and the count of the rest sit together; the explanation takes the card's gap below them.
            let mut list = div().flex().flex_col();
            for (at, name) in names.iter().enumerate() {
                list = list.child(
                    div()
                        .debug_selector(move || format!("op1-name-{at}"))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(card::NAME_GAP))
                        .child(mark(&name.icon, name.dir, palette.foreground))
                        .child(div().flex_1().min_w_0().truncate().child(name.name.clone())),
                );
            }
            if !more.is_empty() {
                list = list.child(
                    div()
                        .text_size(px(card::EXPLAIN_SIZE))
                        .truncate()
                        .child(more.clone()),
                );
            }
            let body = vec![
                list.into_any_element(),
                caption(collide::explain(catalog), palette.foreground)
                    .debug_selector(|| "op1-explain".into())
                    .into_any_element(),
            ];
            let buttons = collide::shown(*merge)
                .into_iter()
                .map(|b| {
                    button(
                        scene,
                        Press::Collide(b),
                        catalog.tr(b.label_key()),
                        state(
                            b == collide::PRIMARY,
                            b == *focus,
                            b == collide::DESTRUCTIVE,
                            false,
                        ),
                    )
                })
                .collect();
            shell(scene, Kind::Collide, window, title.clone(), body, buttons)
        }
        Dialog::Fat {
            title,
            renames,
            note,
            focus,
        } => {
            let list = renames.iter().fold(div().flex().flex_col(), |list, r| {
                list.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(card::NAME_GAP))
                        .child(mark(&r.icon, r.dir, palette.foreground))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_row()
                                .whitespace_nowrap()
                                .child(
                                    div()
                                        .flex_shrink(1.)
                                        .min_w_0()
                                        .truncate()
                                        .text_color(palette.muted)
                                        .child(r.old.clone()),
                                )
                                .child(div().flex_none().child(catalog.tr("jobs.fat.arrow")))
                                .child(div().flex_1().min_w_0().truncate().child(r.new.clone())),
                        ),
                )
            });
            let body = vec![
                list.into_any_element(),
                caption(note.clone(), palette.foreground).into_any_element(),
            ];
            let buttons = collide::FAT_BUTTONS
                .iter()
                .map(|b| {
                    button(
                        scene,
                        Press::Fat(*b),
                        catalog.tr(b.label_key()),
                        state(*b == collide::FAT_START, b == focus, false, false),
                    )
                })
                .collect();
            shell(scene, Kind::Collide, window, title.clone(), body, buttons)
        }
        Dialog::Delete {
            review,
            scope,
            focus,
            busy,
        } => {
            let enabled = !busy && trash::delete_enabled(review, model.emptying.running);
            // A menu's refresh shows the working note in place of the reviewed facts, as v0.3.12 does.
            let body = if *busy {
                vec![caption(catalog.tr("jobs.working"), palette.muted).into_any_element()]
            } else {
                vec![div()
                    .text_size(px(card::BODY_SIZE))
                    .line_height(px(card::BODY_LINE))
                    .child(trash::confirm_body(catalog, review, *scope))
                    .into_any_element()]
            };
            // While a menu refresh is out only Cancel is drawn, under the oracle's own title, as v0.3.12 does.
            let shown: &[trash::Button] = if *busy {
                &[trash::Button::Cancel]
            } else {
                &trash::BUTTONS
            };
            let buttons = shown
                .iter()
                .map(|b| {
                    let disabled = *b == trash::Button::Delete && !enabled;
                    button(
                        scene,
                        Press::Delete(*b),
                        catalog.tr(b.label_key(review.all)),
                        state(*b == trash::PRIMARY, b == focus, false, disabled),
                    )
                })
                .collect();
            shell(
                scene,
                Kind::Delete,
                window,
                delete_title(catalog, review, *busy),
                body,
                buttons,
            )
        }
        Dialog::CopyTo {
            moving,
            path,
            focus,
            busy,
            ..
        } => {
            let in_field = *focus == CopyFocus::Field;
            // The chooser's path opens selected on the 24% accent wash; an empty field shows only the caret.
            let content = if path.is_empty() {
                let caret = in_field.then(|| {
                    div()
                        .w(px(card::RULE))
                        .h(px(card::CARET_HEIGHT))
                        .bg(palette.foreground)
                });
                div().flex().items_center().children(caret)
            } else {
                let text = div()
                    .h(px(card::FIELD_SELECTION))
                    .flex()
                    .items_center()
                    .min_w_0()
                    .truncate()
                    .child(path.clone());
                if in_field {
                    text.bg(palette.accent.opacity(card::FIELD_WASH))
                } else {
                    text
                }
            };
            let field = div()
                .debug_selector(|| "op1-field".into())
                .h(px(card::BUTTON_HEIGHT))
                .px(px(card::BUTTON_PAD_X))
                .flex()
                .items_center()
                .gap(px(card::FIELD_GAP))
                .border_1()
                .border_color(if in_field {
                    palette.accent_frame
                } else {
                    palette.muted_frame
                })
                .rounded(px(palette.radius))
                .bg(palette.background)
                .child(content);
            // The field name keeps its source case in the catalog and draws in capitals, as the boards' CSS does.
            let tracked = Tracked {
                text: catalog.tr("jobs.copyto.field").to_uppercase().into(),
                size: card::EXPLAIN_SIZE,
                line: card::FIELD_LABEL_LINE,
                track: card::FIELD_LABEL_TRACK,
                ink: palette.muted,
            };
            let label = div()
                .flex()
                .debug_selector(|| "op1-field-label".into())
                .mb(px(card::FIELD_LABEL_GAP))
                .child(
                    div()
                        .debug_selector(|| "op1-field-label-text".into())
                        .child(tracked),
                );
            let body = vec![div()
                .flex()
                .flex_col()
                .child(label)
                .child(field)
                .into_any_element()];
            let confirm = catalog.tr(if *moving {
                "jobs.button.move"
            } else {
                "jobs.button.copy"
            });
            let buttons = vec![
                button(
                    scene,
                    Press::CopyCancel,
                    catalog.tr("jobs.button.cancel"),
                    state(false, *focus == CopyFocus::Cancel, false, false),
                ),
                button(
                    scene,
                    Press::CopyConfirm,
                    confirm,
                    state(
                        true,
                        *focus == CopyFocus::Confirm,
                        false,
                        *busy || !Operations::copy_enabled(path),
                    ),
                ),
            ];
            let title = catalog.tr(if *moving {
                "jobs.copyto.moveto"
            } else {
                "jobs.copyto.title"
            });
            shell(scene, Kind::CopyTo, window, title, body, buttons)
        }
    };
    Some(card)
}

fn bar(palette: &CardPalette, track: f32, share: f64) -> Div {
    div()
        .relative()
        .flex_none()
        .h(px(card::JOBS_BAR))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(palette.muted_frame.opacity(card::JOBS_TRACK_ALPHA)),
        )
        .child(
            div()
                .debug_selector(|| "op1-bar-fill".into())
                .absolute()
                .top_0()
                .left_0()
                .h_full()
                .w(px(card::fill_width(track, share)))
                .bg(palette.accent_frame),
        )
}

fn small(text: String, ink: Hsla) -> Div {
    div()
        .text_size(px(card::JOBS_SMALL))
        .line_height(px(card::JOBS_SMALL_LINE))
        .text_color(ink)
        .truncate()
        .child(text)
}

pub fn block(scene: &Scene, track: f32, block: &Block) -> Div {
    let (palette, id) = (scene.palette, block.id);
    let (head_ink, mark): (Hsla, Glyph) = match block.state {
        // A failed head and its alert take the error role as text on the card's surface.
        State::Failed => (
            palette.error_on_surface,
            Glyph::svg(glyph::ALERT, card::JOBS_MARK, palette.error_on_surface),
        ),
        State::Waiting => (
            palette.foreground,
            spiral(
                Some(glyph::BOARD_PHASE),
                scene.since,
                palette.muted_frame.opacity(card::JOBS_WAITING_ALPHA),
            ),
        ),
        _ => (
            palette.foreground,
            spiral(scene.pinned, scene.since, palette.muted_frame),
        ),
    };
    let head = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(card::JOBS_GAP))
        .text_size(px(card::JOBS_HEAD))
        .line_height(px(card::JOBS_HEAD_LINE))
        .child(
            div()
                .flex_none()
                .debug_selector(move || format!("op1-job-{id}-mark"))
                .child(mark),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(head_ink)
                .child(block.head.clone()),
        );
    let mut out = div()
        .debug_selector(move || format!("op1-job-{id}"))
        .flex()
        .flex_col()
        .gap(px(card::JOBS_GAP))
        .child(head);
    if !block.file.is_empty() {
        out = out.child(small(block.file.clone(), palette.muted));
    }
    for line in &block.output {
        out = out.child(small(line.clone(), palette.muted));
    }
    if let Some(share) = block.bar {
        out = out
            .child(bar(palette, track, share).debug_selector(move || format!("op1-job-{id}-bar")));
    }
    if !block.line.is_empty() {
        out = out.child(
            runs(palette, &block.line)
                .text_size(px(card::JOBS_SMALL))
                .line_height(px(card::JOBS_SMALL_LINE)),
        );
    }
    let buttons = block.buttons.iter().fold(
        div()
            .flex()
            .flex_row()
            .justify_end()
            .gap(px(card::BUTTON_GAP)),
        |row, action| {
            row.child(button(
                scene,
                Press::Job(block.id, *action),
                scene.catalog.tr(action.label_key()),
                ButtonState::default(),
            ))
        },
    );
    out.child(buttons)
}

fn spiral(still: Option<f32>, since: Instant, ink: Hsla) -> Glyph {
    Glyph {
        paint: Paint::Spiral { still, since },
        side: card::JOBS_MARK,
        ink,
    }
}

// The Jobs card: every unfinished or failed job as its own block, newest on top, full-width hairlines between.
pub fn jobs_card(model: &Operations, scene: &Scene, window: (f32, f32)) -> Option<Div> {
    if model.jobs.folded || !model.jobs.visible() {
        return None;
    }
    let palette = scene.palette;
    let track = card::jobs_track(window.0);
    let mut out = div()
        .debug_selector(|| "op1-jobs".into())
        .w(px(
            card::JOBS_WIDTH.min((window.0 - 2. * card::JOBS_RIGHT).max(0.))
        ))
        .p(px(card::JOBS_PADDING))
        .flex()
        .flex_col()
        .gap(px(card::JOBS_GAP))
        .bg(palette.surface)
        .border_1()
        .border_color(palette.muted_frame)
        .rounded(px(palette.radius))
        .text_color(palette.foreground);
    for (at, (_, built)) in model.blocks.iter().enumerate() {
        if at > 0 {
            out = out.child(
                rule(palette)
                    .mx(px(-card::JOBS_PADDING))
                    .my(px(card::JOBS_RULE_MARGIN))
                    .debug_selector(move || format!("op1-jobs-rule-{at}")),
            );
        }
        out = out.child(block(scene, track, built));
    }
    Some(out)
}

// The strip's two runs, one padding before the disk facts: the oldest unfinished head in the foreground, the count muted.
pub fn strip(model: &Operations, palette: &CardPalette, catalog: &I18n) -> Option<Div> {
    let (head, count) = jobs::strip(catalog, &model.jobs)?;
    let mut out = div()
        .flex()
        .flex_row()
        .whitespace_nowrap()
        .mr(px(card::STRIP_GAP))
        .child(
            div()
                .text_color(palette.foreground)
                .child(SharedString::from(head)),
        );
    if !count.is_empty() {
        out = out.child(div().text_color(palette.muted).child(count));
    }
    Some(out)
}

pub fn choice_label(catalog: &I18n, choice: Choice) -> String {
    catalog.tr(choice.label_key())
}
