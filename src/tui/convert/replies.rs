//! Replies must name the captured request, source and operation before changing the card or status.
use super::*;

pub fn receive(m: &mut Model, value: &Json, w: &mut Wire) -> io::Result<bool> {
    let event = text(value, "t");
    if event == "listed" && m.converting.card.is_some() {
        cancel(m, w)?;
    }
    // Sample: {"t":"menuaction","op":"validate","action":"convert","id":7,"ok":true,"paths":["/work/photo.png"]}.
    if event == "menuaction"
        && m.converting.card.is_some()
        && count(value, "id") == m.action_id
        && m.menu_action.starts_with("convert")
    {
        if !flag(value, "ok") {
            let error = text(value, "error").to_owned();
            cancel(m, w)?;
            m.fail(error);
        } else if text(value, "op") == "snapshot" && m.menu_action == "convertSnapshot" {
            m.menu_action = "convertValidate".into();
            menu(w, m.action_id, "validate")?;
        } else if text(value, "op") == "validate"
            && text(value, "action") == "convert"
            && m.menu_action == "convertValidate"
        {
            let paths = value
                .get("paths")
                .and_then(Json::as_array)
                .filter(|ps| {
                    ps.len() == m.menu_count && !ps.is_empty() && ps.len() <= output::MAX_ITEMS
                })
                .and_then(|ps| {
                    ps.iter()
                        .map(|p| p.as_str().map(str::to_owned))
                        .collect::<Option<Vec<_>>>()
                });
            let valid = paths.as_ref().is_some_and(|ps| {
                ps.iter()
                    .all(|p| p.starts_with('/') && !flea::bytename::is_refused(p))
            });
            if !valid {
                cancel(m, w)?;
                m.fail_text(Text::Key("dialogs.output.error.convert_kind"));
                return Ok(true);
            }
            let paths = paths.unwrap();
            let kinds: Vec<_> = paths
                .iter()
                .map(|p| {
                    crate::backend::printconvert::classify(
                        p,
                        m.converting
                            .mime
                            .get_or_init(crate::backend::mime::Db::load),
                        m.converting
                            .aliases
                            .get_or_init(crate::backend::aliases::Aliases::load),
                    )
                })
                .collect();
            if !output::can_convert(&kinds) {
                cancel(m, w)?;
                m.fail_text(Text::Key("dialogs.output.error.convert_kind"));
                return Ok(true);
            }
            let f = m.converting.card.as_mut().unwrap();
            f.kind = kinds[0];
            f.choice = 0;
            if f.kind == Kind::Office {
                f.focus = 3;
            }
            if f.kind == Kind::Pdf {
                f.count = m
                    .pdf
                    .as_ref()
                    .filter(|pdf| pdf.source == Path::new(&paths[0]))
                    .and_then(|pdf| u32::try_from(pdf.pages).ok())
                    .filter(|n| *n > 0);
                if f.count.is_none() {
                    f.info = Some(super::super::job::Job::start(
                        paths[0].clone().into(),
                        vec!["/usr/bin/pdfinfo".into(), "{input}".into()],
                    ));
                }
            }
            f.paths = paths;
            m.menu_action = "convertCard".into();
        }
        return Ok(true);
    }
    // Sample: {"t":"convertstarted","id":9,"requestId":3,"source":"/work/photo.png"}; progress echoes id/requestId and done/total; convertdone echoes source and paths/ok/err.
    if !matches!(
        event,
        "convertstarted" | "convertprogress" | "convertdone" | "error"
    ) {
        return Ok(false);
    }
    let Some(run) = &mut m.converting.run else {
        return Ok(false);
    };
    // A pre-start busy refusal has no requestId; this client permits only one pending Convert submission.
    let busy = event == "error"
        && text(value, "where") == "convert"
        && count(value, "requestId") == 0
        && run.operation.is_none()
        && m.menu_action == "convertSubmitting";
    if !busy && count(value, "requestId") != run.request {
        return Ok(false);
    }
    if event != "error" && event != "convertprogress" && text(value, "source") != run.source {
        return Ok(true);
    }
    let id = count(value, "id");
    if event == "convertstarted" && id > 0 && run.operation.is_none() {
        run.operation = Some(id);
        if run.cancelled {
            w.send(vec![("c", word("transfercancel")), ("id", number(id))])?;
        }
        menu(w, run.menu, "close")?;
        m.menu_action.clear();
        return Ok(true);
    }
    if event != "error" && run.operation != Some(id) {
        return Ok(true);
    }
    if event == "convertprogress" {
        if m.job_status.id == 0 && m.transfer_id == 0 {
            m.transfer_text(Text::Key("dialogs.output.converting"));
            let total = count(value, "total");
            m.progress_percent = (total > 0).then(|| {
                super::super::jobstatus::ratio(count(value, "done") as u128, total as u128)
            });
        }
    } else if matches!(event, "convertdone" | "error") {
        let run = m.converting.run.take().unwrap();
        if m.action_id == run.menu && m.menu_action.starts_with("convert") {
            m.menu_action.clear();
        }
        menu(w, run.menu, "close")?;
        if m.job_status.id == 0 && m.transfer_id == 0 {
            m.transfer.clear();
            m.progress_percent = None;
        }
        let outputs = value
            .get("paths")
            .and_then(Json::as_array)
            .map_or(0, |paths| paths.len());
        if event == "error" {
            m.fail(text(value, "msg").into());
        } else if text(value, "err") == "cancelled" {
            m.say_text(Text::Plural("tui.convert.cancelled", outputs, vec![]));
        } else if !flag(value, "ok") {
            m.fail_text(Text::Plural(
                "tui.convert.failed",
                outputs,
                vec![text(value, "err").into()],
            ));
        } else {
            m.say_text(Text::Args(
                "dialogs.output.completed",
                vec![text(value, "path").into()],
            ));
        }
        if run.folder == m.path {
            m.refresh(w)?;
        }
    }
    Ok(true)
}
