// Render the jobs wire and headings from the loop-owned job state.
use super::jobs::{Job, State};
use crate::json::escape;

impl Job {
    pub(super) fn wait_name(&self) -> &str {
        match self.kind.as_str() {
            "copy" => "copy",
            "move" => "move",
            "compress" => "compression",
            "extract" => "extraction",
            _ => "job",
        }
    }

    pub fn head(&self) -> String {
        if self.state == State::Failed {
            return match self.kind.as_str() {
                "copy" => "Copy failed".into(),
                "move" => "Move failed".into(),
                "send" => "Send failed".into(),
                "compress" => "Compression failed".into(),
                "extract" => "Extraction failed".into(),
                "emptyTrash" => "Emptying Trash failed".into(),
                _ => format!("{} failed", self.label),
            };
        }
        if self.phase == "checking" {
            return format!("Checking {} items on {}", self.total, self.target);
        }
        if !self.receipt.is_empty() {
            return self.receipt.clone();
        }
        if self.kind == "extract" && self.total == 0 {
            return if self.done == 0 {
                "Extracting".into()
            } else {
                format!("Extracting {} items", self.done)
            };
        }
        if self.kind == "emptyTrash" {
            return "Emptying Trash".into();
        }
        if self.state == State::Waiting {
            return match self.kind.as_str() {
                "copy" => format!("Copy {} items to {}", self.total, self.target),
                "move" => format!("Move {} items to {}", self.total, self.target),
                "compress" => format!("Compress {} items to {}", self.total, self.target),
                _ => self.label.clone(),
            };
        }
        let at = if self.state == State::Done {
            self.done
        } else {
            self.index
                .map(|i| i + 1)
                .unwrap_or(self.done)
                .min(self.total)
        };
        match self.kind.as_str() {
            "copy" => format!("Copying {} of {} to {}", at, self.total, self.target),
            "move" => format!("Moving {} of {} to {}", at, self.total, self.target),
            "extract" => format!("Extracting {} of {}", at, self.total),
            "compress" => format!("Compressing {} items", self.total),
            "send" => format!("Sending {} of {} to {}", at, self.total, self.target),
            _ => format!("{} · {} of {}", self.label, at, self.total),
        }
    }

    pub fn json(&self) -> String {
        let state = match self.state {
            State::Waiting => "waiting",
            State::Running => "running",
            State::Failed => "failed",
            State::Done => "done",
        };
        let output: Vec<_> = self
            .output
            .iter()
            .map(|line| format!("\"{}\"", escape(line)))
            .collect();
        let bytes = !matches!(self.kind.as_str(), "action" | "emptyTrash");
        format!(
            r#"{{"id":{},"kind":"{}","head":"{}","file":"{}","done":{},"total":{},"bytes":{},"totalBytes":{},"itemBytes":{},"rate":{},"left":{},"state":"{}","reason":"{}","output":[{}],"undo":{},"cancelled":{},"phase":"{}","check":"{}","receipt":"{}","note":"{}"}}"#,
            self.id,
            escape(&self.kind),
            escape(&self.head()),
            escape(&self.file),
            self.done,
            self.total,
            if bytes {
                self.bytes.to_string()
            } else {
                "null".into()
            },
            if bytes {
                self.total_bytes.to_string()
            } else {
                "null".into()
            },
            self.item_bytes,
            self.rate,
            self.left
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".into()),
            state,
            escape(&self.reason),
            output.join(","),
            self.undo,
            self.cancelled,
            escape(&self.phase),
            escape(&self.check),
            escape(&self.receipt),
            if self.cancelled && !self.receipt.is_empty() {
                "not checked"
            } else {
                ""
            }
        )
    }
}
