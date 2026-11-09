//! The legacy fileClipboard reader observes the window's shared selection without claiming ownership.
use super::Clipboard;
use crate::{ipc::Adapter, visual::object};
use flea::{
    ipc::wire::Reply,
    jsondoc::{self, Json},
};

pub(crate) struct ClipboardAdapter<'a> {
    pub board: &'a Clipboard,
}

impl Adapter for ClipboardAdapter<'_> {
    fn call(&self, function: &str, _arguments: &[String]) -> Option<Reply> {
        if function != "fileClipboard" {
            return None;
        }
        let clip = self.board.current();
        let value = object([
            (
                "paths",
                Json::Arr(clip.paths.iter().cloned().map(Json::Str).collect()),
            ),
            ("moving", Json::Bool(clip.moving)),
            ("token", Json::Str(clip.token.clone())),
        ]);
        Some(Reply::value(jsondoc::render_at(&value, 0)))
    }
}
