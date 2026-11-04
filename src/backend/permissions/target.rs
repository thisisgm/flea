//! Batch targets resolve relative changes from the mode captured immediately before the write.
pub(super) enum Target {
    Absolute(u32),
    OwnerExecute,
}

impl Target {
    // Sample input: "u+x" adds owner execute; "0644" sets an absolute ordinary mode.
    pub(super) fn parse(text: &str) -> Result<Self, String> {
        if text == "u+x" {
            Ok(Self::OwnerExecute)
        } else {
            mode(text).map(Self::Absolute)
        }
    }

    pub(super) fn resolve(&self, before: u32) -> u32 {
        match self {
            Self::Absolute(bits) => *bits,
            Self::OwnerExecute => before | 0o100,
        }
    }
}

// Sample input: "644" or "0644"; special bits and partial/whitespace inputs are not ordinary modes.
pub(super) fn mode(text: &str) -> Result<u32, String> {
    let ordinary = text.len() == 3 || (text.len() == 4 && text.starts_with('0'));
    if !ordinary || !text.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return Err("Enter three octal digits or a leading-zero four-digit mode.".into());
    }
    u32::from_str_radix(text, 8).map_err(|e| e.to_string())
}
