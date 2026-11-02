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
        // A reopen inside the close fade keeps the closing menu, so its card fades back up from where it stood.
        let menu = self
            .chrome
            .menu_closing
            .take()
            .unwrap_or_else(|| cx.new(|_| ChipMenu::new((*self.locale.catalog).clone(), face)));
        let height = f32::from(window.viewport_size().height);
        let at = self
            .chrome
            .probe
            .bounds("picker-filter")
            .map_or((14., height - 86.), |bounds| {
                let fit = flea::ui_logic_mn1::fit::ROW as f32 * chips.len() as f32
                    + 2. * flea::ui_logic_mn1::fit::PAD as f32;
                // The probe sits inside the trigger's 1 px frame; the menu meets its outer edge.
                (
                    f32::from(bounds.left()) - 1.,
                    f32::from(bounds.top()) - 1. - fit,
                )
            });
        menu.update(cx, |menu, _| {
            menu.open(&chips, at, window);
            menu.select_chip(&chips, self.listing.chip, window);
        });
        self.chrome.filter_subscription =
            Some(cx.subscribe(&menu, |picker, _, chosen: &FilterChosen, cx| {
                picker.choose_filter(chosen.0, cx)
            }));
        // An outside click closes the card inside ChipMenu, so its notify is the picker's only sign to drop it.
        self.chrome.filter_close = Some(cx.observe(&menu, |picker, menu, cx| {
            if !menu.read(cx).opened() && picker.chrome.menu_view.as_ref() == Some(&menu) {
                picker.retire_filter_menu();
                picker.chrome.menu = None;
                cx.notify();
            } else if !menu.read(cx).visible() && picker.chrome.menu_closing.as_ref() == Some(&menu)
            {
                picker.chrome.menu_closing = None;
                cx.notify();
            }
        }));
        self.chrome.menu_view = Some(menu);
        self.chrome.menu = Some(0);
        window.focus(&self.chrome.filter, cx);
        cx.notify();
    }

    // The open chip menu stops taking input but stays drawn through its close fade.
    fn retire_filter_menu(&mut self) {
        if let Some(menu) = self.chrome.menu_view.take() {
            self.chrome.menu_closing = Some(menu);
        }
    }

    pub fn choose_filter(&mut self, index: i64, cx: &mut Context<Self>) {
        if index < -1 || index >= self.req.filters.len() as i64 {
            return;
        }
        self.chrome.menu = None;
        self.retire_filter_menu();
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
            self.retire_filter_menu();
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
