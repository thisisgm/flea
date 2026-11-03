// The officepdf and officesheets reply lines the backend writes to the viewer.
use crate::json::escape;
use std::path::Path;

pub fn ready_line(id: usize, path: &Path) -> String {
    format!(r#"{{"t":"officepdf","id":{},"state":"ready","path":"{}"}}"#, id, escape(&path.to_string_lossy()))
}

#[cfg(test)]
pub fn converting_line(id: usize) -> String {
    converting_line_of(false, id)
}

// sheets names the officesheets request the line answers.
pub fn converting_line_of(sheets: bool, id: usize) -> String {
    format!(r#"{{"t":"{}","id":{},"state":"converting"}}"#, kind(sheets), id)
}

// unavailable marks a box or a file that will never convert, so the viewer keeps today's preview.
#[cfg(test)]
pub fn failed_line(id: usize, err: &str, unavailable: bool) -> String {
    failed_line_of(false, id, err, unavailable)
}

pub fn failed_line_of(sheets: bool, id: usize, err: &str, unavailable: bool) -> String {
    let flag = if unavailable { r#","unavailable":true"# } else { "" };
    format!(r#"{{"t":"{}","id":{},"state":"failed","err":"{}"{}}}"#, kind(sheets), id, escape(err), flag)
}

fn kind(sheets: bool) -> &'static str {
    if sheets { "officesheets" } else { "officepdf" }
}
