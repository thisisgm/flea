//! Only replies matching the held card or submitted operation can affect Print.
use super::*;

pub fn receive(m: &mut Model, value: &Json, w: &mut Wire) -> io::Result<bool> {
    let event = text(value, "t");
    let navigation = m.printing.card.as_ref().is_some_and(|form| {
        count(value, "gen") == 0
            && (m.pending.as_ref().is_some_and(|path| path != &form.folder)
                || (!text(value, "path").is_empty()
                    && std::path::Path::new(text(value, "path")) != form.folder))
    });
    if event == "listed" && navigation && m.printing.run.is_none() {
        cancel(m, w)?;
    }
    if event == "menuaction"
        && m.printing.card.is_some()
        && count(value, "id") == m.action_id
        && m.menu_action.starts_with("print")
    {
        if !flag(value, "ok") {
            let error = text(value, "error").to_owned();
            cancel(m, w)?;
            m.fail(error);
        } else if text(value, "op") == "snapshot" && m.menu_action == "printSnapshot" {
            m.menu_action = "printValidate".into();
            menu(w, m.action_id, "validate")?;
        } else if text(value, "op") == "validate"
            && text(value, "action") == "print"
            && m.menu_action == "printValidate"
        {
            let paths = value
                .get("paths")
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>();
            let valid = paths.as_ref().is_some_and(|paths| {
                paths.len() == m.menu_count
                    && !paths.is_empty()
                    && paths.len() <= MAX_ITEMS
                    && paths.iter().all(|path| {
                        path.starts_with('/')
                            && !path.contains('\0')
                            && classify(m, path).printable()
                    })
            });
            if !valid {
                cancel(m, w)?;
                m.fail_text(Text::Key("dialogs.output.error.print_kind"));
            } else {
                let paths = paths.unwrap();
                let paged = paths.iter().any(|path| classify(m, path).paged());
                let form = m.printing.card.as_mut().unwrap();
                form.paths = paths;
                form.paged = paged;
                m.menu_action = "printCard".into();
                query(m, w, "")?;
            }
        }
        return Ok(true);
    }
    // Sample input: {"t":"printoptions","id":1,"ok":false,"destinations":[],"err":"CUPS unavailable"}
    if event == "printoptions" {
        options(m, value);
        return Ok(true);
    }
    if !matches!(event, "printstarted" | "printed" | "printdone" | "error") {
        return Ok(false);
    }
    let Some(run) = &mut m.printing.run else {
        return Ok(false);
    };
    if count(value, "requestId") != run.request {
        return Ok(false);
    }
    // Sample input: {"t":"error","msg":"Selected items changed; reopen the menu.","requestId":1}
    if event == "error" {
        let error = text(value, "msg").to_owned();
        let menu_id = run.menu;
        m.printing.run = None;
        m.printing.card = None;
        if m.action_id == menu_id && m.menu_action.starts_with("print") {
            m.menu_action.clear();
        }
        menu(w, menu_id, "close")?;
        m.fail(error);
        return Ok(true);
    }
    let id = count(value, "id");
    if id == 0 {
        return Ok(true);
    }
    // Sample input: {"t":"printstarted","id":2,"requestId":1,"title":"Print report.pdf"}
    if event == "printstarted" && run.operation.is_none() {
        run.operation = Some(id);
        if run.cancelled {
            w.send(vec![("c", word("transfercancel")), ("id", number(id))])?;
        }
        m.printing.card = None;
        m.menu_action.clear();
        menu(w, run.menu, "close")?;
        return Ok(true);
    }
    if run.operation != Some(id) {
        return Ok(true);
    }
    // Sample input: {"t":"printed","id":2,"requestId":1,"path":"/d/report.pdf","ok":true,"message":"Sent report.pdf to Brother"}
    if event == "printed" {
        let path = text(value, "path");
        let Some(ok) = value.get("ok").and_then(Json::as_bool) else {
            return Ok(true);
        };
        if !run.paths.iter().any(|p| p == path) || !run.outcomes.insert(path.into()) {
            return Ok(true);
        }
        let printer = run.printer.clone();
        if ok {
            run.accepted.insert(path.into());
            let name = path.rsplit('/').next().unwrap_or(path);
            m.say_text(Text::Args(
                "dialogs.output.sent",
                vec![name.into(), printer.into()],
            ));
        } else {
            m.fail_text(Text::Args(
                "dialogs.output.error.legacy",
                vec![text(value, "message").into()],
            ));
        }
    // Sample input: {"t":"printdone","id":2,"requestId":1,"ok":false,"sent":0,"message":"","err":"cancelled"}
    } else if event == "printdone" {
        let Some(ok) = value.get("ok").and_then(Json::as_bool) else {
            return Ok(true);
        };
        let reason = text(value, "err");
        let menu_id = run.menu;
        let cancelled = (!ok && reason == "cancelled").then(|| {
            let names = |accepted| {
                let names: Vec<_> = run
                    .paths
                    .iter()
                    .filter(|path| run.accepted.contains(*path) == accepted)
                    .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
                    .collect();
                if names.is_empty() {
                    Text::Key("dialogs.output.choice.None")
                } else {
                    names.join(", ").into()
                }
            };
            Text::Args("dialogs.output.cancelled", vec![names(true), names(false)])
        });
        m.printing.run = None;
        m.printing.card = None;
        if m.action_id == menu_id && m.menu_action.starts_with("print") {
            m.menu_action.clear();
        }
        menu(w, menu_id, "close")?;
        if let Some(message) = cancelled {
            m.say_text(message);
        } else if !ok {
            m.fail_text(Text::Args(
                "dialogs.output.error.legacy",
                vec![if reason.is_empty() {
                    text(value, "message")
                } else {
                    reason
                }
                .into()],
            ));
        }
    }
    Ok(true)
}
// Sample input: {"t":"printoptions","id":1,"ok":false,"destinations":[],"err":"CUPS unavailable"}
fn options(m: &mut Model, value: &Json) {
    let Some(form) = &mut m.printing.card else {
        return;
    };
    if form.query != Some(count(value, "id")) {
        return;
    }
    if value.get("ok").and_then(Json::as_bool) == Some(false) {
        form.query = None;
        form.queried = false;
        let words = Text::Args(
            "dialogs.output.error.printer_query",
            vec![text(value, "err").into()],
        );
        Held::set(
            &mut form.error,
            &mut form.error_words,
            words.clone(),
            &m.i18n,
        );
        m.fail_text(words);
        return;
    }
    let Some((mut destinations, printer, caps)) = parsed_options(value, &form.pending_printer)
    else {
        form.query = None;
        form.queried = false;
        let words = Text::Args(
            "dialogs.output.error.printer_query",
            vec![Text::Key("dialogs.output.error.printer_reply")],
        );
        Held::set(
            &mut form.error,
            &mut form.error_words,
            words.clone(),
            &m.i18n,
        );
        m.fail_text(words);
        return;
    };
    let Capabilities {
        duplex: _,
        two_sided,
        color: _,
        grayscale,
    } = caps;
    destinations.sort_by_key(|d| !d.default);
    form.destinations = destinations;
    form.printer = printer.into();
    let name = text(value, "printerName");
    form.printer_name = if name.trim().is_empty() {
        printer
    } else {
        name
    }
    .into();
    form.caps = caps;
    form.two_sided = two_sided;
    form.grayscale = grayscale;
    form.setup = value
        .get("setupArgv")
        .and_then(Json::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect();
    form.query = None;
    form.queried = true;
    form.choice = form
        .destinations
        .iter()
        .position(|d| d.name == printer)
        .unwrap_or(0);
}

// Sample input: {"t":"printoptions","id":1,"ok":true,"destinations":[{"name":"Brother","default":true}],"printer":"Brother","printerName":"Brother HL-L3270CDW","duplex":true,"twoSided":false,"color":true,"grayscale":false,"setupArgv":["system-config-printer"]}
fn parsed_options<'a>(
    value: &'a Json,
    pending_printer: &str,
) -> Option<(Vec<Destination>, &'a str, Capabilities)> {
    if !flag(value, "ok") {
        return None;
    }
    let rows = value
        .get("destinations")
        .and_then(Json::as_array)
        .filter(|rows| rows.len() <= MAX_ITEMS)?;
    let destinations = rows
        .iter()
        .map(|d| {
            Some(Destination {
                name: d.get("name")?.as_str()?.into(),
                default: d.get("default")?.as_bool()?,
            })
        })
        .collect::<Option<Vec<_>>>();
    let destinations =
        destinations.filter(|ds| ds.iter().all(|d| printing::valid_destination(&d.name)))?;
    let printer = text(value, "printer");
    if (!pending_printer.is_empty() && printer != pending_printer)
        || (!destinations.is_empty() && !destinations.iter().any(|d| d.name == printer))
    {
        return None;
    }
    let caps = ["duplex", "twoSided", "color", "grayscale"]
        .map(|key| value.get(key).and_then(Json::as_bool));
    let [Some(duplex), Some(two_sided), Some(color), Some(grayscale)] = caps else {
        return None;
    };
    Some((
        destinations,
        printer,
        Capabilities {
            duplex,
            two_sided,
            color,
            grayscale,
        },
    ))
}
