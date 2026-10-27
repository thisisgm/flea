//! Caller filters use the mounted chip menu and re-list through the existing chooser transport.
use super::Picker;
use crate::g2::pk2::{chip_input::FilterChosen, chrome::ChipMenu};
use flea::ui_logic_pk2::request::Chip;
use gpui::{AppContext, Context, KeyDownEvent, Window};

impl Picker {
    pub(super) fn open_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.req.filters.is_empty()
            || self.backend_unavailable()
            || self.accept_state().submitting()
        {
            return;
        }
        let mut chips: Vec<Chip> = self
            .req
            .filters
            .iter()
            .enumerate()
            .map(|(index, filter)| Chip {
                label: filter.label.clone(),
                index: index as i64,
            })
            .collect();
        chips.push(Chip {
            label: self.locale.catalog.tr("picker.browser.all_files"),
            index: -1,
        });
        let face = cx
            .try_global::<crate::list::Typeface>()
            .cloned()
            .unwrap_or_else(crate::list::Typeface::omarchy_default);
        let menu = cx.new(|_| ChipMenu::new((*self.locale.catalog).clone(), face));
        let height = f32::from(window.viewport_size().height);
        menu.update(cx, |menu, _| menu.open(&chips, (14., height - 86.), window));
        self.chrome.filter_subscription =
            Some(cx.subscribe(&menu, |picker, _, chosen: &FilterChosen, cx| {
                picker.choose_filter(chosen.0, cx)
            }));
        self.chrome.menu_view = Some(menu);
        self.chrome.menu = Some(0);
        window.focus(&self.chrome.filter, cx);
        cx.notify();
    }

    pub fn choose_filter(&mut self, index: i64, cx: &mut Context<Self>) {
        if index < -1 || index >= self.req.filters.len() as i64 {
            return;
        }
        self.chrome.menu = None;
        self.chrome.menu_view = None;
        if index != self.listing.chip {
            self.listing.chip = index;
            let path = self.listing.location.clone();
            self.open_folder(&path, cx);
        }
        cx.notify();
    }

    pub(super) fn filter_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.chrome.menu_view.clone() else {
            return false;
        };
        let chosen = menu.update(cx, |menu, cx| {
            let chosen = menu.key(event, window);
            cx.notify();
            chosen
        });
        if let Some(index) = chosen {
            self.choose_filter(index, cx);
        } else if !menu.read(cx).opened() {
            self.chrome.menu_view = None;
            self.chrome.menu = None;
            cx.notify();
        }
        true
    }

    pub(super) fn filter_label(&self) -> String {
        usize::try_from(self.listing.chip)
            .ok()
            .and_then(|at| self.req.filters.get(at))
            .map_or_else(
                || self.locale.catalog.tr("picker.browser.all_files"),
                |filter| filter.label.clone(),
            )
    }
}
