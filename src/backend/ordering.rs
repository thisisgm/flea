use super::dirsize::{sort_walk, walked_bytes, DirSize, SORT_BUDGET_MS};
use super::extclass::classify;
use super::listing::Listing;
use super::meta::stat_all;
use super::mime::Db;
use super::sort::{cmp_extension, name_order, parse_sort_by, sort_listing};
use std::cmp::Ordering;
use std::path::Path;
use std::time::{Duration, Instant};

// Sample input: {"c":"list","by":"name","desc":false,"foldersFirst":true,"groupByKind":false,"hiddenLast":false}
pub fn request(
    l: &mut Listing,
    base: &Path,
    mime: &Db,
    line: &str,
) -> Result<(f64, f64, Vec<Option<DirSize>>), &'static str> {
    if !l.outline_open.is_empty() { return super::outline::ordered(l, base, mime, line); }
    let value = crate::jsondoc::parse(line).map_err(|_| "invalid ordering request")?;
    let default_by = if value.get("c").and_then(|v| v.as_str()) == Some("sort") { "" } else { "name" };
    let by = value.get("by").and_then(|v| v.as_str()).unwrap_or(default_by);
    let desc = value.get("desc").and_then(|v| v.as_bool()).unwrap_or(false);
    let folders = value
        .get("foldersFirst")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let groups = value
        .get("groupByKind")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // Absent is today's order, so an older client that never names it keeps it.
    let hidden_last = value
        .get("hiddenLast")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if by == "size" && value.get("foldersByName").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(super::tuifolders::sort(l, base, desc, hidden_last));
    }
    ordered(l, base, mime, by, desc, folders, groups, hidden_last)
}

// The default retains the shipped fast path; explicit grouping needs only filename MIME lookup.
pub fn ordered(
    l: &mut Listing,
    base: &Path,
    mime: &Db,
    by: &str,
    desc: bool,
    folders: bool,
    groups: bool,
    hidden_last: bool,
) -> Result<(f64, f64, Vec<Option<DirSize>>), &'static str> {
    let by = if by == "date" { "mtime" } else { by };
    if matches!(by, "btime" | "extension" | "random") {
        let (pass, sort) = super::paritysort::sort(l, base, by, desc, folders, hidden_last);
        return Ok((pass, sort, Vec::new()));
    }
    if !["name", "size", "mtime", "kind"].contains(&by) {
        return Err("no such sort key; send name, size, mtime or kind");
    }
    if by != "kind" && folders && !groups {
        return Ok(sort_listing(l, base, parse_sort_by(by)?, desc, hidden_last));
    }
    let (stats, mut pass_ms) = if by == "size" || by == "mtime" {
        let (stats, ms) = stat_all(base, l);
        (Some(stats), ms)
    } else {
        (None, 0.0)
    };
    // One shared deadline bounds the whole folder pass, so a size sort costs about one slow row.
    let walked: Vec<Option<DirSize>> = if by == "size" {
        let t = Instant::now();
        let walked = sort_walk(base, classify(base), l, stats.as_ref().unwrap(), t + Duration::from_millis(SORT_BUDGET_MS));
        pass_ms += t.elapsed().as_secs_f64() * 1000.0;
        walked
    } else {
        Vec::new()
    };
    let start = Instant::now();
    let kinds: Vec<&str> = if by == "kind" || groups {
        (0..l.len())
            .map(|i| {
                if l.is_dir(i) {
                    "inode/directory"
                } else {
                    mime.lookup(l.name(i)).unwrap_or("application/octet-stream")
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut indices: Vec<usize> = (0..l.len()).collect();
    indices.sort_by(|&a, &b| {
        // Hidden entries follow every visible one in both directions, outside every other grouping, and no partition here reverses.
        if hidden_last {
            match (super::sort::is_hidden(l.name(a)), super::sort::is_hidden(l.name(b))) {
                (true, false) => return Ordering::Greater,
                (false, true) => return Ordering::Less,
                _ => {}
            }
        }
        let group = if groups {
            group_rank(l.is_dir(a), kinds[a]).cmp(&group_rank(l.is_dir(b), kinds[b]))
        } else if folders {
            l.is_dir(b).cmp(&l.is_dir(a))
        } else {
            Ordering::Equal
        };
        if group != Ordering::Equal {
            return group;
        }
        let key = match by {
            "kind" => kinds[a].cmp(kinds[b]),
            "size" => {
                let stats = stats.as_ref().unwrap();
                // A folder orders by its walked recursive size, the same number dirsized reports.
                let sa = if l.is_dir(a) { walked_bytes(&walked, a) } else { stats[a].size };
                let sb = if l.is_dir(b) { walked_bytes(&walked, b) } else { stats[b].size };
                sa.cmp(&sb)
            }
            "mtime" => {
                let stats = stats.as_ref().unwrap();
                stats[a].mtime.cmp(&stats[b].mtime)
            }
            _ => Ordering::Equal,
        };
        let order = key.then_with(|| {
            // Directories keep name order so folders are unaffected; files break a kind tie by extension first.
            if by == "kind" && !l.is_dir(a) && !l.is_dir(b) {
                cmp_extension(l.name(a), l.name(b))
            } else {
                Ordering::Equal
            }
        }).then_with(|| name_order(l.name(a).as_bytes(), l.name(b).as_bytes()));
        if desc {
            order.reverse()
        } else {
            order
        }
    });
    l.spans = indices.iter().map(|&i| l.spans[i]).collect();
    // Final row order, so the caller seeds its answered-row cache instead of walking them again.
    let seed: Vec<Option<DirSize>> = if by == "size" {
        indices.iter().map(|&i| walked[i]).collect()
    } else {
        Vec::new()
    };
    Ok((pass_ms, start.elapsed().as_secs_f64() * 1000.0, seed))
}

fn group_rank(directory: bool, mime: &str) -> u8 {
    if directory {
        0
    } else if mime.starts_with("image/") {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn sort_requires_a_key_while_list_keeps_its_default() {
        let d = TestDir::new("sort-key");
        let mut listing = Listing::new();
        listing.push("z.txt", false);
        listing.push("a.txt", false);
        let db = Db::from_str("50:text/plain:*.txt\n");
        for line in [
            r#"{"c":"sort"}"#,
            r#"{"c":"sort","by":null}"#,
            r#"{"c":"sort","by":true}"#,
            r#"{"c":"sort","by":""}"#,
            r#"{"c":"sort","by":"mode"}"#,
            r#"{"c":"sort","by":"mode","foldersFirst":false}"#,
            r#"{"c":"sort","by":"mode","groupByKind":true}"#,
        ] {
            assert_eq!(request(&mut listing, d.path(), &db, line).unwrap_err(),
                "no such sort key; send name, size, mtime or kind");
            assert_eq!((listing.name(0), listing.name(1)), ("z.txt", "a.txt"),
                "a refused sort leaves the existing order intact: {}", line);
        }
        request(&mut listing, d.path(), &db, r#"{"c":"list"}"#).unwrap();
        assert_eq!((listing.name(0), listing.name(1)), ("a.txt", "z.txt"));
    }

    #[test]
    fn folder_toggle_and_grouping_change_real_order() {
        let d = TestDir::new("ordering");
        d.dir("z-folder");
        d.file("b.txt", "text");
        d.file("c.jpg", "photo");
        let mut l = Listing::new();
        l.push("z-folder", true);
        l.push("b.txt", false);
        l.push("c.jpg", false);
        let db = Db::from_str("50:image/jpeg:*.jpg\n50:text/plain:*.txt\n");
        ordered(&mut l, d.path(), &db, "name", false, false, false, false).unwrap();
        assert_eq!(l.name(0), "b.txt");
        ordered(&mut l, d.path(), &db, "name", false, true, true, false).unwrap();
        assert_eq!(l.name(0), "z-folder");
        assert_eq!(l.name(1), "c.jpg");
        ordered(&mut l, d.path(), &db, "kind", false, false, false, false).unwrap();
        assert_eq!(l.name(0), "c.jpg");
        assert!(ordered(&mut l, d.path(), &db, "bad", false, false, false, false).is_err());
    }

    #[test]
    fn an_anchor_answers_its_index_in_every_order() {
        let d = TestDir::new("ordering-anchor");
        d.file("b.txt", "xx");
        d.file("a.txt", "xxxx");
        d.file("c.jpg", "x");
        d.dir("z-folder");
        // Distinct mtimes with no clock dependence: touch -d is coreutils, always on a Linux box.
        for (name, date) in [("b.txt", "2020-01-01"), ("a.txt", "2020-01-02"), ("c.jpg", "2020-01-03"), ("z-folder", "2020-01-04")] {
            let status = std::process::Command::new("touch").args(["-d".to_string(), date.to_string(), d.join(name).to_string_lossy().into_owned()]).status().expect("touch sets the fixture mtime");
            assert!(status.success(), "touch -d {date} on {name}");
        }
        let db = Db::from_str("50:image/jpeg:*.jpg\n50:text/plain:*.txt\n");
        let pushed = || {
            let mut l = Listing::new();
            for (name, dir) in [("b.txt", false), ("a.txt", false), ("c.jpg", false), ("z-folder", true)] {
                l.push(name, dir);
            }
            l
        };
        // The sort arm's own composition: order first, then locate the anchor in the new order.
        let index_of = |l: &Listing, anchor: &str| l.index_of(d.path(), &d.path().join(anchor)).map(|index| index as isize).unwrap_or(-1);
        // (by, desc, folders, groups, anchor, want, and the order want is read off)
        for (by, desc, folders, groups, anchor, want, order) in [
            ("name", false, true, false, "a.txt", 1, "[z-folder, a.txt, b.txt, c.jpg]"),
            ("name", true, true, false, "a.txt", 3, "[z-folder, c.jpg, b.txt, a.txt]"),
            ("name", false, false, false, "a.txt", 0, "[a.txt, b.txt, c.jpg, z-folder]"),
            ("name", true, false, false, "z-folder", 0, "[z-folder, c.jpg, b.txt, a.txt]"),
            ("name", false, false, true, "a.txt", 2, "[z-folder, c.jpg, a.txt, b.txt]"),
            ("kind", false, true, false, "c.jpg", 1, "[z-folder, c.jpg, a.txt, b.txt]"),
            ("size", false, true, false, "b.txt", 2, "[z-folder, c.jpg, b.txt, a.txt]"),
            ("size", true, true, false, "a.txt", 1, "[z-folder, a.txt, b.txt, c.jpg]"),
            ("mtime", false, true, false, "a.txt", 2, "[z-folder, b.txt, a.txt, c.jpg]"),
            ("mtime", false, false, false, "a.txt", 1, "[b.txt, a.txt, c.jpg, z-folder]"),
        ] {
            let mut l = pushed();
            ordered(&mut l, d.path(), &db, by, desc, folders, groups, false).unwrap();
            assert_eq!(index_of(&l, anchor), want, "{by} desc={desc} folders={folders} groups={groups} should order {order}");
        }
        // Files alone by walked bytes, no folder to place: [c.jpg(1), b.txt(2), a.txt(4)].
        let mut l = Listing::new();
        for (name, dir) in [("b.txt", false), ("a.txt", false), ("c.jpg", false)] {
            l.push(name, dir);
        }
        ordered(&mut l, d.path(), &db, "size", false, false, false, false).unwrap();
        assert_eq!((l.name(0), l.name(1), l.name(2)), ("c.jpg", "b.txt", "a.txt"));
        assert_eq!(index_of(&l, "b.txt"), 1);
        // Gone or foreign anchors answer -1, never a row.
        let mut l = pushed();
        ordered(&mut l, d.path(), &db, "name", false, true, false, false).unwrap();
        assert_eq!(index_of(&l, "gone.txt"), -1);
        assert!(l.index_of(d.path(), std::path::Path::new("/elsewhere/a.txt")).is_none());
        assert!(l.index_of(d.path(), std::path::Path::new("relative/a.txt")).is_none());
        // A refused key stays refused with an anchor on it, and the order stands.
        let before = l.name(0).to_string();
        let line = format!(r#"{{"c":"sort","by":"mode","anchor":"{}"}}"#, d.join("a.txt").display());
        assert_eq!(request(&mut l, d.path(), &db, &line).unwrap_err(),
            "no such sort key; send name, size, mtime or kind");
        assert_eq!(l.name(0), before);
    }

    #[test]
    fn size_without_folders_first_interleaves_by_walked_size() {
        let d = TestDir::new("ordering-sizeflat");
        // The walk counts mid's own entry, one 4 KiB block on ext4 and a few bytes on btrfs or tmpfs, so big.bin outweighs either.
        d.file("big.bin", &"x".repeat(64 * 1024));
        d.file("small.bin", "12345");
        d.dir("mid");
        d.file("mid/inside", &"x".repeat(100));
        let mut l = Listing::new();
        l.push("big.bin", false);
        l.push("mid", true);
        l.push("small.bin", false);
        let db = Db::from_str("50:application/octet-stream:*.bin\n");
        let (_, _, seed) = ordered(&mut l, d.path(), &db, "size", false, false, false, false).unwrap();
        // A build still keying folders at 0 answers mid first; walked it sits between the files.
        assert_eq!((l.name(0), l.name(1), l.name(2)), ("small.bin", "mid", "big.bin"));
        assert_eq!(seed.len(), 3, "the seed arrives in final row order");
        assert!(seed[0].is_none() && seed[2].is_none(), "file rows seed nothing");
        let mid = seed[1].expect("the folder row carries its walked size");
        assert!(!mid.partial && mid.bytes > 100, "the seed is the whole walk, not a floor");
    }

    // Hidden files last (issue 70): a dotfile follows every visible entry for each key in both directions.
    #[test]
    fn hidden_last_places_dotfiles_after_visible_for_every_key_in_both_directions() {
        let d = TestDir::new("ordering-hiddenlast");
        d.dir("Work");
        d.dir(".cache");
        d.file("notes.md", &"x".repeat(64));
        d.file(".bashrc", "x");
        d.file("photo.jpg", &"x".repeat(8));
        d.file(".shot.jpg", &"x".repeat(1024));
        // Distinct mtimes keep the mtime order from tying into name order.
        for (name, date) in [("notes.md", "2020-01-01"), ("Work", "2020-01-02"),
                             (".bashrc", "2020-01-03"), (".cache", "2020-01-04"),
                             ("photo.jpg", "2020-01-05"), (".shot.jpg", "2020-01-06")] {
            let status = std::process::Command::new("touch").args(["-d".to_string(), date.to_string(), d.join(name).to_string_lossy().into_owned()]).status().expect("touch sets the fixture mtime");
            assert!(status.success(), "touch -d {date} on {name}");
        }
        let db = Db::from_str("50:image/jpeg:*.jpg\n50:text/plain:*.md\n");
        let pushed = || {
            let mut l = Listing::new();
            for (name, dir) in [("notes.md", false), (".bashrc", false), ("Work", true),
                                (".cache", true), ("photo.jpg", false), (".shot.jpg", false)] {
                l.push(name, dir);
            }
            l
        };
        let names = |l: &Listing| (0..l.len()).map(|i| l.name(i).to_string()).collect::<Vec<_>>();
        let partitioned = |got: &[String]| {
            let first_hidden = got.iter().position(|n| n.starts_with('.')).expect("the fixture holds dotfiles");
            got[..first_hidden].iter().all(|n| !n.starts_with('.'))
                && got[first_hidden..].iter().all(|n| n.starts_with('.'))
        };
        for by in ["name", "size", "mtime", "kind"] {
            for desc in [false, true] {
                for folders in [false, true] {
                    // groups=true walks the group_rank arm, so the partition is pinned there too.
                    for groups in [false, true] {
                        let mut l = pushed();
                        ordered(&mut l, d.path(), &db, by, desc, folders, groups, true).unwrap();
                        let got = names(&l);
                        assert!(partitioned(&got),
                            "{by} desc={desc} folders={folders} groups={groups} keeps every dotfile last: {got:?}");
                        let mut off = pushed();
                        ordered(&mut off, d.path(), &db, by, desc, folders, groups, false).unwrap();
                        let old = names(&off);
                        // Only descending name without folders or groups already reads hidden-last, so only that arm asserts equality.
                        if by == "name" && desc && !folders && !groups {
                            assert_eq!(old, got,
                                "{by} desc={desc} folders={folders} groups={groups} is symmetric, so off already reads hidden-last: {old:?}");
                        } else {
                            let first_hidden = old.iter().position(|n| n.starts_with('.')).expect("dotfiles");
                            assert!(old[first_hidden..].iter().any(|n| !n.starts_with('.')),
                                "{by} desc={desc} folders={folders} groups={groups} off leaves a visible entry behind a dotfile: {old:?}");
                        }
                    }
                }
            }
        }
        // Folders first orders each block: visible folders, visible files, hidden folders, hidden files.
        let mut l = pushed();
        ordered(&mut l, d.path(), &db, "name", false, true, false, true).unwrap();
        assert_eq!(names(&l), ["Work", "notes.md", "photo.jpg", ".cache", ".bashrc", ".shot.jpg"]);
        // Both directions keep the partition; descending reverses only inside each block.
        let mut down = pushed();
        ordered(&mut down, d.path(), &db, "name", true, true, false, true).unwrap();
        assert_eq!(names(&down), ["Work", "photo.jpg", "notes.md", ".cache", ".shot.jpg", ".bashrc"]);
        // Without folders first the blocks still hold, ordered by name alone inside each.
        let mut flat = pushed();
        ordered(&mut flat, d.path(), &db, "name", false, false, false, true).unwrap();
        assert_eq!(names(&flat), ["notes.md", "photo.jpg", "Work", ".bashrc", ".cache", ".shot.jpg"]);
        // Off keeps today's order, where a leading dot sorts first.
        let mut today = pushed();
        ordered(&mut today, d.path(), &db, "name", false, true, false, false).unwrap();
        assert_eq!(names(&today), [".cache", "Work", ".bashrc", ".shot.jpg", "notes.md", "photo.jpg"]);
        // Grouped keeps the partition first, then the group rank, then the name inside each.
        let mut grouped = pushed();
        ordered(&mut grouped, d.path(), &db, "name", false, false, true, true).unwrap();
        assert_eq!(names(&grouped), ["Work", "photo.jpg", "notes.md", ".cache", ".shot.jpg", ".bashrc"]);
    }

    #[test]
    fn hidden_last_arrives_on_the_request_line_and_absent_means_today() {
        let d = TestDir::new("ordering-hiddenline");
        d.file("b.txt", "text");
        d.file(".a.txt", "text");
        let db = Db::from_str("50:text/plain:*.txt\n");
        let mut l = Listing::new();
        l.push("b.txt", false);
        l.push(".a.txt", false);
        request(&mut l, d.path(), &db, r#"{"c":"list","hiddenLast":true}"#).unwrap();
        assert_eq!((l.name(0), l.name(1)), ("b.txt", ".a.txt"));
        let mut off = Listing::new();
        off.push("b.txt", false);
        off.push(".a.txt", false);
        request(&mut off, d.path(), &db, r#"{"c":"list"}"#).unwrap();
        assert_eq!((off.name(0), off.name(1)), (".a.txt", "b.txt"),
            "an older client that never names the flag keeps today's order");
        let mut sort = Listing::new();
        sort.push("b.txt", false);
        sort.push(".a.txt", false);
        request(&mut sort, d.path(), &db, r#"{"c":"sort","by":"name","hiddenLast":true}"#).unwrap();
        assert_eq!((sort.name(0), sort.name(1)), ("b.txt", ".a.txt"));
    }
}
