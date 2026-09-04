// Reading one archive's index into a Contents, bounded by a deadline. How each tool shapes the lines
// it reads is archivespec.rs.
#[derive(Default, PartialEq, Debug)]
pub struct Contents {
    // Exact whenever they are present at all: the index is streamed, so counting all of it costs no
    // memory, and a read that failed or ran out of time clears them rather than leaving a partial
    // count on a wire that calls the number a total.
    pub entries: usize,
    pub unpacked: u64,
    // Members that should have produced something in the destination, counted for every line rather
    // than only the named ones: every member except the archive root itself. See
    // Row::produces_destination_entry for the invariant, which is what an extract is checked against.
    pub produced_entries: usize,
    // Only the first ARCHIVE_NAME_CAP, because a frame draws a handful and the wire carries no more.
    pub names: Vec<Entry>,
    // The tool could not read the archive at all, which is a different thing from an empty one.
    pub failed: bool,
}

#[derive(PartialEq, Debug, Clone)]
pub struct Entry {
    pub name: String,
    // Drawn as the folder or file mark on the Archive tile's own rows; the extract predicate asks
    // Row::produces_destination_entry instead, because a nested directory is a destination entry too.
    pub is_dir: bool,
}

// A frame draws a handful of rows, so only that many names ever reach the wire. The count above is
// not capped: it is exact, and the difference is what the tile's "+ N more" line states.
pub const ARCHIVE_NAME_CAP: usize = 64;

// The read is bounded in time rather than in entries, because the cost this bound exists to stop is
// wall clock on a cursor move, not memory: the parser already holds one line at a time. An index
// that does not finish inside this is reported as unread rather than as a total nobody can trust.
pub const ARCHIVE_READ_MS: u64 = 2000;

// Checked every line, not every batch. Per batch the first evaluation lands at entries == 0, before
// a line has been read, so an archive under one batch was checked exactly once and never again; and
// Instant::elapsed is a vDSO read, tens of nanoseconds against a parse of hundreds.

// Read a line at a time off the child's own stdout, so a 200k-entry index is never held in memory
// and is still counted in full. read_until, not BufRead::lines: lines() answers Err on a filename
// that is not UTF-8, and a break there would report a short count as though it were the total.
use crate::backend::archivespec::{row_of, ListSpec};

#[cfg(test)]
pub fn parse_reader<R: std::io::BufRead>(reader: R, spec: &ListSpec) -> Contents {
    parse_until(reader, spec, std::time::Duration::from_millis(ARCHIVE_READ_MS))
}

pub fn parse_until<R: std::io::BufRead>(mut reader: R, spec: &ListSpec, budget: std::time::Duration) -> Contents {
    let started = std::time::Instant::now();
    let mut c = Contents::default();
    let mut buf: Vec<u8> = Vec::with_capacity(256);
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => {
                c.failed = true;
                break;
            }
        }

        // After the read, not before it: checked before, the first evaluation lands at entries == 0
        // and an archive under one batch was never checked again. This bounds the parse and a slow
        // but producing tool; a tool that stops producing blocks inside read_until, which is what
        // the caller's watchdog kills.
        if started.elapsed() > budget {
            c.failed = true;
            break;
        }

        let line = String::from_utf8_lossy(&buf);
        let line = line.trim_end_matches(['\n', '\r']);
        if line.trim().is_empty() {
            continue;
        }
        c.entries += 1;
        // One read of the line, borrowed: the owned Entry is only built for the few that reach the
        // wire, so a 200k index allocates for the cap and not for every row.
        let row = row_of(line, spec);
        if let Some(r) = &row {
            if r.produces_destination_entry() {
                c.produced_entries += 1;
            }
        }
        // A column that is not a number is a directory row or a format this does not know; it counts
        // as an entry and adds nothing, which is honest rather than wrong.
        if let Some(field) = line.split_whitespace().nth(spec.size_column) {
            c.unpacked += field.parse::<u64>().unwrap_or(0);
        }
        if c.names.len() < ARCHIVE_NAME_CAP {
            if let Some(r) = row {
                c.names.push(Entry { name: r.name.to_string(), is_dir: r.is_dir });
            }
        }
    }
    // A count that stopped early is not a total, and this wire calls entries exact, so a failed read
    // carries nothing rather than something a reader would have to know not to trust.
    if c.failed {
        clear_counts(&mut c);
    }
    c
}

// A count that stopped early is not a total, so a failed read carries nothing. Called here and again
// by the caller, which learns the tool's exit status only after this function has returned.
pub fn clear_counts(c: &mut Contents) {
    c.entries = 0;
    c.unpacked = 0;
    c.produced_entries = 0;
    c.names.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::archivespec::{seven_spec, tar_spec};

    // Real output, captured on this box from bsdtar -tvf and 7z l -ba against a fixture holding a
    // directory, a nested directory and a name with spaces in it.
    const TAR_LISTING: &str = "\
drwxr-xr-x  0 gm     gm          0 Sep  1 12:10 ./
-rw-r--r--  0 gm     gm          2 Sep  1 12:10 ./name with spaces.txt
-rw-r--r--  0 gm     gm          1 Sep  1 12:10 ./a.txt
drwxr-xr-x  0 gm     gm          0 Sep  1 12:10 ./sub/
-rw-r--r--  0 gm     gm          3 Sep  1 12:10 ./sub/b.txt
";

    // The packed column is blank for a solid-block member, which is why the name cannot be found by
    // counting fields here the way it is for bsdtar.
    const SEVEN_LISTING: &str = "\
2026-09-01 12:10:26 D....            0            0  arcfmt
2026-09-01 12:10:26 D....            0            0  arcfmt/sub
2026-09-01 12:10:26 ....A            1           10  arcfmt/a.txt
2026-09-01 12:10:26 ....A            2               arcfmt/name with spaces.txt
2026-09-01 12:10:26 ....A            3               arcfmt/sub/b.txt
";

    fn parse(text: &str, spec: &ListSpec) -> Contents {
        parse_reader(std::io::BufReader::new(text.as_bytes()), spec)
    }

    #[test]
    fn a_tar_listing_gives_an_exact_count_a_total_and_its_first_names() {
        let c = parse(TAR_LISTING, &tar_spec());
        assert_eq!(c.entries, 5, "every row counts, directories included");
        assert_eq!(c.unpacked, 6, "1 + 2 + 3, the directories adding nothing");
        assert!(!c.failed);
        let names: Vec<&str> = c.names.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec![".", "./name with spaces.txt", "./a.txt", "./sub", "./sub/b.txt"]);
        assert_eq!(c.names[0].is_dir, true, "the mode column is what says directory");
        assert_eq!(c.names[1].is_dir, false);
        assert_eq!(c.names[3].is_dir, true);
    }

    #[test]
    fn a_7z_listing_gives_the_same_shape_despite_its_blank_packed_column() {
        let c = parse(SEVEN_LISTING, &seven_spec());
        assert_eq!(c.entries, 5);
        assert_eq!(c.unpacked, 6);
        let names: Vec<&str> = c.names.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["arcfmt", "arcfmt/sub", "arcfmt/a.txt", "arcfmt/name with spaces.txt", "arcfmt/sub/b.txt"]);
        assert_eq!(c.names[0].is_dir, true, "the attribute column is what says directory");
        assert_eq!(c.names[2].is_dir, false);
    }

    // BufRead::lines() answers Err on a filename that is not UTF-8, and breaking there reported a
    // short count as though it were the total. The count has to survive the byte, not stop at it.
    #[test]
    fn a_filename_that_is_not_utf8_is_still_counted_and_still_summed() {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"-rw-r--r--  0 gm gm 10 Sep  1 09:14 a.txt\n");
        bytes.extend_from_slice(b"-rw-r--r--  0 gm gm 20 Sep  1 09:14 caf\xe9.txt\n");
        bytes.extend_from_slice(b"-rw-r--r--  0 gm gm 30 Sep  1 09:14 c.txt\n");
        let c = parse_reader(std::io::BufReader::new(&bytes[..]), &tar_spec());
        assert_eq!(c.entries, 3, "a latin-1 name must not truncate the count");
        assert_eq!(c.unpacked, 60, "nor the total");
        assert!(!c.failed);
        assert_eq!(c.names.len(), 3);
    }

    // The count is exact however long the listing is; only the name list is capped, and the tile's
    // "+ N more" line is the difference.
    #[test]
    fn only_the_name_list_is_capped_and_the_count_stays_exact() {
        let mut text = String::new();
        for i in 0..(ARCHIVE_NAME_CAP + 500) {
            text.push_str(&format!("-rw-r--r--  0 gm gm 10 Sep  1 09:14 f{}.txt\n", i));
        }
        let c = parse(&text, &tar_spec());
        assert_eq!(c.entries, ARCHIVE_NAME_CAP + 500, "the count is every entry, never the cap");
        assert_eq!(c.unpacked, ((ARCHIVE_NAME_CAP + 500) as u64) * 10, "and so is the total");
        assert_eq!(c.names.len(), ARCHIVE_NAME_CAP, "only the names stop at the cap");
    }

    // The bound is wall clock, because the cost it exists to stop is time on a cursor move; the
    // parser already holds one line at a time, so memory was never the remaining problem.
    // A zero budget trips on the first evaluation and proves only that the flag can be set, so the
    // count is asserted too: a trip has to leave nothing behind, because a partial count on a wire
    // that calls the number exact is worse than no count.
    #[test]
    fn an_index_that_outruns_its_budget_is_reported_unread_rather_than_as_a_total() {
        let mut text = String::new();
        for i in 0..2000 {
            text.push_str(&format!("-rw-r--r--  0 gm gm 10 Sep  1 09:14 f{}.txt\n", i));
        }
        let c = parse_until(std::io::BufReader::new(text.as_bytes()), &tar_spec(), std::time::Duration::from_millis(0));
        assert!(c.failed, "a read that ran out of time did not read the archive");
        assert_eq!(c.entries, 0, "a trip leaves no count behind for the wire to call exact");
        assert_eq!(c.unpacked, 0);
        assert!(c.names.is_empty());

        // Every entry is checked, so a budget that expires partway still leaves nothing.
        let mid = parse_until(std::io::BufReader::new(text.as_bytes()), &tar_spec(), std::time::Duration::from_nanos(1));
        assert!(mid.failed);
        assert_eq!(mid.entries, 0);

        let ok = parse_until(std::io::BufReader::new(text.as_bytes()), &tar_spec(), std::time::Duration::from_secs(60));
        assert!(!ok.failed);
        assert_eq!(ok.entries, 2000, "and a read inside its budget is still exact");
    }

    // A reader that hands back FAST_LINES instantly and then stalls, so the deadline is crossed after
    // entries have been counted rather than before the first one. A reader that merely paces itself
    // cannot do that: the check sits above the increment, so any per-line cost over the budget trips
    // at entries == 0 and leaves the clearing this reader exists for unexercised.
    struct SlowReader {
        lines: Vec<String>,
        at: usize,
        fast_lines: usize,
        stall: std::time::Duration,
        // How many lines the parser actually took, which is what tells a mid-stream trip from a trip
        // on line one. Shared, because parse_until owns the reader and never gives it back.
        consumed: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl std::io::Read for SlowReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            unreachable!("read_until goes through fill_buf")
        }
    }

    impl std::io::BufRead for SlowReader {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            if self.at >= self.lines.len() {
                return Ok(&[]);
            }
            if self.at >= self.fast_lines {
                std::thread::sleep(self.stall);
            }
            Ok(self.lines[self.at].as_bytes())
        }
        fn consume(&mut self, amt: usize) {
            if amt > 0 {
                self.at += 1;
                self.consumed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    // The one case a zero budget cannot reach: a trip after some entries have been counted, which is
    // the only state the clearing exists for. Three lines are free and the fourth stalls for ten
    // times the budget, so the parser counts three, trips on the fourth and clears them.
    #[test]
    fn a_trip_partway_through_a_listing_still_carries_nothing() {
        let free_lines = 3;
        let budget = std::time::Duration::from_millis(100);
        let lines: Vec<String> = (0..20).map(|i| format!("-rw-r--r--  0 gm gm 10 Sep  1 09:14 f{}.txt\n", i)).collect();
        let consumed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let reader = SlowReader { lines: lines.clone(), at: 0, fast_lines: free_lines, stall: budget * 10, consumed: consumed.clone() };
        let c = parse_until(reader, &tar_spec(), budget);
        assert!(c.failed, "a listing that outran its budget is a failed read");
        assert_eq!((c.entries, c.produced_entries, c.unpacked), (0, 0, 0), "and it carries no partial count, however far it got");
        assert!(c.names.is_empty());

        // Without this the test is vacuous: a stall on line one would trip at entries == 0 and the
        // zeroes above would pass with clear_counts deleted. The free lines cost microseconds, so
        // this count is exact rather than a range, and a box stalled through them fails here loudly
        // instead of passing quietly.
        assert_eq!(consumed.load(std::sync::atomic::Ordering::Relaxed), free_lines + 1, "the trip has to land after entries were counted, or nothing was cleared");

        let unhurried = SlowReader { lines, at: 0, fast_lines: 20, stall: budget, consumed: consumed.clone() };
        let full = parse_until(unhurried, &tar_spec(), std::time::Duration::from_secs(60));
        assert!(!full.failed);
        assert_eq!(full.entries, 20, "the same reader counts every line when it is not cut short");
    }

    // The trap this pairing sets for a caller: a failed read reports zero of everything, so a count
    // of zero means "holds nothing" and "could not be read" at once and only .failed tells them
    // apart. Reading the second as the first is what restored the extract defect once already.
    #[test]
    fn a_failed_read_reports_zero_of_everything_so_a_caller_must_read_failed_first() {
        let text = "-rw-r--r--  0 gm gm 10 Sep  1 09:14 a.txt\n-rw-r--r--  0 gm gm 20 Sep  1 09:14 b.txt\n";
        let tripped = parse_until(std::io::BufReader::new(text.as_bytes()), &tar_spec(), std::time::Duration::from_millis(0));
        let empty = parse_until(std::io::BufReader::new("".as_bytes()), &tar_spec(), std::time::Duration::from_secs(60));
        assert_eq!((tripped.entries, tripped.produced_entries), (empty.entries, empty.produced_entries), "the counts alone cannot tell a truncated read from an empty archive");
        assert!(tripped.failed && !empty.failed, "only this flag can");
    }

    // A symlink lists with an l and a hardlink with an h, and neither is the archive root, so the
    // invariant says each produces a destination entry. Measured on this box: a three-member archive
    // of one file, one symlink and one hardlink extracts to three entries.
    #[test]
    fn a_symlink_and_a_hardlink_each_produce_a_destination_entry() {
        let listing = "drwxr-xr-x  0 gm gm 0 Sep  1 15:32 ./\n\
                       -rw-r--r--  0 gm gm 4 Sep  1 15:32 ./real.txt\n\
                       lrwxrwxrwx  0 gm gm 0 Sep  1 15:32 ./link.txt -> real.txt\n\
                       hrw-r--r--  0 gm gm 0 Sep  1 15:32 ./hard.txt link to ./real.txt\n";
        let c = parse(listing, &tar_spec());
        assert_eq!(c.entries, 4);
        assert_eq!(c.produced_entries, 3, "the root produces nothing; the other three each produce one");
    }

    // The invariant, in the three shapes that each broke a previous predicate. Counting entries
    // missed the root; counting non-directory members missed a nested directory, which extract does
    // produce; counting members that are not the root gets all three right.
    #[test]
    fn every_member_but_the_archive_root_produces_a_destination_entry() {
        let c = parse(TAR_LISTING, &tar_spec());
        assert_eq!(c.entries, 5, "every row is an entry");
        assert_eq!(c.produced_entries, 4, "and every one but ./ produces something");

        let root_only = "drwxr-xr-x  0 gm gm 0 Sep  1 09:14 ./\n";
        let r = parse(root_only, &tar_spec());
        assert_eq!((r.entries, r.produced_entries), (1, 0), "a ./-only archive extracts to nothing, legally");

        // The case a file count called empty while extract produced two directories.
        let dirs = "drwxr-xr-x  0 gm gm 0 Sep  1 09:14 ./\n\
                    drwxr-xr-x  0 gm gm 0 Sep  1 09:14 ./a/\n\
                    drwxr-xr-x  0 gm gm 0 Sep  1 09:14 ./b/\n\
                    drwxr-xr-x  0 gm gm 0 Sep  1 09:14 ./b/c/\n";
        let d = parse(dirs, &tar_spec());
        assert_eq!((d.entries, d.produced_entries), (4, 3), "nested directories are destination entries even though they are directories");

        // 7z emits no root member at all, measured on this box: from inside a directory it lists
        // "a", "b", "f.txt" and naming one it lists "src", "src/a", where the top name is itself a
        // destination entry. So every row produces one and the root exclusion never fires here.
        let seven = parse(SEVEN_LISTING, &seven_spec());
        assert_eq!((seven.entries, seven.produced_entries), (5, 5));
        let seven_from_inside = "2026-09-01 16:13:05 D....            0            0  a\n\
                                 2026-09-01 16:13:05 D....            0            0  b\n\
                                 2026-09-01 16:13:05 ....A            1            5  f.txt\n";
        let inside = parse(seven_from_inside, &seven_spec());
        assert_eq!((inside.entries, inside.produced_entries), (3, 3), "no root to exclude, so the count is every member");

        // The root has more than one spelling. bsdtar writes "././" for `tar -c -C dir ./.`, measured
        // on this box, and a predicate that trimmed one trailing slash read it as "./." and refused a
        // legal extract that had produced exactly what it should.
        let spellings = "drwxr-xr-x  0 gm gm 0 Sep  1 16:20 ././\n\
                         drwxr-xr-x  0 gm gm 0 Sep  1 16:20 .//\n\
                         drwxr-xr-x  0 gm gm 0 Sep  1 16:20 ./././\n";
        let s = parse(spellings, &tar_spec());
        assert_eq!((s.entries, s.produced_entries), (3, 0), "every spelling of the root produces nothing, not just the two-character one");
    }

    #[test]
    fn an_empty_listing_is_empty_and_not_a_failure() {
        let c = parse("", &tar_spec());
        assert_eq!(c.entries, 0);
        assert!(c.names.is_empty());
        assert!(!c.failed, "an archive holding nothing is not an archive that could not be read");
    }

    #[test]
    fn a_line_whose_column_is_not_a_number_still_counts_as_an_entry() {
        // A row this parser does not understand adds nothing rather than a wrong number.
        let c = parse("garbage line with no size here now\n", &tar_spec());
        assert_eq!((c.entries, c.unpacked), (1, 0));
        assert_eq!(parse("\n  \n", &tar_spec()).entries, 0, "blank lines are not entries");

        // A member whose whole name is slashes trims away to nothing. It is an entry in the index and
        // it is not a preview row, because a row drawn with no label is worse than no row.
        let slashes = parse("drwxr-xr-x  0 gm gm 0 Sep  1 09:14 //\n", &tar_spec());
        assert_eq!((slashes.entries, slashes.produced_entries), (1, 0));
        assert!(slashes.names.is_empty(), "a nameless member never reaches the wire");
    }
}
