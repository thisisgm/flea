//! Settings > View's two empty folder rows as one window-wide value, and how a hero follows it.
use super::super::caption::{caseless, tracking_for};
use super::super::now_ms;
use super::EmptyHero;
use flea::i18n::I18n;
use flea::jsondoc::Json;
use flea::ui_logic_status_feedback::empty::PHRASES;
use gpui::{App, Context, Global, SharedString};

/// Show Omi in empty folders and Rotate empty folder messages; each turns off alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptyLook {
    pub omi: bool,
    pub rotate: bool,
}

/// Both rows ticked draws the empty folder as 0.4.1 ships it.
impl Default for EmptyLook {
    fn default() -> Self {
        Self {
            omi: true,
            rotate: true,
        }
    }
}

impl Global for EmptyLook {}

impl EmptyLook {
    /// An absent or unreadable key reads ticked.
    pub fn read(data: &Json) -> Self {
        let on = |key: &str| data.get(key).and_then(Json::as_bool) != Some(false);
        Self {
            omi: on("emptyFolderOmi"),
            rotate: on("emptyFolderRotate"),
        }
    }

    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    /// Only a change reaches the heroes' observers.
    pub fn apply(data: &Json, cx: &mut App) {
        let look = Self::read(data);
        if Self::current(cx) != look {
            cx.set_global(look);
        }
    }
}

impl EmptyHero {
    /// Phrases are translated, raised and tracked once per language (caseless scripts neither), never in render or paint.
    pub fn translated(&mut self, catalog: &I18n) {
        self.shaped = None;
        let raised = !caseless(catalog.language());
        self.tracking = tracking_for(catalog.language());
        let raise = |key: &str| -> SharedString {
            let phrase = catalog.tr(key);
            if raised {
                phrase.to_uppercase().into()
            } else {
                phrase.into()
            }
        };
        self.rotating = PHRASES.iter().map(|key| raise(key)).collect();
        self.fixed = raise("pane.empty.fixed");
        self.phrases = self.active_phrases();
    }

    pub fn look(&self) -> EmptyLook {
        self.look
    }

    /// Unticked messages hold EMPTY FOLDER alone, so every width and every shaped beat reads one phrase.
    fn active_phrases(&self) -> Vec<SharedString> {
        if self.look.rotate {
            self.rotating.clone()
        } else {
            vec![self.fixed.clone()]
        }
    }

    pub(super) fn observe_look(&mut self, cx: &mut Context<Self>) {
        self.look_changed(EmptyLook::current(cx), cx);
        self.look_watch = Some(cx.observe_global::<EmptyLook>(|hero, cx| {
            hero.look_changed(EmptyLook::current(cx), cx);
        }));
    }

    fn look_changed(&mut self, look: EmptyLook, cx: &mut Context<Self>) {
        let held = !look.rotate;
        if self.look == look && self.rotation.held() == held {
            return;
        }
        self.look = look;
        self.timer = None;
        self.shaped = None;
        self.rotation.set_held(held, now_ms(self.base, cx));
        self.phrases = self.active_phrases();
        cx.notify();
    }
}
