//! A container kind checked against its own bytes before any conversion is asked.
use super::OfficeView;
use flea::ui_logic_doc_preview::office_sniff;
use gpui::Context;
use std::os::unix::fs::FileExt;

impl OfficeView {
    /// A new subject: the last one's request is left, and a container kind's bytes are checked before it is asked.
    pub fn set_path(&mut self, path: &str, window: &mut gpui::Window, cx: &mut Context<Self>) {
        self.leave(window, cx);
        self.thumb = None;
        self.meta = (None, None);
        self.office.open(path, 0);
        if office_sniff::is_container_path(path) {
            self.check_container(cx);
        } else {
            self.ask();
        }
        cx.notify();
    }

    // Asks the backend for the subject in place, under the id it returns.
    pub(crate) fn ask(&mut self) {
        let (path, sheets) = (self.office.path.clone(), self.office.wants_sheets());
        let id = self
            .host
            .as_ref()
            .map_or(0, |host| host.office(&path, sheets));
        self.office.asked(id);
    }

    // The container check off this thread: a real container asks, a fake falls to the unsupported card unasked.
    pub(crate) fn check_container(&mut self, cx: &mut Context<Self>) {
        let path = self.office.path.clone();
        self.check = Some(cx.spawn(async move |this, cx| {
            let read = path.clone();
            let real = cx
                .background_executor()
                .spawn(async move { container_file(&read) })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.office.path == path && view.office.id == 0 {
                    if real {
                        view.ask();
                    } else {
                        view.office.reject_container();
                    }
                }
                view.check = None;
                cx.notify();
            });
        }));
    }
}

// A regular file's head and tail checked; anything else keeps the conversion's answer, since the backend checks only the name.
fn container_file(path: &str) -> bool {
    let Some(file) = flea::backend::regfile::open_regular(&flea::bytename::view(path)) else {
        return true;
    };
    let size = file.metadata().map_or(0, |m| m.len());
    office_sniff::is_real_container(path, size, &mut |at, len| {
        let mut buf = vec![0; len];
        let read = file.read_at(&mut buf, at).ok()?;
        buf.truncate(read);
        Some(buf)
    })
}
