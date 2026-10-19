// The officepdf reply lines the backend writes to the viewer.
use crate::json::escape;
use std::path::Path;

pub fn ready_line(id: usize, path: &Path) -> String {
    format!(r#"{{"t":"officepdf","id":{},"state":"ready","path":"{}"}}"#, id, escape(&path.to_string_lossy()))
}

pub fn converting_line(id: usize) -> String {
    format!(r#"{{"t":"officepdf","id":{},"state":"converting"}}"#, id)
}

// unavailable marks a box or a file that will never convert, so the viewer keeps today's preview.
pub fn failed_line(id: usize, err: &str, unavailable: bool) -> String {
    let flag = if unavailable { r#","unavailable":true"# } else { "" };
    format!(r#"{{"t":"officepdf","id":{},"state":"failed","err":"{}"{}}}"#, id, escape(err), flag)
}
