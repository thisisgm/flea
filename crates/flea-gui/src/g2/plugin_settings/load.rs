use super::view::PluginsPane;
use gpui::Context;
use std::sync::Arc;

const MARK_BYTES: u64 = 64 * 1024;

impl PluginsPane {
    // A community plugin's mark.svg is read once, off the UI thread.
    pub(super) fn load_marks(&mut self, cx: &mut Context<Self>) {
        let wanted: Vec<String> = self
            .page
            .plugins()
            .iter()
            .map(|p| p.mark_path.clone())
            .filter(|p| !p.is_empty() && !self.marks.contains_key(p))
            .collect();
        for path in wanted {
            self.marks.insert(path.clone(), None);
            let read = path.clone();
            cx.spawn(async move |this, cx| {
                let data = cx
                    .background_executor()
                    .spawn(async move { read_mark(&read) })
                    .await;
                let _ = this.update(cx, |pane, cx| {
                    if let Some(data) = data {
                        pane.marks.insert(path, Some(data));
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
}

pub(crate) fn read_mark(path: &str) -> Option<Arc<[u8]>> {
    use std::io::Read;
    let mut data = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MARK_BYTES + 1)
        .read_to_end(&mut data)
        .ok()?;
    (data.len() as u64 <= MARK_BYTES).then(|| Arc::from(data))
}
