use crate::i18n::I18n;
use crate::tui::{
    cells::{self, Run},
    model::Model,
};

pub use crate::tui::words::Text;

// A locale refresh re-reads held rows without restarting the preview or losing its scroll.
pub fn refresh_head(m: &mut Model) {
    if m.current_path().as_ref() != Some(&m.preview_path) {
        return;
    }
    let Some(row) = m.rows.get(&m.cursor) else {
        return;
    };
    if let Some(head) = m.preview.get_mut(1) {
        *head = format!(
            "{} · {}",
            row.kind,
            m.preview_state.i18n.locale().format_size(row.size as u64)
        );
    }
}

pub fn from_io(error: &std::io::Error) -> Text {
    match error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<Fault>())
    {
        Some(fault) => Text::Key(fault.key),
        None => match error
            .get_ref()
            .and_then(|cause| cause.downcast_ref::<std::io::Error>())
        {
            Some(inner) => from_io(inner),
            None => Text::Data(cells::clean(&crate::error::io_message(error))),
        },
    }
}

// The result channel keeps its existing diagnostic contract for other Job consumers.
#[derive(Debug)]
pub struct Fault {
    pub key: &'static str,
    legacy: &'static str,
}
impl Fault {
    pub fn new(key: &'static str, legacy: &'static str) -> Self {
        Self { key, legacy }
    }
}
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.legacy)
    }
}
impl std::error::Error for Fault {}
pub fn issue(key: &'static str, legacy: &'static str) -> std::io::Error {
    std::io::Error::other(Fault { key, legacy })
}

#[derive(Clone)]
pub struct Fact {
    pub key: &'static str,
    pub value: String,
    pub number: bool,
}
impl Fact {
    pub fn line(&self, i18n: &I18n) -> String {
        format!("{} · {}", i18n.tr(self.key), cells::clean(&self.value))
    }
    pub fn paint(&self, i18n: &I18n, width: usize) -> String {
        let label = i18n.tr(self.key);
        let label_width = cells::text_width(&label).min(width);
        let label = cells::ellipsis(&label, label_width);
        let room = width - label_width;
        let sep = cells::fit(" · ", room.min(3));
        let value = cells::ellipsis(&self.value, room.saturating_sub(3));
        if !i18n.locale().right_to_left {
            return format!("{label}{sep}{value}");
        }
        let value = cells::isolate(if self.number { Run::Number } else { Run::Name }, &value);
        format!("{value}{sep}{}", cells::isolate(Run::Name, &label))
    }
}
pub fn push(m: &mut Model, key: &'static str, value: String, number: bool) {
    let fact = Fact { key, value, number };
    m.preview_metadata.push(fact.line(&m.preview_state.i18n));
    m.preview_state.facts.push(fact);
}

// Metadata is rebuilt from held sources, never recovered by parsing translated labels.
pub fn metadata(m: &mut Model) {
    m.preview_metadata.clear();
    m.preview_state.facts.clear();
    if let Some(row) = m.rows.get(&m.cursor) {
        let modified = m
            .preview_state
            .i18n
            .locale()
            .format_date(row.modified)
            .filter(|_| row.modified > 0);
        if let Some(value) = modified {
            push(m, "preview.fact.modified", value, true);
        }
    }
    if let Some((w, h, ms, rate)) = m.preview_state.media {
        let locale = m.preview_state.i18n.locale();
        let dimensions = (w > 0 && h > 0).then(|| {
            m.preview_state.i18n.tr_args(
                "preview.pixels",
                &[
                    &locale.format_count(w as i64),
                    &locale.format_count(h as i64),
                ],
            )
        });
        let duration = (ms > 0)
            .then(|| crate::tui::media::localized_clock(ms as f64 / 1000., &m.preview_state.i18n));
        let rate = (rate > 0).then(|| {
            m.preview_state.i18n.tr_args(
                "preview.rate.khz",
                &[&locale.format_number(rate as f64 / 1000., 1)],
            )
        });
        for (key, value) in [
            ("model3d.fact.dimensions", dimensions),
            ("preview.fact.duration", duration),
            ("preview.fact.rate", rate),
        ] {
            if let Some(value) = value {
                push(m, key, value, true);
            }
        }
    }
    crate::tui::office::append_facts(m);
    crate::tui::model3d::append_facts(m);
}

#[cfg(test)]
pub fn fail(m: &mut Model, message: Text) {
    m.fail_text(message);
}

#[cfg(test)]
#[path = "../kind_language_tests.rs"]
mod kind_language_tests;
