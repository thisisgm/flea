//! Retained disk facts and the answered state of the currently committed directory.
#[derive(Default)]
pub(crate) struct Filesystem {
    pub path: String,
    pub(super) listing: usize,
    pub answered: bool,
    /// Retained across a folder change; only an accepted answer for the shown folder replaces it.
    pub facts: Option<crate::backend::status::Filesystem>,
}

impl Filesystem {
    pub(crate) fn enter(&mut self, path: &str) {
        if self.path != path {
            self.path = path.into();
            self.listing = 0;
            self.answered = false;
        }
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
            listing,
            answered: true,
            facts: Some(facts),
        }
    }
}
