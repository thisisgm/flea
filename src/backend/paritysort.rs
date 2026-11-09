//! Orders needed by the optional yazi preset, over the same listing snapshot.
use super::{
    birthtime,
    listing::Listing,
    sort::{cmp_extension, name_order},
};
use std::{cmp::Ordering, hash::BuildHasher, path::Path, time::Instant};

pub fn sort(
    l: &mut Listing,
    base: &Path,
    by: &str,
    desc: bool,
    folders: bool,
    hidden_last: bool,
) -> (f64, f64) {
    let start = Instant::now();
    let born: Vec<_> = if by == "btime" {
        (0..l.len())
            .map(|i| birthtime::get(&l.path(base, i)))
            .collect()
    } else {
        Vec::new()
    };
    let pass = if by == "btime" {
        start.elapsed().as_secs_f64() * 1000.0
    } else {
        0.0
    };
    let start = Instant::now();
    let mut indices: Vec<_> = (0..l.len()).collect();
    let random = std::collections::hash_map::RandomState::new();
    // A rank is fixed before comparison, so this is a total order and window reads never reshuffle.
    let ranks: Vec<_> = if by == "random" {
        (0..l.len()).map(|i| random.hash_one(l.name(i))).collect()
    } else {
        Vec::new()
    };
    indices.sort_by(|&a, &b| {
        if hidden_last {
            let hidden = super::sort::is_hidden(l.name(a)).cmp(&super::sort::is_hidden(l.name(b)));
            if hidden != Ordering::Equal {
                return hidden;
            }
        }
        if folders {
            let group = l.is_dir(b).cmp(&l.is_dir(a));
            if group != Ordering::Equal {
                return group;
            }
        }
        let order = match by {
            "btime" => born[a].cmp(&born[b]),
            "extension" if !l.is_dir(a) && !l.is_dir(b) => cmp_extension(l.name(a), l.name(b)),
            "extension" => Ordering::Equal,
            "random" => ranks[a].cmp(&ranks[b]),
            _ => unreachable!(),
        }
        .then_with(|| name_order(l.name(a).as_bytes(), l.name(b).as_bytes()));
        if desc {
            order.reverse()
        } else {
            order
        }
    });
    l.spans = indices.into_iter().map(|i| l.spans[i]).collect();
    (pass, start.elapsed().as_secs_f64() * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extension_and_random_need_only_names_and_preserve_the_snapshot() {
        let mut l = Listing::new();
        for name in ["c.txt", "b.rs", "a.txt", "README"] {
            l.push(name, false);
        }
        let (pass, _) = sort(
            &mut l,
            Path::new("/missing"),
            "extension",
            false,
            true,
            false,
        );
        assert_eq!(pass, 0.0);
        assert_eq!(
            (l.name(0), l.name(1), l.name(2), l.name(3)),
            ("README", "b.rs", "a.txt", "c.txt")
        );
        assert_eq!(
            sort(&mut l, Path::new("/missing"), "random", false, true, false).0,
            0.0
        );
        let mut names: Vec<_> = (0..l.len()).map(|i| l.name(i)).collect();
        names.sort();
        assert_eq!(names, ["README", "a.txt", "b.rs", "c.txt"]);
    }
    #[test]
    fn unavailable_birth_times_follow_yazi_option_order_and_never_use_mtime() {
        let mut l = Listing::new();
        l.push("z", false);
        l.push("a", false);
        sort(&mut l, Path::new("/missing"), "btime", false, true, false);
        assert_eq!((l.name(0), l.name(1)), ("a", "z"));
        sort(&mut l, Path::new("/missing"), "btime", true, true, false);
        assert_eq!((l.name(0), l.name(1)), ("z", "a"));
    }
}

#[cfg(test)]
mod available_tests {
    use super::*;
    #[test]
    fn birth_time_sort_keeps_unavailable_entries_on_the_explicit_side() {
        let root = crate::backend::testdir::TestDir::new("tui17-birth-order");
        root.file("z-present", "x");
        let mut l = Listing::new();
        l.push("z-present", false);
        l.push("a-absent", false);
        sort(&mut l, root.path(), "btime", false, true, false);
        assert_eq!((l.name(0), l.name(1)), ("a-absent", "z-present"));
        sort(&mut l, root.path(), "btime", true, true, false);
        assert_eq!((l.name(0), l.name(1)), ("z-present", "a-absent"));
    }
}
