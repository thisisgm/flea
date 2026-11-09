//! Mount one preview subject through its existing native reader.
use super::*;

impl Shell {
    /// One row's subject by its kind, held back until the backend answers what it still needs.
    pub(super) fn show_row(
        &mut self,
        side: usize,
        index: usize,
        row: &RowFacts,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = kinds::quick_look_kind(&row.icon, &row.name, false);
        let subject = |state, body| Subject {
            name: row.name.clone(),
            icon: row.icon.clone(),
            mode: row.mode,
            state,
            shape: Shape::unknown(state),
            body,
        };
        if let Some(mount) = mount::Mount::of(row) {
            if self.show_doc(side, index, row, &path, mount, window, cx) {
                return;
            }
        }
        if state == State::Image {
            self.show_picture(side, index, row, path, window, cx);
            return;
        }
        let token = token();
        let (subject, revealed, line, ask) = match state {
            State::Text => {
                let state = if kinds::is_code(&row.name) {
                    State::Code
                } else {
                    State::Text
                };
                let body = Body::Text {
                    lines: Vec::new(),
                    count: String::new(),
                };
                let line = code_line(token, &path, 1);
                (subject(state, body), false, Some(line), Ask::Code { path })
            }
            _ => {
                let body = self.quick_look_unsupported(row, &path, token, window, cx);
                (subject(State::Unsupported, body), true, None, Ask::Nothing)
            }
        };
        let early = replies::header_path(&ask, &subject.body);
        let host = self.quick_look_host(side, window, cx);
        host.token = token;
        host.row = Some(index);
        host.ask = ask;
        self.show(subject, revealed, line, window, cx);
        self.quick_look_header(token, early, cx);
    }
}
