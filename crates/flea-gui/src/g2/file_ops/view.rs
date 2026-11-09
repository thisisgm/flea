//! The dialog shell, its scrim and every operations card, drawn from the model with theme roles only.
use super::feedback::{label, runs};
use super::geometry::Metrics;
use super::glyph::{self, Chrome, Glyph, Paint};
use super::line::Line;
use super::model::{Dialog, Operations};
use super::palette::CardPalette;
use super::press::Press;
use crate::{
    feedback::spinner::{self, Mode},
    row::marks::Mark,
};
use flea::i18n::I18n;
use flea::ui_logic_file_ops::{
    card::{self, ButtonState, Kind},
    collide::{self, Choice},
    jobs::{self, Block, State},
    trash,
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
    pub copy_field: Option<gpui::Entity<super::host::Overlay>>,
    pub(crate) frame: Option<&'a super::measured::Frame>,
    pub typeface: &'a crate::list::Typeface,
    pub palette: &'a CardPalette,
    pub catalog: &'a I18n,
    pub pointer: &'a Pointer,
    pub wire: &'a dyn Fn(Press, Stateful<Div>) -> Stateful<Div>,
    // Fixed mode seconds for a capture; None animates.
    pub pinned: Option<f32>,
    pub feedback: Option<(u64, &'a crate::g2::status_feedback::observe::Paint)>,
}

impl Scene<'_> {
    pub(super) fn track<D: ParentElement>(&self, div: D, name: &str, text: String) -> D {
        match self.frame {
            Some(frame) => frame.track(div, name.into(), text),
            None => div,
        }
    }
}

// One button, with its look decided by the shared rule before any colour is picked.
pub fn button(scene: &Scene, press: Press, label: String, mut state: ButtonState) -> Stateful<Div> {
    let (palette, pointer) = (scene.palette, scene.pointer);
    let m = Metrics::new(scene.typeface);
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
        .h(px(m.button))
        .px(px(m.button_pad + card::BORDER))
        .flex()
        .items_center()
        .whitespace_nowrap()
        .text_size(px(m.body))
        .text_color(palette.ink(look.label).unwrap_or(palette.foreground))
        .opacity(look.alpha)
        .child(chrome)
        .child(div().relative().child(Line::new(label.clone())));
    let out = scene.track(out, &format!("button-{}", press.code()), label);
    (scene.wire)(press, out)
}

fn rule(palette: &CardPalette) -> Div {
    div()
        .flex_none()
        .h(px(card::RULE))
        .bg(palette.muted_frame.opacity(card::RULE_ALPHA))
}

pub(super) fn mark(icon: &str, dir: bool, side: f32, ink: Hsla) -> Glyph {
    let (key, data) = Mark::for_row(if dir { "folder" } else { icon }, 0).svg();
    Glyph {
        paint: Paint::Svg(key, data),
        side,
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
    let m = Metrics::new(scene.typeface);
    let header = div()
        .debug_selector(|| "op1-title".into())
        .flex()
        .flex_row()
        .items_center()
        .gap(px(card::NAME_GAP))
        .line_height(relative(card::LINE))
        .child(
            scene.track(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(m.title))
                    .font_weight(FontWeight::BOLD)
                    .text_color(palette.foreground)
                    .child(Line::new(title.clone())),
                "title",
                title,
            ),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(m.caption))
                .text_color(palette.muted)
                .child(Line::new(scene.catalog.tr("jobs.esc"))),
        );
    let row = buttons.into_iter().enumerate().fold(
        div()
            .debug_selector(|| "op1-buttons".into())
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap(px(m.button_gap))
            .mt(px(m.buttons_margin)),
        |row, (at, b)| row.child(b.debug_selector(move || format!("op1-button-{at}"))),
    );
    let row = scene.track(row.relative(), "buttons", String::new());
    let room = m.body_room(window.1);
    let body = div()
        .id("op1-body")
        .debug_selector(|| "op1-body".into())
        .flex()
        .flex_col()
        .gap(px(m.gap))
        .max_h(px(room))
        .overflow_y_scroll()
        .children(body);
    let body = scene.track(body.relative(), "body", String::new());
    let card = div()
        .relative()
        .debug_selector(|| "op1-card".into())
        .min_w(px(m.width(kind, window.0)))
        .max_w(px(m.widest(kind, window.0)))
        .p(px(m.padding))
        .flex()
        .flex_col()
        .gap(px(m.gap))
        .bg(palette.surface)
        .border_1()
        .border_color(palette.muted_frame)
        .rounded(px(palette.radius))
        .font_family(scene.typeface.font.family.clone())
        .text_size(px(m.body))
        .line_height(relative(card::LINE))
        .text_color(palette.foreground)
        .child(yields(header))
        .child(yields(rule(palette).debug_selector(|| "op1-rule".into())))
        .child(yields(body))
        .child(row);
    scene.track(div().relative().child(card), "card", String::new())
}

// A part that fills the card's width but asks none of its own, so only the button row can widen the card.
fn yields<E: Styled>(part: E) -> E {
    part.w_0().min_w_full()
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

pub(super) fn state(
    primary: bool,
    focused: bool,
    destructive: bool,
    disabled: bool,
) -> ButtonState {
    ButtonState {
        primary,
        focused,
        destructive,
        disabled,
        ..ButtonState::default()
    }
}

// The Delete card's title: the oracle's own while a menu refresh is out, the review's otherwise.
pub(crate) fn delete_title(catalog: &I18n, review: &trash::Review, busy: bool) -> String {
    if busy {
        catalog.tr("jobs.delete.busy_title")
    } else {
        trash::confirm_title(catalog, review)
    }
}

// The Delete card's body sentence: the working note while a menu refresh is out, the reviewed facts otherwise.
pub(crate) fn delete_body(
    catalog: &I18n,
    review: &trash::Review,
    scope: trash::Scope,
    busy: bool,
) -> String {
    if busy {
        catalog.tr("jobs.working")
    } else {
        trash::confirm_body(catalog, review, scope)
    }
}

// The Delete card's sentence keeps the shell's live body size, the buttons' size, and wraps (EY-350).
pub(super) fn sentence(text: String) -> Div {
    div()
        .line_height(relative(card::BODY_LINE / card::BODY_SIZE))
        .child(text)
}

pub(super) fn caption(scene: &Scene, text: String, ink: Hsla) -> Div {
    let m = Metrics::new(scene.typeface);
    div()
        .text_size(px(m.caption))
        .line_height(relative(card::EXPLAIN_LINE / card::EXPLAIN_SIZE))
        .text_color(ink)
        .child(text)
}

pub fn dialog(model: &Operations, scene: &Scene, window: (f32, f32)) -> Option<Div> {
    dialog_snapshot(model.dialog.as_ref(), model.emptying.running, scene, window)
}

pub(super) fn dialog_snapshot(
    dialog: Option<&Dialog>,
    emptying: bool,
    scene: &Scene,
    window: (f32, f32),
) -> Option<Div> {
    let (palette, catalog) = (scene.palette, scene.catalog);
    let dialog = dialog?;
    let card = match dialog {
        Dialog::Collide { .. } => super::collision::draw(dialog, scene, window),
        Dialog::Fat {
            title,
            renames,
            note,
            focus,
        } => {
            let m = Metrics::new(scene.typeface);
            let list = renames.iter().fold(div().flex().flex_col(), |list, r| {
                list.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(m.name_gap))
                        .child(mark(&r.icon, r.dir, m.name_mark, palette.foreground))
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
                caption(scene, note.clone(), palette.foreground).into_any_element(),
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
            ..
        } => {
            let enabled = !busy && trash::delete_enabled(review, emptying);
            // A menu's refresh shows the working note in place of the reviewed facts, as v0.3.12 does.
            let body = if *busy {
                vec![caption(
                    scene,
                    delete_body(catalog, review, *scope, true),
                    palette.muted,
                )
                .into_any_element()]
            } else {
                vec![sentence(delete_body(catalog, review, *scope, false)).into_any_element()]
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
            error,
            ..
        } => super::copy_to::card(scene, window, *moving, path, *focus, *busy, error, None),
        Dialog::SendTo {
            path,
            focus,
            busy,
            card,
            ..
        } => {
            let mut card = card.clone();
            let refusal = flea::ui_logic_rclone_send::folder_check(path).err();
            card.can_submit = !busy && refusal.is_none();
            if refusal.is_some() || card.value != *path {
                card.error =
                    flea::ui_logic_rclone_send::present::card_error(scene.catalog, refusal, None);
            }
            super::copy_to::card(scene, window, false, path, *focus, *busy, "", Some(&card))
        }
        Dialog::NewFile {
            name,
            focus,
            busy,
            error,
            ..
        } => super::new_file::card(scene, window, name, *focus, *busy, error),
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

pub(super) fn small(text: String, ink: Hsla) -> Div {
    div()
        .text_size(px(card::JOBS_SMALL))
        .line_height(px(card::JOBS_SMALL_LINE))
        .text_color(ink)
        .truncate()
        .child(text)
}

pub fn block(scene: &Scene, track: f32, block: &Block, mode: Mode) -> Div {
    let (palette, id) = (scene.palette, block.id);
    let paint = scene
        .feedback
        .filter(|(observed, _)| *observed == id)
        .map(|(_, paint)| paint);
    let still = (block.state == State::Waiting)
        .then(|| spinner::spiral_seconds(WAITING_PHASE))
        .or(scene.pinned);
    let (head_ink, mark): (Hsla, Glyph) = match block.state {
        // A failed head and its alert take the error role as text on the card's surface.
        State::Failed => (
            palette.error_on_surface,
            Glyph::svg(glyph::ALERT, card::JOBS_MARK, palette.error_on_surface),
        ),
        // TransferCard.qml draws its spiral in the muted frame ink; a waiting job holds it still at 55%.
        _ => {
            let ink = if block.mark_dim {
                palette.muted_frame.opacity(card::JOBS_WAITING_ALPHA)
            } else {
                palette.muted_frame
            };
            (palette.foreground, spiral(still, block.id, mode, ink))
        }
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
            label(paint)
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
    out = out.children(super::feedback::file(palette, &block.file, paint));
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
                .line_height(px(card::JOBS_SMALL_LINE))
                .observed(paint),
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
    if block.cancelling {
        let m = Metrics::new(scene.typeface);
        out.child(
            div()
                .h(px(m.button))
                .flex()
                .justify_end()
                .items_center()
                .text_size(px(m.body))
                .text_color(palette.muted)
                .debug_selector(move || format!("op1-job-{id}-cancelling"))
                .child(scene.catalog.tr("send.rclone.cancelling")),
        )
    } else {
        out.child(buttons)
    }
}

// The jobs board's still frame: the dash on the inner turns, units 54 to 114 of the spiral.
const WAITING_PHASE: f32 = 27.;

fn spiral(still: Option<f32>, id: u64, mode: Mode, ink: Hsla) -> Glyph {
    Glyph {
        paint: Paint::Spiral { id, mode, still },
        side: card::JOBS_MARK,
        ink,
    }
}

// The Jobs card: every unfinished or failed job as its own block, newest on top, full-width hairlines between.
pub fn jobs_card(model: &Operations, scene: &Scene, window: (f32, f32)) -> Option<Div> {
    jobs_snapshot(&model.jobs, &model.blocks, scene, window)
}

pub(super) fn jobs_snapshot(
    jobs: &flea::ui_logic_file_ops::jobs::Jobs,
    blocks: &[(flea::ui_logic_file_ops::JobRow, Block)],
    scene: &Scene,
    window: (f32, f32),
) -> Option<Div> {
    if jobs.folded || !jobs.visible() {
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
    for (at, (row, built)) in blocks.iter().enumerate() {
        if at > 0 {
            out = out.child(
                rule(palette)
                    .mx(px(-card::JOBS_PADDING))
                    .my(px(card::JOBS_RULE_MARGIN))
                    .debug_selector(move || format!("op1-jobs-rule-{at}")),
            );
        }
        out = out.child(block(scene, track, built, Mode::for_job(&row.kind)));
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
