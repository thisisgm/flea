//! Open With application rows retain the shared card anatomy and existing icon/control services.
use super::*;
use gpui::{div, prelude::*, px, relative, Div, FontWeight};

const TRACKING: f32 = CAPTION * 0.14;

impl AppRow {
    /// The board height a row adds besides application rows: an eyebrow, its air and a later group's hairline.
    pub(crate) fn chrome(&self) -> f32 {
        match self {
            AppRow::Eyebrow { first: true, .. } => EYEBROW + FIRST_EYEBROW_AIR,
            AppRow::Eyebrow { first: false, .. } => EYEBROW + LATER_EYEBROW_AIR + 1.,
            _ => 0.,
        }
    }
}

impl Draw<'_> {
    pub(super) fn apps(&self, rows: &[AppRow], filler: f32) -> gpui::Stateful<Div> {
        let t = self.t;
        let mut list = div().flex().flex_col().mx(self.px(-PAD));
        let (mut eyebrows, mut apps) = (0, 0);
        for row in rows {
            list = match row {
                AppRow::Empty { text, height } => list.child(
                    div()
                        .h(self.px(*height))
                        .flex_none()
                        .px(self.px(2. * ROW_INSET))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            self.track(
                                self.text(CAPTION)
                                    .whitespace_normal()
                                    .w_full()
                                    .line_height(relative(1.5))
                                    .text_center()
                                    .text_color(t.muted)
                                    .child(text.clone()),
                                &format!("note:{text}"),
                            ),
                        ),
                ),
                AppRow::Eyebrow { text, first } => {
                    let air = if *first {
                        FIRST_EYEBROW_AIR
                    } else {
                        LATER_EYEBROW_AIR
                    };
                    let list = if *first {
                        list
                    } else {
                        list.child(self.track(self.hairline(), "groupRule"))
                    };
                    eyebrows += 1;
                    list.child(
                        self.track(
                            self.text(CAPTION)
                                .h(self.px(EYEBROW))
                                .mt(self.px(air))
                                .px(self.px(ROW_INSET))
                                .flex()
                                .items_center()
                                .font_weight(FontWeight::BOLD)
                                .text_color(t.muted)
                                .child({
                                    let mut caption = self.caption(text, EYEBROW, t.muted);
                                    caption.font.weight = FontWeight::BOLD;
                                    caption.tracking = self.px(
                                        if crate::g2::status_feedback::caption::caseless(
                                            self.language,
                                        ) {
                                            0.
                                        } else {
                                            TRACKING
                                        },
                                    );
                                    caption
                                }),
                            &format!("eyebrow{}", eyebrows - 1),
                        ),
                    )
                }
                AppRow::App {
                    at,
                    icon,
                    label,
                    tail,
                    cursor,
                } => {
                    let mut line = self
                        .text(BOARD_BODY)
                        .relative()
                        .h(self.px(APP_ROW))
                        .flex()
                        .items_center()
                        .gap(self.px(ROW_GAP))
                        .px(self.px(ROW_INSET))
                        .text_color(t.fg);
                    line = match cursor {
                        Cursor::None => line,
                        Cursor::Waiting => line.bg(t.fg.opacity(HOVER)),
                        Cursor::Focused => line.bg(t.accent.opacity(WASH_CURSOR)).child(
                            div()
                                .absolute()
                                .left_0()
                                .top_0()
                                .bottom_0()
                                .w(px(2.))
                                .bg(t.accent_frame),
                        ),
                    };
                    let slot = crate::g2::context_menus::app_icons::mark(
                        icon.clone(),
                        MARK * self.s,
                        t.muted_frame,
                    );
                    let line = line.child(slot).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(label.clone()),
                    );
                    let line = if tail.is_empty() {
                        line
                    } else {
                        line.child(self.text(CAPTION).text_color(t.muted).child(tail.clone()))
                    };
                    apps += 1;
                    list.child(
                        self.wired(
                            Control::Application(*at),
                            self.track(line, &format!("row{}", apps - 1))
                                .id(("op3-app", *at)),
                        ),
                    )
                }
            };
        }
        let list = list
            .child(div().h(self.px(filler)).flex_none())
            .child(self.track(self.hairline(), "listRule"));
        self.wired(
            Control::Applications,
            self.track(list, "list").id("op3-applications"),
        )
    }
}
