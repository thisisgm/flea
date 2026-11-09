//! Semantic held labels, projected through the frame's shared catalog.
use crate::i18n::I18n;

#[derive(Clone, Debug)]
pub enum Text {
    Data(String),
    Key(&'static str),
    Args(&'static str, Vec<Text>),
    Plural(&'static str, usize, Vec<Text>),
    Count(usize),
    Size(usize),
    Joined(Vec<Text>),
    // A backend cause set inside another sentence: a fixed one reads in the current language, anything else as the backend said it.
    Cause(String),
}
impl Text {
    pub fn render(&self, i18n: &I18n) -> String {
        match self {
            Self::Data(text) => text.clone(),
            Self::Key(key) => i18n.tr(key),
            Self::Args(key, args) | Self::Plural(key, _, args) => {
                let values: Vec<String> = args.iter().map(|arg| arg.render(i18n)).collect();
                let args: Vec<&str> = values.iter().map(String::as_str).collect();
                if let Self::Plural(_, count, _) = self {
                    i18n.trn_args(key, (*count).min(i64::MAX as usize) as i64, &args)
                } else {
                    i18n.tr_args(key, &args)
                }
            }
            Self::Count(count) => i18n
                .locale()
                .format_count((*count).min(i64::MAX as usize) as i64),
            Self::Size(bytes) => i18n.locale().format_size(*bytes as u64),
            Self::Joined(parts) => parts.iter().map(|part| part.render(i18n)).collect(),
            Self::Cause(words) => flea::ui_logic_status_feedback::errors::clause(words, i18n),
        }
    }
}
impl From<String> for Text {
    fn from(value: String) -> Self {
        Self::Data(value)
    }
}
impl From<&str> for Text {
    fn from(value: &str) -> Self {
        Self::Data(value.into())
    }
}
impl std::fmt::Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(not(test))]
        let i18n = I18n::default();
        #[cfg(test)]
        let i18n = crate::i18n::test_catalogs::english();
        f.write_str(&self.render(&i18n))
    }
}
impl std::error::Error for Text {}

#[derive(Clone, Copy)]
pub(super) enum ErrorSlot {
    Current,
    Queued,
}

#[derive(Default)]
pub struct Held {
    message: Option<(Text, String)>,
    error: Option<Text>,
    errors: std::collections::VecDeque<Option<Text>>,
    transfer: Option<(Text, String)>,
    launches: std::collections::BTreeMap<u32, (Text, String)>,
}
impl Held {
    #[cfg(test)]
    fn retained_errors(&self) -> usize {
        usize::from(self.error.is_some()) + self.errors.iter().filter(|text| text.is_some()).count()
    }
    #[cfg(test)]
    pub fn clear_message(&mut self) {
        self.message = None;
    }
    pub(super) fn sync_errors(&mut self, count: usize) {
        self.errors.resize_with(count, || None);
    }
    pub(super) fn error(&mut self, slot: ErrorSlot, text: Option<Text>) {
        match slot {
            ErrorSlot::Current => self.error = text,
            ErrorSlot::Queued => self.errors.push_back(text),
        }
    }
    pub(super) fn retract_errors(
        &mut self,
        errors: &mut std::collections::VecDeque<String>,
        gone: &[(String, bool)],
    ) {
        self.sync_errors(errors.len());
        let mut retained = std::collections::VecDeque::new();
        errors.retain(|error| {
            let text = self.errors.pop_front().flatten();
            let keep = !gone.iter().any(|(posted, _)| posted == error);
            if keep {
                retained.push_back(text);
            }
            keep
        });
        self.errors = retained;
    }
    pub(super) fn dismiss_error(&mut self) {
        self.error = self.errors.pop_front().flatten();
    }
    pub fn message(&mut self, text: Text, i18n: &I18n) -> String {
        Self::set(&mut self.message, text, i18n)
    }
    pub fn transfer(&mut self, text: Text, i18n: &I18n) -> String {
        Self::set(&mut self.transfer, text, i18n)
    }
    pub fn launched(&mut self, id: u32, text: Text, i18n: &I18n) -> String {
        let label = text.render(i18n);
        self.launches.insert(id, (text, label.clone()));
        label
    }
    pub fn launch_text(&mut self, id: u32, fallback: &str) -> Text {
        self.launches
            .remove(&id)
            .map(|(text, _)| text)
            .unwrap_or_else(|| fallback.into())
    }
    pub fn refresh_launches(
        &mut self,
        i18n: &I18n,
        launches: &mut [(String, std::process::Child)],
    ) {
        self.launches
            .retain(|id, _| launches.iter().any(|(_, child)| child.id() == *id));
        for (name, child) in launches {
            if let Some((text, before)) = self.launches.get_mut(&child.id()) {
                if name == before {
                    *name = text.render(i18n);
                    *before = name.clone();
                }
            }
        }
    }
    fn set(slot: &mut Option<(Text, String)>, text: Text, i18n: &I18n) -> String {
        let rendered = text.render(i18n);
        *slot = Some((text, rendered.clone()));
        rendered
    }
    pub fn refresh(
        &mut self,
        i18n: &I18n,
        message: &mut String,
        error: &mut String,
        errors: &mut std::collections::VecDeque<String>,
        transfer: &mut String,
    ) {
        for (slot, value) in [(&mut self.message, message), (&mut self.transfer, transfer)] {
            if let Some((text, before)) = slot {
                if value == before {
                    *value = text.render(i18n);
                    *before = value.clone();
                } else {
                    *slot = None;
                }
            }
        }
        if let Some(text) = &self.error {
            *error = text.render(i18n);
        }
        self.sync_errors(errors.len());
        for (value, text) in errors.iter_mut().zip(&self.errors) {
            if let Some(text) = text {
                *value = text.render(i18n);
            }
        }
    }
}
pub fn error(key: &'static str) -> std::io::Error {
    std::io::Error::other(Text::Key(key))
}
pub fn args(key: &'static str, args: Vec<Text>) -> Text {
    Text::Args(key, args)
}
pub fn items(i18n: &I18n, count: usize) -> String {
    Text::Plural("items.count", count, vec![]).render(i18n)
}

impl super::model::Model {
    #[allow(
        dead_code,
        reason = "I50-11 terminal active-layout identity adapter is an outstanding owner dependency"
    )]
    pub fn set_keycaps(
        &mut self,
        snapshot: flea::keys::display::KeycapSnapshot,
        map: &super::keymap::Map,
    ) -> Result<(), flea::keys::display::KeycapError> {
        if self.keycaps == Some(snapshot) {
            return Ok(());
        }
        let labels = super::render::key_sheet::resolve(self, map, &snapshot)?;
        let mut hints = super::keymap::Hints::new(map, &self.preset);
        for hint in [
            &mut hints.keys,
            &mut hints.load_preview,
            &mut hints.delete_permanently,
            &mut hints.trash,
            &mut hints.undo,
        ] {
            if !hint.is_empty() {
                *hint = flea::keys::display::chord_legend(
                    &super::render::key_sheet::lowercase(hint),
                    &snapshot,
                )?
                .text;
            }
        }
        self.keycap_labels = labels;
        self.keycaps = Some(snapshot);
        self.hints = hints;
        super::preview::refresh_locale(self);
        Ok(())
    }
    pub fn poll_launches(&mut self) {
        let mut launch_errors = Vec::new();
        let words = &mut self.words;
        self.launches
            .retain_mut(|(name, child)| match child.try_wait() {
                Ok(Some(status)) => {
                    let label = words.launch_text(child.id(), name);
                    if !status.success() {
                        launch_errors.push(args(
                            "tui.launch.could_not_finish",
                            vec![label, status.to_string().into()],
                        ));
                    }
                    false
                }
                Ok(None) => true,
                Err(error) => {
                    launch_errors.push(args(
                        "tui.launch.could_not_observe",
                        vec![
                            words.launch_text(child.id(), name),
                            crate::error::io_message(&error).into(),
                        ],
                    ));
                    false
                }
            });
        for error in launch_errors {
            self.fail_text(error);
        }
    }
    pub fn launched(&mut self, key: &'static str, child: std::process::Child) {
        let label = self.words.launched(child.id(), Text::Key(key), &self.i18n);
        self.launches.push((label, child));
    }
    pub fn say_text(&mut self, text: Text) {
        self.message = self.words.message(text, &self.i18n);
        self.message_at = std::time::Instant::now();
    }
    pub fn fail_text(&mut self, text: Text) {
        let rendered = text.render(&self.i18n);
        if let Some(slot) = self.enqueue_error(rendered) {
            self.words.error(slot, Some(text));
        }
    }
    pub fn transfer_text(&mut self, text: Text) {
        self.transfer = self.words.transfer(text, &self.i18n);
    }
    pub fn say_key(&mut self, key: &'static str) {
        self.say_text(Text::Key(key));
    }
    pub fn fail_key(&mut self, key: &'static str) {
        self.fail_text(Text::Key(key));
    }
    pub fn fail_io(&mut self, error: &std::io::Error) {
        if let Some(text) = error
            .get_ref()
            .and_then(|error| error.downcast_ref::<Text>())
        {
            self.fail_text(text.clone());
        } else if let Some(fault) = error
            .get_ref()
            .and_then(|cause| cause.downcast_ref::<super::preview::words::Fault>())
        {
            self.fail_key(fault.key);
        } else {
            self.fail(crate::error::io_message(error));
        }
    }
    pub fn refresh_frame_locale(&mut self) -> bool {
        self.refresh_locale(&[
            super::preview::refresh_locale,
            super::render::cards::refresh_locale,
        ])
    }
    // The caller supplies projection callbacks; the transaction emits no terminal bytes or backend requests.
    pub fn refresh_locale(&mut self, refresh: &[fn(&mut Self)]) -> bool {
        if !self.i18n.poll_changed() {
            return false;
        }
        self.prompts.i18n = self.i18n.clone();
        let old_errors: Vec<String> = std::iter::once(self.error.clone())
            .chain(self.errors.iter().cloned())
            .collect();
        self.words.refresh(
            &self.i18n,
            &mut self.message,
            &mut self.error,
            &mut self.errors,
            &mut self.transfer,
        );
        let new_errors: Vec<String> = std::iter::once(self.error.clone())
            .chain(self.errors.iter().cloned())
            .collect();
        for (posted, _) in &mut self.preview_errors {
            if let Some(index) = old_errors.iter().position(|old| old == posted) {
                *posted = new_errors[index].clone();
            }
        }
        super::trashview::refresh_locale(self);
        self.words.refresh_launches(&self.i18n, &mut self.launches);
        if let Some(editor) = &mut self.editor {
            editor.refresh_locale(&self.i18n);
        }
        if !self.search.is_empty() {
            self.search = super::prompts::found(self, self.search_scanned);
        }
        for project in refresh {
            project(self);
        }
        self.locale_revision = self.locale_revision.wrapping_add(1);
        true
    }
}

#[path = "words_title.rs"]
pub(super) mod title;

#[cfg(test)]
#[path = "words_layout_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "words_caller_tests.rs"]
mod caller_tests;

#[cfg(test)]
#[path = "words_preview_join_tests.rs"]
mod preview_join_tests;

#[cfg(test)]
#[path = "ar_tui_copy_tests.rs"]
mod ar_tui_copy_tests;
