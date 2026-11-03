//! The compress half of archiveCardsState, in ArchiveCompressDialog.qml's fields and never a typed character.
use super::*;
use flea::json::escape;

impl Compress {
    /// Which field wears the error frame: Password while it is empty, Confirm while the pair differs.
    pub(super) fn field_errors(&self) -> (bool, bool) {
        let error = self.error.as_deref().is_some_and(|e| !e.is_empty());
        (error && self.password.is_empty(), error && !self.matched())
    }

    /// v0.3.12's focusPart: a text field 0, Format 1, Encrypt 4, Cancel 5, Compress 6.
    fn focus_part(&self) -> u8 {
        match self.focus {
            Stop::Format => 1,
            Stop::Encrypt => 4,
            Stop::Cancel => 5,
            Stop::Compress => 6,
            Stop::Name | Stop::Password | Stop::Confirm => 0,
        }
    }

    /// The compress fields; eye is the painted Password eye centre "x y", empty before the card paints.
    pub fn reading(&self, eye: &str) -> String {
        let (password_error, confirm_error) = self.field_errors();
        format!(
            "\"compressOpened\":{},\"compressCount\":{},\"compressFormat\":\"{}\",\"compressError\":\"{}\",\"compressFocus\":{},\"canCompress\":{},\"compressCapture\":{{\"encrypt\":{},\"encryptShown\":{},\"revealed\":{},\"eye\":\"{}\",\"passwordError\":{},\"confirmError\":{}}}",
            self.opened,
            self.paths.len(),
            escape(&self.format),
            escape(self.error.as_deref().unwrap_or("")),
            self.focus_part(),
            self.can_compress(),
            self.encrypt_names,
            self.format == "7z",
            self.held_password,
            escape(eye),
            password_error,
            confirm_error,
        )
    }
}
