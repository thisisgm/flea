//! Retained disk facts and the answered state of the currently committed directory.
#[derive(Default)]
pub(crate) struct Filesystem {
    pub path: String,
    /// Listing the shown folder's facts were asked for; None (never asked) still asks.
    pub(super) listing: Option<usize>,
    pub answered: bool,
    pub system: bool,
    /// Retained across a folder change; only an accepted answer for the shown folder replaces it.
    pub facts: Option<crate::backend::status::Filesystem>,
}

impl Filesystem {
    pub(crate) fn can_tag(&self) -> bool {
        self.facts
            .as_ref()
            .filter(|facts| facts.path == self.path)
            .and_then(|facts| facts.tagable)
            != Some(false)
    }

    pub(crate) fn enter(&mut self, path: &str) {
        if self.path != path {
            self.path = path.into();
            self.listing = None;
            self.answered = false;
            self.system = false;
        }
    }

    /// A refused listing or failed backend answers with no facts; retained facts stay.
    pub(crate) fn settle_without_facts(&mut self) {
        self.answered = true;
    }

    /// Lands an fsinfo answer on the shown folder; an empty path is the backend's unmoved base.
    pub(crate) fn answer(
        &mut self,
        shown: &str,
        mut facts: crate::backend::status::Filesystem,
    ) -> bool {
        if facts.path != shown && !facts.path.is_empty() {
            return false;
        }
        facts.path = shown.to_owned();
        self.enter(shown);
        self.answered = true;
        self.facts = Some(facts);
        true
    }
}

#[cfg(test)]
impl Filesystem {
    /// A ready fixture can begin after the matching listing's filesystem answer has settled.
    pub(crate) fn settled_fixture(
        facts: crate::backend::status::Filesystem,
        listing: usize,
    ) -> Self {
        Self {
            path: facts.path.clone(),
            listing: Some(listing),
            answered: true,
            system: false,
            facts: Some(facts),
        }
    }
}
