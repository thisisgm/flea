//! The chooser's localized words, built once per language and request so paint never translates.
use flea::i18n::I18n;
use flea::ui_logic_pk1::{self as logic, Request, Text};
use gpui::SharedString;
use std::cell::RefCell;

#[derive(Clone, Debug, PartialEq)]
pub struct Labels {
    pub title: SharedString,
    pub requester: Option<SharedString>,
    pub cancel: SharedString,
    pub replace: SharedString,
    accept_base: SharedString,
    // The counted answer for the last mark count, so a repaint reuses it instead of translating again.
    counted: RefCell<Option<(usize, SharedString)>>,
    #[cfg(test)]
    pub(crate) built: std::cell::Cell<usize>,
    pub hints: SharedString,
    pub hints_unavailable: SharedString,
    pub hints_filtered: SharedString,
    pub hints_folder: SharedString,
}

fn text(catalog: &I18n, text: &Text) -> String {
    match text {
        Text::Caller(words) => words.clone(),
        Text::Key(key) => catalog.tr(key),
    }
}

impl Labels {
    pub fn new(catalog: &I18n, req: &Request) -> Self {
        Self {
            title: text(catalog, &logic::title(req)).into(),
            requester: logic::requester(req)
                .map(|app| catalog.tr_args("picker.portal.requested_by", &[app]).into()),
            cancel: catalog.tr("picker.portal.cancel").into(),
            replace: catalog.tr("picker.portal.replace").into(),
            accept_base: text(catalog, &logic::accept_label(req, 0).base).into(),
            counted: RefCell::new(None),
            #[cfg(test)]
            built: std::cell::Cell::new(0),
            hints: catalog.tr(logic::hints(req, false)).into(),
            hints_unavailable: catalog.tr(logic::hints(req, true)).into(),
            hints_filtered: flea::ui_logic_pk2::hints_filtered(catalog).into(),
            hints_folder: catalog.tr("picker.browser.hints_folder").into(),
        }
    }

    // The count goes in as an argument after the caller's own text, so a placeholder in that text is never filled.
    pub fn accept(&self, catalog: &I18n, req: &Request, marks: usize) -> SharedString {
        if !(req.multiple && marks > 1) {
            return self.accept_base.clone();
        }
        if let Some((cached, label)) = self.counted.borrow().as_ref() {
            if *cached == marks {
                return label.clone();
            }
        }
        #[cfg(test)]
        self.built.set(self.built.get() + 1);
        let count = catalog.locale().format_count(marks as i64);
        let label: SharedString = catalog
            .tr_args(
                "picker.portal.accept.counted",
                &[self.accept_base.as_ref(), &count],
            )
            .into();
        *self.counted.borrow_mut() = Some((marks, label.clone()));
        label
    }
}
