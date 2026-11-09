//! Optional viewport facts for TUI folder previews; ordinary ancestor peeks do no row metadata work.
use std::os::unix::fs::MetadataExt;

pub fn fields(base: &str, name: &str, enabled: bool) -> String {
    fields_with(enabled, || {
        flea::bytename::view(base)
            .join(flea::bytename::view(name))
            .symlink_metadata()
            .ok()
            .map(|meta| (meta.len(), meta.mode()))
    })
}
fn fields_with(enabled: bool, stat: impl FnOnce() -> Option<(u64, u32)>) -> String {
    if !enabled {
        return String::new();
    }
    let (size, mode) = stat().unwrap_or((0, 0));
    format!(r#","s":{size},"p":{mode}"#)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_peeks_never_call_the_metadata_reader() {
        assert_eq!(
            fields_with(false, || panic!("ordinary peek read per-entry metadata")),
            ""
        );
    }
    #[test]
    fn a_requested_row_reads_once_and_missing_facts_stay_unavailable() {
        let calls = std::cell::Cell::new(0);
        assert_eq!(
            fields_with(true, || {
                calls.set(calls.get() + 1);
                Some((19, 0o100644))
            }),
            r#","s":19,"p":33188"#
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(fields_with(true, || None), r#","s":0,"p":0"#);
    }
    #[test]
    fn a_raw_named_row_reads_its_own_file_and_never_its_twin() {
        use flea::bytename::ExactText as _;
        use std::os::unix::ffi::OsStrExt;
        let root = crate::backend::testdir::TestDir::new("peekdetail-raw");
        let raw = std::ffi::OsStr::from_bytes(b"row-\xff");
        std::fs::write(root.path().join(raw), "raw!").unwrap();
        std::fs::write(root.path().join("row-\u{fffd}"), "the longer twin").unwrap();
        let line = fields(&root.path().to_string_lossy(), &raw.exact_text(), true);
        assert!(line.starts_with(r#","s":4,"#), "{line}");
    }
}
