use crate::backend::fsinfo::dev_of;
use crate::backend::listing::Listing;
use crate::backend::proto::listed_line;
use crate::backend::run::{forget_rows, write_window};
use crate::backend::searchreq::finish_search;
use crate::backend::state::{State, Tables};
use crate::backend::thumbs::Pool;
use std::fs;
use std::io::{self, BufWriter, Write};
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
    for path in paths {
        // A trust boundary: this list comes from a file every application on the desktop writes.
        let Some(name) = path.strip_prefix('/') else { continue };
        if name.is_empty() {
            continue;
        }
        // The link's own type, the same rule scan() reads off d_type: a symlink to a directory is
        // listed as a file, and a symlink to nothing is still an entry the user put here.
        let Ok(meta) = fs::symlink_metadata(path) else { continue };
        let index = l.len();
        l.push(name, meta.is_dir());
        l.spans[index].is_symlink = meta.file_type().is_symlink();
    }
    (l, t.elapsed().as_secs_f64() * 1000.0)
}

// The wire side, the way searchreq answers a search: one listing built, announced and windowed.
pub fn answer(
    out: &mut BufWriter<io::Stdout>,
    st: &mut State,
    pool: &Pool,
    tb: &Tables,
    paths: &[String],
    first: usize,
    line: &str,
) {
    // A new listing replaces whatever the walk was filling, so the walk ends before the build starts.
    finish_search(out, st, true);
    let (mut l, read_ms) = listing_of(paths);
    super::picker::filter_listing(&mut l, &tb.mime, line);
    // base and listing only move together, exactly as a list moves them.
    st.base = PathBuf::from(BASE);
    st.listing = l;
    forget_rows(st, pool);
    // The sort figure is always zero: nothing here is sorted, see docs/protocol.md "listpaths".
    writeln!(out, "{}", listed_line(st.listing.len(), read_ms, 0.0, dev_of(&st.base))).ok();
    // Rides along unasked, the same first-paint saving a list makes.
    write_window(out, st, 0, first, tb);
    out.flush().ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn lists_the_paths_that_exist_in_the_order_they_were_given() {
        let d = TestDir::new("listpaths");
        let file = d.join("b.txt");
        let dir = d.join("a-dir");
        fs::write(&file, "x").unwrap();
        fs::create_dir(&dir).unwrap();
        let asked = vec![
            file.to_string_lossy().to_string(),
            dir.to_string_lossy().to_string(),
        ];
        let (l, _) = listing_of(&asked);
        assert_eq!(l.len(), 2);
        assert_eq!(l.name(0), file.to_string_lossy().strip_prefix('/').unwrap());
        assert!(!l.is_dir(0));
        assert!(l.is_dir(1));
    }

    #[test]
    fn drops_a_path_that_is_gone_rather_than_listing_it() {
        let d = TestDir::new("listpaths-gone");
        let kept = d.join("kept.txt");
        fs::write(&kept, "x").unwrap();
        let asked = vec![
            d.join("never-existed.txt").to_string_lossy().to_string(),
            kept.to_string_lossy().to_string(),
        ];
        let (l, _) = listing_of(&asked);
        assert_eq!(l.len(), 1);
        assert_eq!(l.name(0), kept.to_string_lossy().strip_prefix('/').unwrap());
    }

    #[test]
    fn refuses_a_path_that_is_not_absolute_and_refuses_the_root_itself() {
        let (l, _) = listing_of(&["etc/hostname".to_string(), "".to_string(), "/".to_string()]);
        assert_eq!(l.len(), 0);
    }

    #[test]
    fn a_broken_symlink_is_still_the_entry_the_user_put_here() {
        let d = TestDir::new("listpaths-link");
        let link = d.join("dangling");
        std::os::unix::fs::symlink(d.join("no-such-target"), &link).unwrap();
        let (l, _) = listing_of(&[link.to_string_lossy().to_string()]);
        assert_eq!(l.len(), 1);
        assert!(!l.is_dir(0));
    }

    #[test]
    fn an_empty_request_answers_an_empty_listing() {
        let (l, ms) = listing_of(&[]);
        assert_eq!(l.len(), 0);
        assert!(ms >= 0.0);
    }
}
