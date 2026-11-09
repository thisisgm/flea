//! The presses that ask for an answer and the check results that complete one.
use super::picker::Picker;
use flea::ui_logic_picker_portal::{
    self as logic, AcceptState, AcceptStep, Check, CheckResult, DoubleAction, Mark, SaveReview,
};
use gpui::Context;

impl Picker {
    fn composing(&self, cx: &gpui::App) -> bool {
        self.find.entry.marked.is_some()
            || self.browse.path_view(cx).read(cx).entry.marked.is_some()
            || self
                .browse
                .filename
                .as_ref()
                .is_some_and(|field| field.read(cx).entry.marked.is_some())
    }

    pub fn accept_state(&self) -> AcceptState<'_> {
        AcceptState {
            req: &self.req,
            backend_unavailable: self.backend_unavailable(),
            accept_marks: self.accept_marks,
            mark_request: self.mark_request,
            review_request: self.review_request,
            marks: self.marks.len(),
            location: &self.listing.location,
            listing_loading: self.listing.state == "loading",
            cursor_file: self.listing.cursor_file.as_deref(),
            save_name: &self.listing.save_name,
            review: self.review.as_ref(),
            review_asked: self.review_asked,
        }
    }

    pub fn can_accept(&self) -> bool {
        logic::can_accept(&self.accept_state())
    }

    // PickerSave.qml's status line in the collision review, with the wrap joints it draws.
    pub fn collision_line(&self) -> Option<String> {
        self.accept_state().collision_review().then(|| {
            let jointed = flea::ui_logic_picker_listing::joint_name(&self.listing.save_name);
            flea::ui_logic_picker_listing::collision_short(&self.locale.catalog, &jointed)
        })
    }

    // PickerSave.qml's statusLine: the backend's refusal first, else the review's sentence.
    pub fn save_status(&self) -> Option<String> {
        if self.save_error.is_empty() {
            self.collision_line()
        } else {
            Some(self.save_error.clone())
        }
    }

    pub fn accept(&mut self, cx: &mut Context<Self>) {
        if self.answered() || self.composing(cx) {
            return;
        }
        match logic::accept_step(&self.accept_state()) {
            AcceptStep::Busy | AcceptStep::Reviewing | AcceptStep::Nothing => {}
            AcceptStep::SaveNameEmpty => self.say_key("picker.portal.save_name_empty", false, cx),
            AcceptStep::SaveNameRefused => {
                self.say_key("picker.portal.save_name_refused", false, cx)
            }
            AcceptStep::ProbeSave => self.probe_save(),
            AcceptStep::AskReview => {
                self.review_asked = true;
                self.stem_requests += 1;
            }
            AcceptStep::SubmitSave => self.submit_save(),
            AcceptStep::RecentFolder => self.say_key("picker.portal.recent_folder", false, cx),
            AcceptStep::MarkFolder(path) => self.accept_mark(&path, true, false),
            AcceptStep::MarkCursor(path) => self.accept_mark(&path, false, false),
            AcceptStep::ValidateMarks => self.validate_marks(true),
        }
        cx.notify();
    }

    // Replace is the only answer that overwrites; nothing but its own activation arrives here.
    pub fn replace(&mut self, cx: &mut Context<Self>) {
        if self.composing(cx) {
            return;
        }
        if logic::replace_allowed(&self.accept_state()) {
            self.submit_save();
            cx.notify();
        }
    }

    // The listing owner resolves Open and Mark; this answers the two double clicks that send.
    pub fn double_activate(
        &mut self,
        row_directory: Option<bool>,
        row_path: &str,
        first_path: &str,
        cx: &mut Context<Self>,
    ) -> DoubleAction {
        let marked = self.marks.iter().any(|mark| mark.path == row_path);
        let action = logic::double_action(&self.req, row_directory, row_path, first_path, marked);
        if !matches!(action, DoubleAction::Accept | DoubleAction::MarkAccept) {
            return action;
        }
        let state = self.accept_state();
        if state.backend_unavailable || state.mark_request > 0 || state.submitting() {
            self.say_key("picker.portal.selection_busy", false, cx);
        } else if action == DoubleAction::Accept {
            self.accept(cx);
        } else {
            self.accept_mark(row_path, false, self.req.multiple);
        }
        cx.notify();
        action
    }

    pub(super) fn accept_mark(&mut self, path: &str, directory: bool, multiple: bool) {
        self.accept_marks = true;
        self.mark_request = self.check(&Check::Mark {
            path,
            directory,
            multiple,
        });
    }

    pub(super) fn validate_marks(&mut self, accepting: bool) {
        if self.mark_request > 0 {
            self.selection.dirty = true;
            return;
        }
        if self.backend_unavailable()
            || (self.marks.is_empty() && self.mark_request == 0)
            || self.mark_request > 0
        {
            return;
        }
        self.accept_marks = accepting;
        self.mark_request = self.check(&Check::Validate);
    }

    fn submit_save(&mut self) {
        let review = self.review.as_ref().map_or(0, |review| review.review);
        self.review_request = self.check(&Check::Review { review });
    }

    // The listing owner calls this when the folder or the name changes, so a review never outlives its name.
    pub fn invalidate_save(&mut self, cx: &mut Context<Self>) {
        self.review = None;
        self.review_asked = false;
        self.save_error.clear();
        self.probe_save();
        cx.notify();
    }

    pub(super) fn probe_save(&mut self) {
        let listing = &self.listing;
        if self.backend_unavailable()
            || !logic::saving(&self.req)
            || self.save_request > 0
            || listing.location.is_empty()
            || !logic::valid_name(&listing.save_name)
        {
            return;
        }
        let (folder, name) = (listing.location.clone(), listing.save_name.clone());
        self.save_request = self.check(&Check::Save {
            folder: &folder,
            name: &name,
        });
        self.probe = Some((folder, name));
    }

    pub fn received(&mut self, result: CheckResult, cx: &mut Context<Self>) {
        if self.answered() || self.backend_unavailable() {
            return;
        }
        if result.id == self.mark_request {
            self.marked(result, cx);
        } else if result.id == self.save_request {
            self.save_request = 0;
            let probed = self.probe.take();
            if probed.as_ref()
                != Some(&(
                    self.listing.location.clone(),
                    self.listing.save_name.clone(),
                ))
            {
                self.probe_save();
            } else if !result.ok {
                self.save_error = result.error.clone();
                self.say(result.error, true, cx);
            } else if let Some((folder, name)) = probed {
                self.review = Some(SaveReview {
                    folder,
                    name,
                    collision: result.collision,
                    review: result.review,
                    path: result.path,
                });
            }
        } else if result.id == self.review_request {
            self.review_request = 0;
            let ready = self.accept_state().save_ready();
            if !result.ok || !ready {
                let text = if result.error.is_empty() {
                    self.locale.catalog.tr("picker.portal.review_stale")
                } else {
                    result.error
                };
                self.say(text, true, cx);
                self.invalidate_save(cx);
            } else {
                self.finish_ok(&[logic::file_uri(&result.path)], cx);
            }
        }
        self.relist_settings(cx);
        cx.notify();
    }

    fn marked(&mut self, result: CheckResult, cx: &mut Context<Self>) {
        self.mark_request = 0;
        let accepting = std::mem::take(&mut self.accept_marks);
        if !result.ok {
            let selecting = result.op == "select";
            self.end_range();
            self.selection_failed();
            self.say(result.error, true, cx);
            if selecting {
                self.validate_marks(false);
            }
            return;
        }
        self.marks = Mark::reviewed(&self.marks, &result.marks);
        self.mark_changed(cx);
        if result.removed > 0 || result.skipped > 0 {
            self.selection_feedback(&result, cx);
            self.selection_notice = Some(result);
        } else if accepting && !self.marks.is_empty() {
            let uris: Vec<String> = self.marks.iter().map(|mark| mark.uri.clone()).collect();
            self.finish_ok(&uris, cx);
        }
        self.selection_settled(cx);
    }
}
