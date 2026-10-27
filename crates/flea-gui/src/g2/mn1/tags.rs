//! Shift+T prepares the existing Tags flyout from the backend's captured selection state.
use super::delivery::string;
use crate::state::Shell;
use flea::{
    jsondoc::Json,
    ui_logic_mn1::{Entry, Text},
    ui_logic_pl4::tags::{self, FlyoutRow, State},
};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn open_tags_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.focus_side;
        if self.locked_folder(side).is_some()
            || self.panes[side]
                .as_ref()
                .is_none_or(|host| host.pane.held.row(host.pane.cursor).is_none())
        {
            return;
        }
        self.open_flyout("tags", window, cx);
        let Some(capture) = self.menu.captured.clone() else {
            return;
        };
        self.menu.tags_reading = true;
        let rows = capture
            .rows
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.menu_send(
            side,
            format!(
                "{{\"c\":\"tagstates\",\"id\":{},\"listing\":{},\"rows\":[{rows}]}}",
                capture.id, capture.listing
            ),
            cx,
        );
    }

    pub(crate) fn menu_tags_reply(
        &mut self,
        side: usize,
        reply: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if string(reply, "t") != "tagstates" {
            return false;
        }
        let number = |key| match reply.get(key) {
            Some(Json::Num(value)) => value.parse::<u64>().ok(),
            _ => None,
        };
        let Some(capture) = self.menu.captured.as_ref().filter(|capture| {
            self.menu.tags_reading
                && capture.side == side
                && Some(capture.id) == number("id")
                && number("listing").is_none_or(|listing| listing == capture.listing as u64)
        }) else {
            return true;
        };
        if capture.identity != self.menu_identity() {
            self.retire_menu(window, cx);
            return true;
        }
        let names = |key| {
            reply
                .get(key)
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(Json::as_str)
                .collect::<Vec<_>>()
        };
        let states = tags::states_from(&names("applied"), &names("mixed"));
        self.menu.context.tag_submenu = Some(entries(&states));
        self.menu.tags_reading = false;
        self.refresh_menu(window, cx);
        true
    }
}

fn entries(states: &[State; 7]) -> Vec<Entry> {
    tags::flyout(states, false)
        .into_iter()
        .map(|row| match row {
            FlyoutRow::Tag(tag) => Entry {
                tag: Some((tags::index_of(tag.name).unwrap() + 1) as u8),
                check: (tag.state == State::Applied).then_some(true),
                ..Entry::leaf(tag.name, Text::Key(tag.label_key)).with_key_hint(tag.digit)
            },
            FlyoutRow::Separator => Entry::separator(),
            FlyoutRow::Clear => {
                Entry::leaf("__clear__", Text::Key(tags::CLEAR_LABEL_KEY)).with_glyph("minus")
            }
        })
        .collect()
}
