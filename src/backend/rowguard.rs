// A row index names a file only in the numbering the client read it from; see docs/protocol.md "listing".
use crate::backend::menu_actions;
use crate::backend::proto::error_line;
use crate::error::FleaError;
use crate::json::{field_str, field_usize};

// The requests whose rows resolve to files something then acts on; thumb, dirsize, meta, tagstates and window only read.
const GUARDED: [&str; 9] = ["outline", "trash", "transfer", "paths", "menuaction", "collisions", "mergecollisions", "link", "settags"];

// The numbering a backend's first listing answers in, which State::new starts one below and a prewarm file stands in for.
pub const FIRST_LISTING: u64 = 1;

const STALE: &str = "the folder changed before this could run, so nothing was done";
const STALE_MENU: &str = "The folder changed; reopen the menu.";

// Sample input: {"t":"rows","start":0,"rows":[],"kinds":[],"ms":0.001}
// The rows line carries the numbering it was written in, as its last field, so a client can name it back.
pub fn stamped(mut rows_line: String, generation: u64) -> String {
    if rows_line.ends_with('}') {
        rows_line.insert_str(rows_line.len() - 1, &format!(",\"listing\":{}", generation));
    }
    rows_line
}

// A session answer's rows line, carrying the query gen beside the listing stamp.
pub fn stamped_gen(mut rows_line: String, generation: u64, gen: usize) -> String {
    if rows_line.ends_with('}') {
        rows_line.insert_str(rows_line.len() - 1, &format!(",\"listing\":{},\"gen\":{}", generation, gen));
    }
    rows_line
}

// Sample input: {"c":"trash","rows":[0],"menuId":0,"listing":7}
// None for a request that names no numbering, or the one in force; otherwise the answer that refuses it.
pub fn refusal(line: &str, generation: u64) -> Option<String> {
    let named = field_usize(line, "listing")? as u64;
    let command = field_str(line, "c")?;
    if named == generation || !GUARDED.contains(&command.as_str()) {
        return None;
    }
    // The menu waits on a menuaction reply in its own shape, so it is refused in that shape.
    if command == "menuaction" {
        return Some(menu_actions::response(line, Err(STALE_MENU.into())));
    }
    let where_ = if command == "outline" { "outline" } else { "stale" };
    Some(error_line(&FleaError { where_: where_.into(), path: command, msg: STALE.into() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rows_line_names_its_numbering_as_its_last_field() {
        let line = stamped(r#"{"t":"rows","start":0,"rows":[],"kinds":[],"ms":0.001}"#.to_string(), 7);
        assert_eq!(line, r#"{"t":"rows","start":0,"rows":[],"kinds":[],"ms":0.001,"listing":7}"#);
        assert_eq!(field_usize(&line, "listing"), Some(7));
    }

    #[test]
    fn rows_read_from_an_earlier_numbering_are_refused_by_name() {
        let refused = refusal(r#"{"c":"trash","rows":[0],"menuId":0,"listing":1}"#, 2).expect("refused");
        assert_eq!(field_str(&refused, "t").as_deref(), Some("error"));
        assert_eq!(field_str(&refused, "where").as_deref(), Some("stale"));
        assert_eq!(field_str(&refused, "path").as_deref(), Some("trash"));
        assert_eq!(field_str(&refused, "msg").as_deref(), Some("the folder changed before this could run, so nothing was done"));
        for command in ["transfer", "paths"] {
            let line = format!(r#"{{"c":"{}","rows":[3],"listing":4}}"#, command);
            assert_eq!(refusal(&line, 5).and_then(|r| field_str(&r, "path")).as_deref(), Some(command));
        }
    }

    #[test]
    fn a_stale_menu_snapshot_is_refused_in_the_shape_the_menu_waits_for() {
        let refused = refusal(r#"{"c":"menuaction","op":"snapshot","id":9,"rows":[0],"cursor":0,"listing":1}"#, 2)
            .expect("refused");
        assert_eq!(field_str(&refused, "t").as_deref(), Some("menuaction"));
        assert_eq!(field_usize(&refused, "id"), Some(9));
        assert!(refused.contains(r#""ok":false"#));
        assert_eq!(field_str(&refused, "error").as_deref(), Some("The folder changed; reopen the menu."));
    }

    #[test]
    fn a_stale_collisions_question_is_refused_like_any_other_row_request() {
        let refused = refusal(r#"{"c":"collisions","id":7,"rows":[0],"dest":"/b","listing":4}"#, 5).expect("refused");
        assert_eq!(field_str(&refused, "t").as_deref(), Some("error"));
        assert_eq!(field_str(&refused, "where").as_deref(), Some("stale"));
        assert_eq!(field_str(&refused, "path").as_deref(), Some("collisions"));
    }

    #[test]
    fn the_numbering_in_force_an_absent_one_and_a_read_only_request_all_pass() {
        assert!(refusal(r#"{"c":"trash","rows":[0],"listing":2}"#, 2).is_none());
        assert!(refusal(r#"{"c":"trash","rows":[0]}"#, 2).is_none());
        assert!(refusal(r#"{"c":"window","start":0,"count":9,"listing":1}"#, 2).is_none());
        assert!(refusal(r#"{"c":"thumb","rows":[0],"listing":1}"#, 2).is_none());
    }
}
