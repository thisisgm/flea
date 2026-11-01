use super::restore;
use crate::jsondoc::Json;
use crate::tui::{
    model::Model,
    wire::{count, text},
};

// A superseded gen is dropped; an untagged failure answers a waiting names Find only from its own wheres.
pub(super) fn receive(m: &mut Model, value: &Json) -> bool {
    let gen = count(value, "gen");
    let contents = text(value, "where") == "searchcontents";
    let own = m.find.asked.as_ref().is_some_and(|q| q.gen == gen);
    let answers = if contents {
        own || m.find.waiting && gen == 0
    } else if gen > 0 {
        own
    } else {
        m.find.waiting && names_failure(value)
    };
    if answers {
        restore(m);
        m.fail(text(value, "msg").into());
    }
    answers || gen > 0 || contents
}

// Untagged failures a names Find raises: its search walk, its scan, sort and read, and the refusal of its own request.
fn names_failure(value: &Json) -> bool {
    match text(value, "where") {
        "search" | "scan" | "sort" | "read" => true,
        "contentsidentity" => matches!(text(value, "path"), "searchopen" | "searchquery"),
        _ => false,
    }
}
