//! One portal request's window state: the answer, the reply on disk, then both children, then exit.
use super::labels::Labels;
use super::process::{Line, PickerChild};
use super::reply;
use crate::window::locale::Snapshot;
use flea::ui_logic_picker_portal::{
    self as logic, check_result, Child, Effect, Lifetime, Mark, Request, SaveReview,
};
use gpui::{AnyView, App, Context, FocusHandle, SharedString, Task};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

// The listing owner's facts about what the window stands on; the answer reads them and never writes them.
#[derive(Clone, Debug, PartialEq)]
pub struct ListingFacts {
    pub location: String,
    pub total: usize,
    pub shown_total: usize,
    pub held: usize,
    pub names: Vec<String>,
    pub cursor: usize,
    pub state: &'static str,
    pub failed: bool,
    // The cursor row's name while the listing is ready and the cursor stands on a file.
    pub cursor_file: Option<String>,
    pub chip: i64,
    pub save_name: String,
}

impl ListingFacts {
    pub fn start(req: &Request, home: &str) -> Self {
        Self {
            location: logic::start_folder(req, home).to_owned(),
            total: 0,
            shown_total: 0,
            held: 0,
            names: Vec::new(),
            cursor: 0,
            state: "loading",
            failed: false,
            cursor_file: None,
            chip: logic::current_chip(req),
            save_name: logic::initial_save_name(req).to_owned(),
        }
    }
    pub fn row(&self, index: usize) -> Option<&str> {
        index
            .checked_sub(self.held)
            .and_then(|at| self.names.get(at))
            .map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub text: SharedString,
    pub error: bool,
}

pub struct Picker {
    pub req: Request,
    pub(super) preferences: super::settings::Preferences,
    pub chrome: super::chrome::Chrome,
    pub selection: super::selection::Selection,
    pub(super) lifetime: Lifetime,
    pub listing: ListingFacts,
    pub marks: Vec<Mark>,
    pub(super) accept_marks: bool,
    pub(super) mark_request: u64,
    pub(super) review_request: u64,
    pub(super) save_request: u64,
    pub(super) probe: Option<(String, String)>,
    pub(super) review: Option<SaveReview>,
    pub(super) review_asked: bool,
    pub(super) save_error: String,
    next_check: u64,
    pub message: Option<Message>,
    message_task: Option<Task<()>>,
    pub(super) locale: Snapshot,
    pub labels: Labels,
    pub(super) checks: Box<dyn PickerChild>,
    pub(super) listing_child: Box<dyn PickerChild>,
    reply_path: PathBuf,
    pub focus: FocusHandle,
    exit: Rc<dyn Fn(&mut App)>,
    _watchers: [Task<()>; 2],
    _kept: Vec<Task<()>>,
    // Producer slots: the listing body and the filter belong to the listing owner.
    pub body: Option<AnyView>,
    pub filter: Option<AnyView>,
    // A mark answer that removed or skipped items is the selection owner's to word; the answer waits for it.
    pub selection_notice: Option<logic::CheckResult>,
    // Bumped when a taken save name starts its review, so the save field's owner selects the stem.
    pub stem_requests: u64,
    // The folder, path strip and Filename the listing child feeds.
    pub browse: super::browse::Browse,
    pub(super) drawn: super::drawn::Drawn,
    // A column title pressed in List and not yet released, and where the press was.
    pub(super) header_tap: Option<(&'static str, gpui::Point<gpui::Pixels>)>,
    // The name filter "/" opens over the listing.
    pub(super) find: super::find::Find,
    // The places rail beside the listing, one flat list as picker-open draws it.
    pub rail: super::rail::Rail,
}

pub struct Children {
    pub checks: Box<dyn PickerChild>,
    pub listing: Box<dyn PickerChild>,
}

impl Picker {
    pub fn new(
        req: Request,
        reply_path: PathBuf,
        home: &str,
        locale: Snapshot,
        children: Children,
        cx: &mut Context<Self>,
    ) -> Self {
        let labels = Labels::new(&locale.catalog, &req);
        let browse = super::browse::Browse::new(&req, home, &locale.catalog, cx);
        let typeface = cx
            .try_global::<crate::list::Typeface>()
            .cloned()
            .unwrap_or_else(crate::list::Typeface::omarchy_default);
        let watchers = [
            Self::watch(children.checks.lines(), Child::Checks, cx),
            Self::watch(children.listing.lines(), Child::Listing, cx),
        ];
        let mut picker = Self {
            listing: ListingFacts::start(&req, home),
            req,
            preferences: Default::default(),
            chrome: super::chrome::Chrome::new(cx),
            selection: Default::default(),
            lifetime: Lifetime::default(),
            marks: Vec::new(),
            accept_marks: false,
            mark_request: 0,
            review_request: 0,
            save_request: 0,
            probe: None,
            review: None,
            review_asked: false,
            save_error: String::new(),
            next_check: 0,
            message: None,
            message_task: None,
            locale,
            labels,
            checks: children.checks,
            listing_child: children.listing,
            reply_path,
            focus: cx.focus_handle(),
            exit: Rc::new(|cx: &mut App| cx.quit()),
            _watchers: watchers,
            _kept: Vec::new(),
            body: None,
            filter: None,
            selection_notice: None,
            stem_requests: 0,
            browse,
            drawn: super::drawn::Drawn::default(),
            header_tap: None,
            find: super::find::Find::new(typeface),
            rail: super::rail::Rail::new(home, cx.focus_handle()),
        };
        picker.preferences.followed.hidden = cx
            .try_global::<super::settings::HiddenSeed>()
            .is_some_and(|seed| seed.0);
        picker.chrome.rail_focus = Some(picker.rail.focus.clone());
        picker.rebuild_rail(cx);
        picker.body = Some(picker.browse.body(cx));
        picker.filter = picker.browse.filename.clone().map(AnyView::from);
        let start = picker.listing.location.clone();
        picker.open_folder(&start, cx);
        picker.sync_path(cx);
        picker
    }

    // Tests and hosts that own their own process exit replace the default quit.
    pub fn on_exit(&mut self, exit: impl Fn(&mut App) + 'static) {
        self.exit = Rc::new(exit);
    }

    // A task the window's lifetime owns, such as its locale reader.
    pub fn keep(&mut self, task: Task<()>) {
        self._kept.push(task);
    }

    fn watch(
        lines: async_channel::Receiver<Line>,
        child: Child,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while let Ok(line) = lines.recv().await {
                let stop = line == Line::Stopped;
                let kept = this.update(cx, |picker, cx| match line {
                    Line::Reply(text) if child == Child::Checks => {
                        if picker.folder_reply(&text, cx) {
                            return;
                        }
                        if let Some(result) = check_result(&text) {
                            picker.received(result, cx);
                        }
                    }
                    Line::Reply(text) => picker.listing_line(&text, cx),
                    Line::Failed(_) if child == Child::Checks => picker.backend_failed(cx),
                    Line::Failed(_) => picker.listing_failed(cx),
                    Line::Stopped => picker.child_stopped(child, cx),
                });
                if stop || kept.is_err() {
                    return;
                }
            }
            // Lines that end without Stopped (a reader that never started) still count as the child's stop.
            let _ = this.update(cx, |picker, cx| picker.child_stopped(child, cx));
        })
    }

    pub fn answered(&self) -> bool {
        self.lifetime.answered()
    }
    pub fn backend_unavailable(&self) -> bool {
        self.lifetime.backend_unavailable()
    }
    pub fn exited(&self) -> bool {
        self.lifetime.exited()
    }

    pub fn set_locale(&mut self, locale: Snapshot, cx: &mut Context<Self>) {
        if locale.revision != self.locale.revision {
            self.labels = Labels::new(&locale.catalog, &self.req);
            self.locale = locale;
            self.rebuild_rail(cx);
            if let Some(network) = &self.rail.network {
                network.shares.update(cx, |shares, cx| {
                    shares.set_catalog(self.locale.catalog.clone(), cx)
                });
            }
            cx.notify();
        }
    }

    pub fn say(&mut self, text: impl Into<SharedString>, error: bool, cx: &mut Context<Self>) {
        self.message = Some(Message {
            text: text.into(),
            error,
        });
        self.message_task = (!error).then(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(logic::MESSAGE_LIFE_MS))
                    .await;
                let _ = this.update(cx, |picker, cx| {
                    picker.message = None;
                    cx.notify();
                });
            })
        });
        cx.notify();
    }

    pub(super) fn say_key(&mut self, key: &str, error: bool, cx: &mut Context<Self>) {
        let text = self.locale.catalog.tr(key);
        self.say(text, error, cx);
    }

    pub(super) fn check(&mut self, check: &logic::Check) -> u64 {
        self.next_check += 1;
        let _ = self.checks.send(&logic::check_line(self.next_check, check));
        self.next_check
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        let effects = self.lifetime.cancel();
        self.run(effects, cx);
    }

    pub(super) fn finish_ok(&mut self, uris: &[String], cx: &mut Context<Self>) {
        let effects = self
            .lifetime
            .finish(logic::RESPONSE_OK, uris, Some(self.listing.chip));
        self.run(effects, cx);
    }

    // True when the window may close now; otherwise the close became a refusal and the exit follows the reply.
    pub fn window_closed(&mut self, cx: &mut Context<Self>) -> bool {
        let effects = self.lifetime.window_closed();
        self.run(effects, cx);
        self.lifetime.exited()
    }

    pub fn backend_failed(&mut self, cx: &mut Context<Self>) {
        if !self.lifetime.backend_failed() {
            return;
        }
        self.mark_request = 0;
        self.save_request = 0;
        self.review_request = 0;
        self.accept_marks = false;
        self.review = None;
        self.save_error = self
            .locale
            .catalog
            .tr("picker.portal.backend_stopped_short");
        self.say_key("picker.portal.backend_stopped", true, cx);
    }

    fn child_stopped(&mut self, child: Child, cx: &mut Context<Self>) {
        let effects = self.lifetime.child_stopped(child);
        self.run(effects, cx);
    }

    fn run(&mut self, effects: Vec<Effect>, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                Effect::CloseChecks => {
                    let _ = self.checks.send(logic::CLOSE_LINE);
                }
                Effect::WriteReply(text) => {
                    let path = self.browse.path_view(cx);
                    path.update(cx, |path, cx| path.close(cx));
                    self.browse.route = Default::default();
                    self.find.entry.close();
                    if let Some(field) = self.browse.filename.clone() {
                        field.update(cx, |field, cx| {
                            field.entry.close();
                            cx.notify();
                        });
                    }
                    self.write_reply(text, cx);
                }
                Effect::QuitChildren => {
                    self.stop_places(cx);
                    self.checks.quit();
                    self.listing_child.quit();
                }
                Effect::Exit => (self.exit.clone())(cx),
            }
        }
        cx.notify();
    }

    // The write runs on the background executor; the children are quit only once it has finished, saved or failed.
    fn write_reply(&mut self, text: String, cx: &mut Context<Self>) {
        let path = self.reply_path.clone();
        let written = cx
            .background_executor()
            .spawn(async move { reply::write(&path, &text) });
        cx.spawn(async move |this, cx| {
            let result = written.await;
            let _ = this.update(cx, |picker, cx| {
                if result.is_err() {
                    eprintln!("flea: the portal reply could not be written, so the request fails rather than reporting a refusal");
                }
                let effects = picker.lifetime.reply_written();
                picker.run(effects, cx);
            });
        })
        .detach();
    }
}
