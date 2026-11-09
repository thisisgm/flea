//! Legacy Columns preview readers, and previewBlockKinds(true), project only the currently mounted document.
use super::Adapter;
use crate::window::selection_preview::{docs::Doc, readers::Reader, Mounted};
use flea::ipc::wire::Reply;

pub(crate) struct ColumnsPreview<'a, 'b> {
    pub preview: &'a Reader<'b>,
}

impl Adapter for ColumnsPreview<'_, '_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let column = arguments.first().is_some_and(|flag| flag == "true");
        if !matches!(
            function,
            "columnMarkdownText" | "columnMarkdownView" | "columnArchiveRect"
        ) && !(function == "previewBlockKinds" && column)
        {
            return None;
        }
        let view = self
            .preview
            .view
            .filter(|view| self.preview.active && view.visible);
        let frame = view.and_then(|view| {
            let loading =
                view.pending.is_some() && (!view.port.0.borrow().holding || view.drawn.is_none());
            (!loading).then_some(view.drawn.as_ref()).flatten()
        });
        let value = match (function, frame.map(|frame| &frame.mounted)) {
            ("columnMarkdownText", Some(Mounted::Doc(Doc::Markdown(markdown)))) => {
                let markdown = markdown.read(self.preview.cx);
                markdown_text(
                    markdown.doc.text.as_deref().unwrap_or_default(),
                    markdown.shown_view() == crate::g2::doc_preview::markdown::View::Source,
                )
            }
            ("columnMarkdownView", Some(Mounted::Doc(Doc::Markdown(markdown)))) => {
                let markdown = markdown.read(self.preview.cx);
                if !markdown.active || !markdown.doc.content_ready() {
                    String::new()
                } else {
                    match markdown.shown_view() {
                        crate::g2::doc_preview::markdown::View::Rendered => "rendered",
                        crate::g2::doc_preview::markdown::View::Source => "source",
                    }
                    .to_owned()
                }
            }
            // IpcPreviewState.previewBlockKinds(true): the visible column's Markdown, "" while it loads or shows none.
            ("previewBlockKinds", Some(Mounted::Doc(Doc::Markdown(markdown)))) => {
                crate::ipc::preview_look::block_kinds(markdown.read(self.preview.cx))
            }
            ("columnArchiveRect", Some(Mounted::Doc(Doc::Archive(archive)))) => {
                let archive = archive.read(self.preview.cx);
                view.filter(|_| archive.meta.as_ref().is_some_and(|meta| !meta.failed))
                    .and_then(|view| *view.frame_rect.borrow())
                    .map_or(String::new(), |bounds| {
                        let inset = gpui::px(archive.metrics.inset);
                        format!(
                            "{} {} {} {}",
                            f32::from(bounds.left() + inset).round(),
                            f32::from(bounds.top() + inset).round(),
                            f32::from(bounds.size.width - inset * 2.).max(0.).round(),
                            f32::from(bounds.size.height - inset * 2.).max(0.).round()
                        )
                    })
            }
            _ => String::new(),
        };
        Some(Reply::value(value))
    }
}

fn markdown_text(body: &str, source: bool) -> String {
    if source {
        return body.to_owned();
    }
    body.split('\n')
        .map(|line| {
            let hashes = line.bytes().take_while(|&byte| byte == b'#').count();
            if !(1..=6).contains(&hashes) {
                return line;
            }
            let rest = &line[hashes..];
            match rest.chars().next() {
                None => "",
                Some(space)
                    if (space.is_whitespace() && space != '\u{85}') || space == '\u{feff}' =>
                {
                    &rest[space.len_utf8()..]
                }
                _ => line,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    #[test]
    fn pb_columns2_markdown_reader_keeps_the_legacy_heading_and_source_contract() {
        let raw = "# Notes\n###\ntext\n##  spaced\n####### literal\n#not-heading\n";
        assert_eq!(
            super::markdown_text(raw, false),
            "Notes\n\ntext\n spaced\n####### literal\n#not-heading\n"
        );
        assert_eq!(super::markdown_text(raw, true), raw);
        assert_eq!(super::markdown_text("#\u{feff}Title", false), "Title");
        assert_eq!(super::markdown_text("#\u{85}Title", false), "#\u{85}Title");
    }
}
