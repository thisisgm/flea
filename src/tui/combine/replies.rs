//! Replies stay bound to the held menu, operation, source and request identity.
use super::*;

pub fn receive(m: &mut Model, value: &Json, w: &mut Wire) -> io::Result<bool> {
    let event = text(value, "t");
    if event == "listed" && m.combining.card.is_some() {
        cancel(m, w)?;
    }
    // Sample: {"t":"menuaction","op":"validate","action":"combine","id":7,"ok":true,"paths":["/work/a.png","/work/b.jpg"]}.
    if event == "menuaction"
        && m.combining.card.is_some()
        && count(value, "id") == m.action_id
        && m.menu_action.starts_with("combine")
    {
        if !flag(value, "ok") {
            let error = text(value, "error").into();
            cancel(m, w)?;
            m.fail(error);
        } else if text(value, "op") == "snapshot" && m.menu_action == "combineSnapshot" {
            m.menu_action = "combineValidate".into();
            menu(w, m.action_id, "validate")?;
        } else if text(value, "op") == "validate"
            && text(value, "action") == "combine"
            && m.menu_action == "combineValidate"
        {
            let paths = value
                .get("paths")
                .and_then(Json::as_array)
                .filter(|ps| {
                    ps.len() == m.menu_count && (2..=output::MAX_ITEMS).contains(&ps.len())
                })
                .and_then(|ps| {
                    ps.iter()
                        .map(|p| p.as_str().map(str::to_owned))
                        .collect::<Option<Vec<_>>>()
                });
            let valid = paths.as_ref().is_some_and(|ps| {
                let first = Path::new(&ps[0]).parent();
                let mut seen = std::collections::HashSet::new();
                ps.iter().all(|p| {
                    Path::new(p).is_absolute()
                        && !flea::bytename::is_refused(p)
                        && Path::new(p).parent() == first
                        && seen.insert(p)
                        && crate::backend::printconvert::classify(
                            p,
                            m.combining.mime.get_or_init(crate::backend::mime::Db::load),
                            m.combining
                                .aliases
                                .get_or_init(crate::backend::aliases::Aliases::load),
                        ) == output::Kind::Image
                })
            });
            if !valid {
                cancel(m, w)?;
                m.fail_text(Text::Key("dialogs.output.error.image_count"));
                return Ok(true);
            }
            let f = m.combining.card.as_mut().unwrap();
            f.paths = paths.unwrap();
            f.format = output::combined_format(Path::new(&f.paths[0])).into();
            if f.format == "jpg" {
                f.settings.background = Background::White;
            }
            f.rename_default();
            m.menu_action = "combineCard".into();
        }
        return Ok(true);
    }
    // Samples: {"t":"combinestarted","id":9,"requestId":3}; combinedone also echoes source/paths/ok/err, while progress carries done/total.
    if !matches!(
        event,
        "combinestarted" | "combineprogress" | "combinedone" | "error"
    ) {
        return Ok(false);
    }
    let Some(run) = &mut m.combining.run else {
        return Ok(false);
    };
    let busy = event == "error"
        && matches!(text(value, "where"), "combine" | "convert")
        && count(value, "requestId") == 0
        && run.operation.is_none()
        && m.menu_action == "combineSubmitting";
    if !busy && count(value, "requestId") != run.request {
        return Ok(false);
    }
    if event == "error" && !busy && text(value, "where") != "combine" {
        return Ok(false);
    }
    if event == "combinedone" && text(value, "source") != run.source {
        return Ok(true);
    }
    let id = count(value, "id");
    if event == "combinestarted" && id > 0 && run.operation.is_none() {
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
    if event == "combineprogress" {
        if m.job_status.id == 0 && m.transfer_id == 0 {
            m.transfer_text(Text::Key("tui.combine.running"));
            let total = count(value, "total");
            m.progress_percent = (total > 0).then(|| {
                super::super::jobstatus::ratio(count(value, "done") as u128, total as u128)
            });
        }
    } else if matches!(event, "combinedone" | "error") {
        let run = m.combining.run.take().unwrap();
        if m.action_id == run.menu && m.menu_action.starts_with("combine") {
            m.menu_action.clear();
        }
        menu(w, run.menu, "close")?;
        if m.job_status.id == 0 && m.transfer_id == 0 {
            m.transfer.clear();
            m.progress_percent = None;
        }
        if event == "error" {
            m.fail(text(value, "msg").into());
        } else if text(value, "err") == "cancelled" {
            m.say_text(Text::Key("tui.combine.cancelled"));
        } else if !flag(value, "ok") {
            m.fail_text(Text::Args(
                "dialogs.output.error.operation",
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
