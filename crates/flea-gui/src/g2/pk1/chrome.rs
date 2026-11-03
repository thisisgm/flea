//! Chooser chrome state shared by navigation, focus, filters and the mounted listing views.
use super::Picker;
use gpui::{Context, Entity, FocusHandle};

pub struct Chrome {
    pub keyboard: crate::window::input::Keyboard,
    pub preset: String,
    pub recent_back: Option<fn(&mut Picker, &mut Context<Picker>)>,
    pub probe: crate::g2::st1::probe::Probe,
    pub focused: Option<&'static str>,
    pub rail_focus: Option<FocusHandle>,
    pub back: FocusHandle,
    pub up: FocusHandle,
    pub filter: FocusHandle,
    pub cancel: FocusHandle,
    pub answer: FocusHandle,
    pub new_folder: FocusHandle,
    pub history: Vec<String>,
    pub backing: bool,
    pub menu: Option<usize>,
    pub menu_view: Option<Entity<crate::g2::pk2::chrome::ChipMenu>>,
    // The last chip menu, kept drawn through its 140 ms close fade.
    pub menu_closing: Option<Entity<crate::g2::pk2::chrome::ChipMenu>>,
    pub filter_subscription: Option<gpui::Subscription>,
    pub filter_close: Option<gpui::Subscription>,
    pub folder_request: Option<String>,
    pub folder: Option<Entity<super::filename::Filename>>,
    pub restore: Option<FocusHandle>,
    pub was_submitting: bool,
    pub restore_path: Option<String>,
    pub reopen_path: bool,
    // The last left press on a chrome mark, so a release after a long press never navigates (EY-200).
    pub pressed_at: Option<std::time::Instant>,
}

impl Chrome {
    pub fn new(cx: &mut Context<Picker>) -> Self {
        Self {
            keyboard: Default::default(),
            preset: "default".into(),
            recent_back: None,
            probe: Default::default(),
            focused: None,
            rail_focus: None,
            back: cx.focus_handle(),
            up: cx.focus_handle(),
            filter: cx.focus_handle(),
            cancel: cx.focus_handle(),
            answer: cx.focus_handle(),
            new_folder: cx.focus_handle(),
            history: Vec::new(),
            backing: false,
            menu: None,
            menu_view: None,
            menu_closing: None,
            filter_subscription: None,
            filter_close: None,
            folder_request: None,
            folder: None,
            restore: None,
            was_submitting: false,
            restore_path: None,
            reopen_path: false,
            pressed_at: None,
        }
    }
}

impl Picker {
    pub fn user_set_view(&mut self, mode: &str, cx: &mut Context<Self>) {
        if self.accept_state().submitting()
            || self.chrome.folder_request.is_some()
            || self.browse.listing.read(cx).view_mode() == mode
        {
            return;
        }
        self.set_view(mode, cx);
        self.persist_picker_view(mode, cx);
    }

    // Any view set before the settings land wins over the remembered one.
    pub fn set_view(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.preferences.view_chosen = true;
        self.browse.listing.update(cx, |listing, cx| {
            listing.set_view(mode);
            cx.notify();
        });
        cx.notify();
    }
}
