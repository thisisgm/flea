use super::listing::Listing;
use super::meta::stat_all;
use super::mime::Db;
use super::sort::{name_order, parse_sort_by, sort_listing};
use std::cmp::Ordering;
use std::path::Path;
use std::time::Instant;

// Sample input: {"c":"list","by":"name","desc":false,"foldersFirst":true,"groupByKind":false}
pub fn request(
    l: &mut Listing,
    base: &Path,
    mime: &Db,
    line: &str,
) -> Result<(f64, f64), &'static str> {
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
    ordered(l, base, mime, by, desc, folders, groups)
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
) -> Result<(f64, f64), &'static str> {
    let by = if by == "date" { "mtime" } else { by };
    if !["name", "size", "mtime", "kind"].contains(&by) {
        return Err("no such sort key; send name, size, mtime or kind");
    }
    if by != "kind" && folders && !groups {
        return Ok(sort_listing(l, base, parse_sort_by(by)?, desc));
    }
    let (stats, pass_ms) = if by == "size" || by == "mtime" {
        let (stats, ms) = stat_all(base, l);
        (Some(stats), ms)
    } else {
        (None, 0.0)
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
                let sa = if l.is_dir(a) { 0 } else { stats[a].size };
                let sb = if l.is_dir(b) { 0 } else { stats[b].size };
                sa.cmp(&sb)
            }
            "mtime" => {
                let stats = stats.as_ref().unwrap();
                stats[a].mtime.cmp(&stats[b].mtime)
            }
            _ => Ordering::Equal,
        };
        let order = key.then_with(|| name_order(l.name(a).as_bytes(), l.name(b).as_bytes()));
        if desc {
            order.reverse()
        } else {
            order
        }
    });
    l.spans = indices.into_iter().map(|i| l.spans[i]).collect();
    Ok((pass_ms, start.elapsed().as_secs_f64() * 1000.0))
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
        ordered(&mut l, d.path(), &db, "name", false, false, false).unwrap();
        assert_eq!(l.name(0), "b.txt");
        ordered(&mut l, d.path(), &db, "name", false, true, true).unwrap();
        assert_eq!(l.name(0), "z-folder");
        assert_eq!(l.name(1), "c.jpg");
        ordered(&mut l, d.path(), &db, "kind", false, false, false).unwrap();
        assert_eq!(l.name(0), "c.jpg");
        assert!(ordered(&mut l, d.path(), &db, "bad", false, false, false).is_err());
    }
}
