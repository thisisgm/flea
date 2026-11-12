use crate::backend::fsinfo::dev_of;
use crate::backend::listing::Listing;
use crate::backend::proto::listed_line;
use crate::backend::run::{forget_rows, write_window};
use crate::backend::searchreq::finish_search;
use crate::backend::state::{Held, State, Tables};
use crate::backend::thumbs::Pool;
use flea::bytename::ExactText as _;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

// A listing built from paths the client names instead of a directory it scans; see docs/protocol.md
// "listpaths". The base is always "/", so every entry is its absolute path with the leading slash
// removed and base.join(name) reaches the same file again in phase 2.
pub const BASE: &str = "/";

// Sample input: ["/home/gm/Pictures/a.png", "/home/gm/Downloads", "relative/no", "/gone"].
// The order the client gave is the order it gets back: the picker's Recent is newest first and a
// sort here would throw that away. An entry that does not exist is dropped rather than listed
// against a failed stat, which is what keeps a stale history row off the picker instead of drawing
// a row whose size and date are both zero.
pub fn listing_of(paths: &[String]) -> (Listing, f64) {
    let t = Instant::now();
    let mut l = Listing::new();
    let body = std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    for path in paths {
        // A trust boundary: this list comes from a file every application on the desktop writes.
        let Some(name) = path.strip_prefix('/') else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        // The link's own type, the same rule scan() reads off d_type: a symlink to a directory is
        // listed as a file, and a symlink to nothing is still an entry the user put here.
        let exact = flea::bytename::exact(path);
        let owned = exact.clone();
        let meta = match super::iomount::call(&exact, &body, "listpaths", move || {
            fs::symlink_metadata(&owned)
        }) {
            Ok(Ok(meta)) => meta,
            _ => continue,
        };
        let index = l.len();
        l.push(name, meta.is_dir());
        l.spans[index].is_symlink = meta.file_type().is_symlink();
    }
    (l, t.elapsed().as_secs_f64() * 1000.0)
}

// The wire side, the way searchreq answers a search: one listing built, announced and windowed.
pub fn answer(
    out: &mut impl Write,
    st: &mut State,
    pool: &Pool,
    tb: &Tables,
    paths: &[String],
    first: usize,
    line: &str,
) {
    let out = &mut super::listwork::requestgen::Writer::new(
        out,
        crate::json::field_usize(line, "pickerGen"),
    );
    // A new listing replaces whatever the walk was filling, so the walk ends before the build starts.
    finish_search(out, st, true);
    let (mut l, read_ms) = listing_of(paths);
    super::picker::filter_listing(&mut l, &tb.mime, line);
    // A history is small, so a re-read over one always names its added plus removed rows.
    let recheck = st.held == Held::ListPaths;
    let changed = if recheck {
        super::listing::changed_count(&st.listing, &l)
    } else {
        0
    };
    // base and listing only move together, exactly as a list moves them.
    st.base = PathBuf::from(BASE);
    st.listing = l;
    // The held listing is a listpaths one now, so a re-read over it names its count.
    st.held = Held::ListPaths;
    forget_rows(st, pool);
    // The sort figure is always zero: nothing here is sorted, see docs/protocol.md "listpaths".
    let listed = listed_line(
        st.listing.len(),
        read_ms,
        0.0,
        dev_of(&st.base),
        &st.base.exact_text(),
        crate::backend::ops::dir_writable(&st.base),
    );
    let listed = if recheck {
        super::proto::with_changed(&listed, changed)
    } else {
        listed
    };
    writeln!(out, "{}", listed).ok();
    // Rides along unasked, the same first-paint saving a list makes.
    write_window(out, st, 0, first, tb);
    out.flush().ok();
}

#[cfg(test)]
#[path = "listpaths_tests.rs"]
mod tests;
