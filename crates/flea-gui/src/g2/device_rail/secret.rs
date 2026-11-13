//! A passphrase buffer that never reallocates its bytes into freed memory and zeroes them on every exit.
use std::fmt;

/// Bytes that are wiped on clear, on growth and on drop, and never printed.
#[derive(Default)]
pub struct Secret(Vec<u8>);

impl Secret {
    pub fn new() -> Self {
        Self(Vec::with_capacity(256))
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut secret = Self::new();
        secret.extend(bytes);
        secret
    }

    pub fn push_str(&mut self, text: &str) {
        self.extend(text.as_bytes());
    }

    fn extend(&mut self, bytes: &[u8]) {
        if self.0.len() + bytes.len() > self.0.capacity() {
            let mut grown = Vec::with_capacity((self.0.len() + bytes.len()).max(256) * 2);
            grown.extend_from_slice(&self.0);
            wipe(&mut self.0);
            self.0 = grown;
        }
        self.0.extend_from_slice(bytes);
    }

    /// Removes the last character, wiping its bytes.
    pub fn pop(&mut self) {
        let text = std::str::from_utf8(&self.0).unwrap_or("");
        let cut = text.char_indices().last().map_or(0, |(at, _)| at);
        self.0[cut..].fill(0);
        std::hint::black_box(&self.0);
        self.0.truncate(cut);
    }

    pub fn clear(&mut self) {
        wipe(&mut self.0);
    }

    /// Moves the bytes out, leaving this buffer empty.
    pub fn take(&mut self) -> Secret {
        std::mem::replace(self, Secret::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The character count, which is all a mask may draw.
    pub fn chars(&self) -> usize {
        std::str::from_utf8(&self.0).map_or(self.0.len(), |t| t.chars().count())
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    /// The bytes udisks gets: a BitLocker recovery key typed as Windows takes it, 48 digits among any spaces
    /// or dashes, regrouped into the dashed form cryptsetup alone reads. Any other secret passes unchanged.
    pub fn keyed_for(self, crypto: &str) -> Secret {
        let digits = || self.0.iter().filter(|b| b.is_ascii_digit());
        let key_shaped = self
            .0
            .iter()
            .all(|b| b.is_ascii_digit() || *b == b'-' || b.is_ascii_whitespace());
        if crypto != "BitLocker" || !key_shaped || digits().count() != 48 {
            return self;
        }
        let mut grouped = Secret::new();
        for (at, digit) in digits().enumerate() {
            if at > 0 && at % 6 == 0 {
                grouped.extend(b"-");
            }
            grouped.extend(std::slice::from_ref(digit));
        }
        grouped
    }
}

fn wipe(bytes: &mut Vec<u8>) {
    bytes.fill(0);
    std::hint::black_box(&*bytes);
    bytes.clear();
}

impl Drop for Secret {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret({} bytes)", self.0.len())
    }
}
