// The TUI status row uses the newest unfinished job head and the existing jobs suffix.
pub(super) fn line(i18n: &crate::i18n::I18n, head: &str, unfinished: usize) -> String {
    if unfinished == 0 {
        String::new()
    } else if unfinished == 1 {
        head.into()
    } else {
        let count = i18n.trn("jobs.count", unfinished as i64);
        let suffix = i18n.tr_args("jobs.suffix", &[&count]);
        format!("{head} {suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_job_status_keeps_the_backend_head_and_unfinished_count() {
        let i18n = crate::i18n::I18n::default();
        assert_eq!(line(&i18n, "Extracting 7 of 18", 1), "Extracting 7 of 18");
        assert_eq!(
            line(&i18n, "Extracting 7 of 18", 3),
            "Extracting 7 of 18 · 3 jobs"
        );
        assert_eq!(line(&i18n, "Extracting 7 items", 1), "Extracting 7 items");
        assert_eq!(line(&i18n, "", 0), "");
    }
}

#[cfg(test)]
mod model_tests {
    use crate::jsondoc::{parse, Json};
    use crate::tui::model::Model;
    use crate::tui::wire::{echo_wire, finish};
    use std::path::PathBuf;

    #[test]
    fn extraction_status_uses_newest_job_and_legacy_progress_cannot_replace_it() {
        let (mut wire, reader) = echo_wire();
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        // The legacy transfer shares id 12, so only the job_status guard can keep the head below.
        model.receive(parse(r#"{"t":"transferstarted","id":12,"n":3}"#).unwrap(), &mut wire).unwrap();
        model
            .receive(
                parse(r#"{"t":"jobs","newest":12,"unfinished":3,"head":"Extracting 7 of 18"}"#)
                    .unwrap(),
                &mut wire,
            )
            .unwrap();
        assert_eq!(model.transfer, "Extracting 7 of 18 · 3 jobs");
        assert_eq!(model.active_operation(), 12);
        model
            .receive(
                parse(r#"{"t":"transferprogress","id":12,"index":0}"#).unwrap(),
                &mut wire,
            )
            .unwrap();
        assert_eq!(model.transfer, "Extracting 7 of 18 · 3 jobs");
        model.receive(parse(r#"{"t":"job","job":{"id":12,"kind":"extract","state":"running","head":"Extracting 8 of 18"},"unfinished":2}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.transfer, "Extracting 8 of 18 · 2 jobs");
        model
            .receive(
                parse(r#"{"t":"jobs","newest":null,"unfinished":0,"head":""}"#).unwrap(),
                &mut wire,
            )
            .unwrap();
        assert_eq!(model.transfer, "");
        assert_eq!(model.active_operation(), 12);
        model.receive(parse(r#"{"t":"transferprogress","id":12,"index":1}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.transfer, "Copying 2 of 3", "with no job active the legacy event draws");
        finish(wire, reader);
    }

    #[test]
    fn an_older_extract_failure_shows_once_while_a_newer_job_owns_progress() {
        let (mut wire, reader) = echo_wire();
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        for line in [
            r#"{"t":"jobs","newest":12,"unfinished":1,"head":"Extracting A"}"#,
            r#"{"t":"jobs","newest":13,"unfinished":2,"head":"Extracting B"}"#,
            r#"{"t":"job","job":{"id":12,"kind":"extract","state":"failed","reason":"A.zip is corrupt"},"unfinished":1}"#,
            r#"{"t":"jobs","newest":13,"unfinished":1,"head":"Extracting B"}"#,
            r#"{"t":"archivedone","id":12,"ok":false,"verified":true,"err":"A.zip is corrupt"}"#,
        ] {
            model.receive(parse(line).unwrap(), &mut wire).unwrap();
        }
        assert_eq!((model.active_operation(), model.transfer.as_str()), (13, "Extracting B"));
        assert_eq!((model.error.as_str(), model.errors.len()), ("A.zip is corrupt", 0));
        model.receive(parse(r#"{"t":"job","job":{"id":13,"kind":"extract","state":"failed","reason":"B.zip is corrupt"},"unfinished":0}"#).unwrap(), &mut wire).unwrap();
        assert_eq!(model.transfer, "");
        assert_eq!(model.errors.iter().collect::<Vec<_>>(), ["B.zip is corrupt"]);
        finish(wire, reader);
    }
}

pub(super) fn fraction(job: &crate::jsondoc::Json) -> Option<f64> {
    use super::wire::{count, text};
    if text(job, "state") != "running" {
        return None;
    }
    let total = count(job, "totalBytes");
    (total > 0).then(|| (count(job, "bytes") as f64 / total as f64).clamp(0.0, 1.0))
}

// tui-progress draws twenty cells, with the completed stroke in accent and the rest in frame ink.
pub(super) fn bar<'a>(
    theme: &'a super::theme::Theme,
    fraction: f64,
    i18n: &crate::i18n::I18n,
) -> [(&'a str, String, usize); 3] {
    let fraction = fraction.clamp(0.0, 1.0);
    let filled = (fraction * 20.0) as usize;
    [
        (theme.accent.as_str(), "━".repeat(filled), 2),
        (theme.border.as_str(), "─".repeat(20 - filled), 0),
        (
            theme.muted.as_str(),
            i18n.tr_args(
                "progress.percent",
                &[&i18n.locale().format_count((fraction * 100.0) as i64)],
            ),
            2,
        ),
    ]
}

#[cfg(test)]
mod bar_tests {
    use super::*;
    use crate::jsondoc::parse;

    #[test]
    fn extraction_unknown_bytes_never_fabricate_a_percentage() {
        let job = parse(r#"{"state":"running","done":12,"total":0,"bytes":3400,"totalBytes":0}"#)
            .unwrap();
        assert_eq!(fraction(&job), None);
    }

    #[test]
    fn extraction_bar_uses_unpacked_fraction_and_the_existing_theme_roles() {
        let job = parse(r#"{"state":"running","bytes":3700000,"totalBytes":9700000}"#).unwrap();
        let theme = super::super::theme::Theme::from_text("");
        let pieces = bar(
            &theme,
            fraction(&job).unwrap(),
            &crate::i18n::I18n::default(),
        );
        assert_eq!(pieces[0], (theme.accent.as_str(), "━━━━━━━".into(), 2));
        assert_eq!(
            pieces[1],
            (theme.border.as_str(), "─────────────".into(), 0)
        );
        assert_eq!(pieces[2], (theme.muted.as_str(), "38%".into(), 2));
    }
}

#[derive(Default)]
pub(super) struct Status {
    pub id: usize,
    pub fraction: Option<f64>,
    pub count: usize,
    i18n: crate::i18n::I18n,
}

impl Status {
    pub fn suffix(&self) -> String {
        let count = self.i18n.trn("jobs.count", self.count as i64);
        self.i18n.tr_args("jobs.suffix", &[&count])
    }

    pub fn bar<'a>(
        &self,
        theme: &'a super::theme::Theme,
        fraction: f64,
    ) -> [(&'a str, String, usize); 3] {
        bar(theme, fraction, &self.i18n)
    }
}

pub(super) fn receive(model: &mut super::model::Model, value: &crate::jsondoc::Json) {
    use super::wire::{count, text};
    use crate::jsondoc::Json;
    let status = &mut model.job_status;
    status.i18n.poll_changed();
    if text(value, "t") == "jobs" {
        status.id = count(value, "newest");
        status.count = count(value, "unfinished");
        status.fraction = value
            .get("jobs")
            .and_then(Json::as_array)
            .and_then(|jobs| jobs.iter().find(|job| count(job, "id") == status.id))
            .and_then(fraction);
        model.transfer = line(&status.i18n, text(value, "head"), status.count);
    } else if let Some(job) = value.get("job") {
        if count(job, "id") == status.id {
            status.fraction = fraction(job);
            status.count = count(value, "unfinished");
            model.transfer = if matches!(text(job, "state"), "waiting" | "running") {
                line(&status.i18n, text(job, "head"), status.count)
            } else {
                String::new()
            };
        }
        // A failed extraction says why, whichever job owns the status row.
        if text(job, "kind") == "extract" && text(job, "state") == "failed" {
            model.fail(text(job, "reason").into());
        }
    }
}
