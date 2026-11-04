// Parse only rclone's documented JSON fields, retaining bounded errors and absolute byte samples.
use crate::jsondoc::{parse, Json};

// Bytes kept from one log row's error, skip or file name, so a long row cannot grow the job line.
const MAX_TEXT: usize = 4096;

#[derive(Default)]
pub(super) struct Log {
    pub bytes: u64,
    pub total: u64,
    pub size: u64,
    pub file: String,
    pub rate: u64,
    pub left: Option<u64>,
    pub error: String,
    pub no_hash: bool,
    pub skipped: String,
    file_error: bool,
}

fn number(value: Option<&Json>) -> u64 {
    value
        .and_then(Json::as_f64)
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
        .max(0.0) as u64
}

impl Log {
    // Sample input: {"level":"error","object":"a.jpg","msg":"md5 differ"} or {"level":"notice","msg":"...","stats":{"bytes":4000,"totalBytes":12000,"speed":240000.0,"eta":3,"transferring":[{"name":"a.jpg","size":12000}]}}.
    pub fn line(&mut self, text: &str) -> bool {
        let Ok(row) = parse(text) else { return false };
        let msg = row.get("msg").and_then(Json::as_str).unwrap_or_default();
        if msg.contains("hashes could not be checked") || msg.contains("could not check hash") {
            self.no_hash = true;
        }
        if self.skipped.is_empty()
            && (msg.starts_with("Can't follow symlink without -L/--copy-links")
                || msg.starts_with("Can't transfer non file/directory"))
        {
            let object = row.get("object").and_then(Json::as_str).unwrap_or_default();
            self.skipped = bounded(&format!("{}: {}", object, msg));
        }
        let object = row.get("object").and_then(Json::as_str).unwrap_or_default();
        // rclone 1.75 ends a failed command with a notice row "Failed to <command>: <cause>", its whole cause.
        let summary = object.is_empty() && msg.starts_with("Failed to ");
        let failed = summary
            || matches!(
                row.get("level").and_then(Json::as_str),
                Some("error" | "critical" | "alert" | "emergency")
            );
        if failed && !self.file_error && (self.error.is_empty() || !object.is_empty() || summary) {
            self.error = bounded(&format!(
                "{}{}{}",
                object,
                if object.is_empty() { "" } else { ": " },
                msg
            ));
            self.file_error = !object.is_empty();
        }
        let Some(stats) = row.get("stats") else {
            return false;
        };
        self.bytes = self.bytes.max(number(stats.get("bytes")));
        self.total = self
            .total
            .max(number(stats.get("totalBytes")))
            .max(self.bytes);
        self.rate = number(stats.get("speed"));
        self.left = stats
            .get("eta")
            .and_then(Json::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0)
            .map(|v| v.ceil() as u64);
        if let Some(current) = stats
            .get("transferring")
            .and_then(Json::as_array)
            .and_then(|items| items.first())
        {
            self.file = bounded(
                current
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            );
            self.size = number(current.get("size"));
        }
        true
    }
}

impl Log {
    // A failure sentence followed by the error rclone itself logged, already bounded, when there is one.
    pub fn explain(&self, sentence: String) -> String {
        if self.error.is_empty() {
            sentence
        } else {
            format!("{} {}", sentence, self.error)
        }
    }
}

fn bounded(text: &str) -> String {
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rclone_checksum_failure_keeps_the_file_diagnostic_after_the_summary() {
        let mut log = Log::default();
        log.line(r#"{"level":"error","msg":"checking failed"}"#);
        log.line(r#"{"level":"error","object":"two words.jpg","msg":"md5 differ"}"#);
        log.line(r#"{"level":"error","msg":"Failed to check: 1 differences found"}"#);
        assert_eq!(log.error, "two words.jpg: md5 differ");
    }

    #[test]
    fn rclone_command_failure_notice_is_the_cause_unless_a_file_failed() {
        let mut log = Log::default();
        log.line(r#"{"level":"notice","msg":"Failed to backend: didn't find backend called \"nosuchbackend\"","source":"cmd/cmd.go:334"}"#);
        assert_eq!(
            log.error,
            "Failed to backend: didn't find backend called \"nosuchbackend\""
        );
        let mut log = Log::default();
        log.line(r#"{"level":"error","msg":"Attempt 1/3 failed with 1 errors and: mkdir /r/afile: not a directory"}"#);
        log.line(r#"{"level":"notice","msg":"Failed to mkdir: mkdir /r/afile: not a directory"}"#);
        assert_eq!(
            log.error,
            "Failed to mkdir: mkdir /r/afile: not a directory"
        );
        let mut log = Log::default();
        log.line(r#"{"level":"error","object":"a.jpg","msg":"Failed to copy: connection reset"}"#);
        log.line(r#"{"level":"notice","msg":"Failed to copyto: connection reset"}"#);
        assert_eq!(log.error, "a.jpg: Failed to copy: connection reset");
        let mut log = Log::default();
        log.line(r#"{"level":"notice","object":"a.jpg","msg":"Failed to set modification time"}"#);
        assert!(log.error.is_empty());
    }

    #[test]
    fn rclone_json_stats_are_monotonic_and_retain_speed_eta_and_file() {
        let mut log = Log::default();
        assert!(log.line(r#"{"stats":{"bytes":795100,"totalBytes":1700000,"speed":240000.0,"eta":3,"transferring":[{"name":"DSC_4821.jpg","size":565800}]}}"#));
        assert_eq!(
            (log.bytes, log.total, log.rate, log.left),
            (795100, 1700000, 240000, Some(3))
        );
        assert_eq!((log.file.as_str(), log.size), ("DSC_4821.jpg", 565800));
        log.line(r#"{"stats":{"bytes":1,"eta":null}}"#);
        assert_eq!(log.bytes, 795100);
        assert_eq!(log.left, None);
        assert!(!log.line("not JSON"));
        log.line(r#"{"level":"error","object":"bad.jpg","msg":"md5 differ"}"#);
        assert_eq!(log.error, "bad.jpg: md5 differ");
        log.line(r#"{"msg":"1 hashes could not be checked"}"#);
        assert!(log.no_hash);
    }
}
