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
        let i18n = crate::i18n::test_catalogs::english();
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

    #[test]
    fn ey568_a_copy_status_names_no_destination_and_a_move_keeps_its_own() {
        let (mut wire, reader) = echo_wire();
        let mut model = Model::new(PathBuf::from("/listing"), &Json::Null);
        for (line, shown) in [
            (r#"{"t":"jobs","jobs":[{"id":7,"kind":"copy","state":"waiting","head":"Copy 5 items to fx"}],"newest":7,"unfinished":1,"head":"Copy 5 items to fx"}"#, "Copy 5 items"),
            // The snapshot lists every row newest first, finished ones too, so newest sits between two moves.
            (r#"{"t":"jobs","jobs":[{"id":9,"kind":"move","state":"done","head":"Moving 2 of 2 to fx"},{"id":7,"kind":"copy","state":"running","head":"Copying 2 of 5 to fx"},{"id":6,"kind":"move","state":"running","head":"Moving 1 of 1 to fx"}],"newest":7,"unfinished":2,"head":"Copying 2 of 5 to fx"}"#, "Copying 2 of 5 · 2 jobs"),
            (r#"{"t":"job","job":{"id":7,"kind":"copy","state":"running","head":"Copying 3 of 5 to a to b"},"unfinished":2}"#, "Copying 3 of 5 · 2 jobs"),
            (r#"{"t":"job","job":{"id":7,"kind":"copy","state":"failed","head":"Copy failed"},"unfinished":1}"#, ""),
            (r#"{"t":"jobs","jobs":[{"id":9,"kind":"move","state":"done","head":"Moving 2 of 2 to fx"},{"id":7,"kind":"copy","state":"failed","head":"Copy failed"},{"id":6,"kind":"move","state":"running","head":"Moving 1 of 1 to fx"}],"newest":6,"unfinished":1,"head":"Moving 1 of 1 to fx"}"#, "Moving 1 of 1 to fx"),
            (r#"{"t":"jobs","jobs":[{"id":11,"kind":"move","state":"running","head":"Moving 1 of 2 to fx"},{"id":10,"kind":"copy","state":"running","head":"Copying 1 of 4 to fx"}],"newest":11,"unfinished":2,"head":"Moving 1 of 2 to fx"}"#, "Moving 1 of 2 to fx · 2 jobs"),
            (r#"{"t":"job","job":{"id":11,"kind":"move","state":"running","head":"Moving 2 of 2 to fx"},"unfinished":2}"#, "Moving 2 of 2 to fx · 2 jobs"),
        ] {
            model.receive(parse(line).unwrap(), &mut wire).unwrap();
            assert_eq!(model.transfer, shown, "{line}");
        }
        finish(wire, reader);
    }
}

pub(super) fn percent(job: &crate::jsondoc::Json) -> Option<usize> {
    use super::wire::{count, text};
    if text(job, "state") != "running" {
        return None;
    }
    let total = count(job, "totalBytes");
    (total > 0).then(|| ratio(count(job, "bytes").min(total) as u128, total as u128))
}

// A whole percent in integers, so 57 bytes of 100 read 57% where a float reads 56.99.
pub(super) fn ratio(done: u128, total: u128) -> usize {
    (done.min(total) * 100 / total.max(1)) as usize
}

// The tui-progress board draws twenty cells, the done run in the accent over the border track, then the percent in meta ink.
pub(super) fn bar<'a>(
    theme: &'a super::theme::Theme,
    percent: usize,
    i18n: &crate::i18n::I18n,
) -> [(&'a str, String, usize); 3] {
    const CELLS: usize = 20;
    let percent = percent.min(100);
    let filled = (percent * CELLS + 50) / 100;
    [
        (theme.accent.as_str(), "━".repeat(filled), 2),
        (theme.border.as_str(), "─".repeat(CELLS - filled), 0),
        (
            theme.meta.as_str(),
            i18n.tr_args("progress.percent", &[&i18n.locale().format_count(percent as i64)]),
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
        assert_eq!(percent(&job), None);
    }

    #[test]
    fn extraction_bar_uses_unpacked_bytes_and_the_existing_theme_roles() {
        let job = parse(r#"{"state":"running","bytes":3700000,"totalBytes":9700000}"#).unwrap();
        let theme = super::super::theme::Theme::from_text("");
        let pieces = bar(
            &theme,
            percent(&job).unwrap(),
            &crate::i18n::test_catalogs::english(),
        );
        assert_eq!(pieces[0], (theme.accent.as_str(), "━".repeat(8), 2));
        assert_eq!(pieces[1], (theme.border.as_str(), "─".repeat(12), 0));
        assert_eq!(pieces[2], (theme.meta.as_str(), "38%".into(), 2));
    }

    #[test]
    fn a_percent_is_whole_and_the_bar_rounds_to_twenty_cells() {
        let job = parse(r#"{"state":"running","bytes":57,"totalBytes":100}"#).unwrap();
        assert_eq!(percent(&job), Some(57));
        let job = parse(r#"{"state":"running","bytes":120,"totalBytes":100}"#).unwrap();
        assert_eq!(percent(&job), Some(100));
        let theme = super::super::theme::Theme::from_text("");
        let i18n = crate::i18n::test_catalogs::english();
        for (percent, filled) in [(0, 0), (2, 0), (3, 1), (40, 8), (57, 11), (100, 20), (250, 20)] {
            let pieces = bar(&theme, percent, &i18n);
            assert_eq!((pieces[0].1.chars().count(), pieces[1].1.chars().count()), (filled, 20 - filled), "{percent}");
        }
    }
}

#[derive(Default)]
pub(super) struct Status {
    pub id: usize,
    pub percent: Option<usize>,
    pub count: usize,
    i18n: crate::i18n::I18n,
    head: String,
}

impl Status {
    // Tests read English only; the fields are spelled out because Self::default() would still load the host's language.
    #[cfg(test)]
    pub fn english() -> Self {
        Self { id: 0, percent: None, count: 0, i18n: crate::i18n::test_catalogs::english(), head: String::new() }
    }

    pub fn suffix(&self) -> String {
        let count = self.i18n.trn("jobs.count", self.count as i64);
        self.i18n.tr_args("jobs.suffix", &[&count])
    }

    pub fn bar<'a>(
        &self,
        theme: &'a super::theme::Theme,
        percent: usize,
    ) -> [(&'a str, String, usize); 3] {
        bar(theme, percent, &self.i18n)
    }
}

pub(super) fn receive(model: &mut super::model::Model, value: &crate::jsondoc::Json) {
    use super::wire::{count, text};
    use crate::jsondoc::Json;
    let status = &mut model.job_status;
    status.i18n = model.i18n.clone();
    if text(value, "t") == "jobs" {
        status.id = count(value, "newest");
        status.count = count(value, "unfinished");
        let newest = value
            .get("jobs")
            .and_then(Json::as_array)
            .and_then(|jobs| jobs.iter().find(|job| count(job, "id") == status.id));
        status.percent = newest.and_then(percent);
        status.head = status_head(newest.map_or("", |job| text(job, "kind")), text(value, "head"));
        model.transfer = line(&status.i18n, &status.head, status.count);
    } else if let Some(job) = value.get("job") {
        if count(job, "id") == status.id {
            status.percent = percent(job);
            status.count = count(value, "unfinished");
            status.head = if matches!(text(job, "state"), "waiting" | "running") {
                status_head(text(job, "kind"), text(job, "head"))
            } else {
                String::new()
            };
            model.transfer = if status.head.is_empty() { String::new() } else {
                line(&status.i18n, &status.head, status.count)
            };
        }
        // A failed extraction says why, whichever job owns the status row.
        if text(job, "kind") == "extract" && text(job, "state") == "failed" {
            model.fail(text(job, "reason").into());
        }
    }
}

// The tui-progress board names no destination for a copy, while the window jobs card keeps it.
// "Copying 2 of 5 to fx" or "Copy 5 items to fx": the backend's first " to " ends the count.
fn status_head(kind: &str, head: &str) -> String {
    match head.split_once(" to ") {
        Some((words, _)) if kind == "copy" => words.into(),
        _ => head.into(),
    }
}

pub(super) fn refresh_locale(model: &mut super::model::Model) {
    let status = &mut model.job_status;
    status.i18n = model.i18n.clone();
    if status.id != 0 && !status.head.is_empty() {
        model.transfer = line(&status.i18n, &status.head, status.count);
    }
}
