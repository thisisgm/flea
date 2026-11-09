//! Read original bytes only when touched, off the UI thread and within the preview's storage gate.
use super::CodeView;
use gpui::{AppContext, Context};
use std::io::Read;

fn read(path: &str, limit: u64) -> Option<String> {
    let file = flea::backend::regfile::open_regular(&flea::bytename::view(path))?;
    if file.metadata().ok()?.len() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > limit || bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

impl CodeView {
    pub(super) fn start_selection_read(&mut self, cx: &mut Context<Self>) {
        if self.lines.from_text
            || self.selection_original.is_some()
            || self.selection_read.is_some()
        {
            return;
        }
        let path = self.lines.path.clone();
        let limit = self.max_bytes;
        let read = cx.background_spawn(async move { read(&path, limit) });
        self.selection_read = Some(cx.spawn(async move |this, cx| {
            let text = read.await;
            let _ = this.update(cx, |view, cx| {
                let text = text.filter(|text| view.same_selection_source(text));
                if text.is_some() {
                    view.selection.chunks.clear();
                }
                view.selection_original =
                    Some(text.as_deref().map(super::selection::document_lines));
                view.selection_read = None;
                view.selection.set_pending(false);
                view.selection_chunks();
                view.selection.complete_copy(cx);
                cx.notify();
            });
        }));
    }

    fn same_selection_source(&self, text: &str) -> bool {
        let rows: Vec<&str> = text.split_inclusive('\n').collect();
        if rows.len() != self.lines.total {
            return false;
        }
        let (from, to) = self.lines.needed(self.view());
        let unchanged = self.selection.chunks.iter().all(|(n, chunk)| {
            rows.get(*n).is_some_and(|raw| {
                let original = crate::g2::doc_preview::selection::Chunk::new((*raw).to_owned());
                original.text.trim_end_matches(['\r', '\n'])
                    == chunk.text.trim_end_matches(['\r', '\n'])
            })
        });
        unchanged
            && (from..=to).all(|n| {
                self.lines.row(n).is_none_or(|runs| {
                    let shown: String = runs.iter().map(|(_, text)| text.as_str()).collect();
                    rows.get(n - 1).is_some_and(|raw| {
                        raw.trim_end_matches('\n')
                            .strip_suffix('\r')
                            .unwrap_or(raw.trim_end_matches('\n'))
                            == shown
                    })
                })
            })
    }
}

#[test]
fn gm_prevsel_original_read_is_bounded_and_rejects_nonregular_files() {
    assert!(read("/dev/null", 65536).is_none());
    assert!(read("/proc/self/status", 1).is_none());
    assert!(read("/proc/self/status", 65536)
        .unwrap()
        .starts_with("Name:"));
}
