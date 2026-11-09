//! The work counters the cost tests read; outside tests each count compiles to nothing.

#[cfg(test)]
thread_local! {
    /// Membership steps the list helpers took, read by the cost tests.
    pub(crate) static STEPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Alignments that recorded their positions, read by the cost tests.
    pub(crate) static RECORDED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Whole-name passes: each walk over a name's text or over its loaded copy, read by the cost tests.
    pub(crate) static PASSES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// A name the matcher holds unread: its text comes only through these accessors, and each pass over it is counted.
#[derive(Clone, Copy)]
pub(super) struct Name<'n>(&'n str);

impl<'n> Name<'n> {
    pub(super) fn of(name: &'n str) -> Self {
        Self(name)
    }

    /// The length in bytes, read from the string's metadata, so not a pass.
    pub(super) fn len(self) -> usize {
        self.0.len()
    }

    /// One pass: whether every byte is ASCII.
    pub(super) fn is_ascii(self) -> bool {
        pass();
        self.0.is_ascii()
    }

    /// One pass: the bytes, for a scan or a copy.
    pub(super) fn bytes(self) -> std::str::Bytes<'n> {
        pass();
        self.0.bytes()
    }

    /// One pass: the chars, for a scan or a copy.
    pub(super) fn chars(self) -> std::str::Chars<'n> {
        pass();
        self.0.chars()
    }

    /// The bytes as a slice; each scan of the slice is one pass, counted at its deref.
    pub(super) fn as_bytes(self) -> Scan<'n, [u8]> {
        Scan(self.0.as_bytes())
    }

    /// The whole text; each scan of the text is one pass, counted at its deref.
    pub(super) fn text(self) -> Scan<'n, str> {
        Scan(self.0)
    }
}

/// A name's slice or text: every deref scans it, so every scan is one counted pass, even when the slice is held.
pub(super) struct Scan<'n, T: ?Sized>(&'n T);

impl<T: ?Sized> std::ops::Deref for Scan<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        pass();
        self.0
    }
}

/// One whole-name pass, over a name's text or its loaded copy, counted where the matcher takes it.
pub(super) fn pass() {
    #[cfg(test)]
    PASSES.with(|passes| passes.set(passes.get() + 1));
}

pub(super) fn recorded() {
    #[cfg(test)]
    RECORDED.with(|recorded| recorded.set(recorded.get() + 1));
}

pub(super) fn steps(n: usize) {
    #[cfg(test)]
    STEPS.with(|steps| steps.set(steps.get() + n as u64));
    let _ = n;
}
