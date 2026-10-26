//! ViewState.langMarks: whether a code file's row wears its language's logo.
use super::ListView;
use flea::jsondoc::Json;

/// ViewState.langMarks: absent reads on.
pub fn lang_marks(data: &Json) -> bool {
    data.get("langMarks").and_then(Json::as_bool) != Some(false)
}

impl ListView {
    /// A changed langMarks reshapes every row, since a code file's mark changes.
    pub fn set_lang_marks(&mut self, on: bool) {
        if on != self.lang_marks {
            self.lang_marks = on;
            self.text += 1;
        }
    }
}
