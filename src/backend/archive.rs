// Which archive formats this box can actually write and read, probed once at startup, and the argv
// each tool needs. The jobs themselves are archiveops.rs and reading an index is archivelist.rs.
use crate::backend::archivespec::{seven_spec, tar_spec, ListSpec};
use std::path::Path;

const BSDTAR: &str = "bsdtar";
const SEVENZIP: &str = "7z";

// The formats bsdtar writes on this box, in the order the submenu offers them.
const TAR_FORMATS: &[&str] = &["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst"];
const SEVENZIP_FORMAT: &str = "7z";

pub struct Formats {
    names: Vec<String>,
    have_bsdtar: bool,
    have_7z: bool,
}

fn on_path(prog: &str) -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':')
        .filter(|d| !d.is_empty())
        .any(|d| Path::new(d).join(prog).is_file())
}

impl Formats {
    pub fn probe() -> Formats {
        Formats::from_tools(on_path(BSDTAR), on_path(SEVENZIP))
    }

    pub fn from_tools(have_bsdtar: bool, have_7z: bool) -> Formats {
        let mut names = Vec::new();
        if have_bsdtar {
            names.extend(TAR_FORMATS.iter().map(|s| s.to_string()));
        }
        if have_7z {
            names.push(SEVENZIP_FORMAT.to_string());
        }
        Formats { names, have_bsdtar, have_7z }
    }

    // Exactly the table, which is what the compress submenu draws; an empty one self-hides the entry.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn offers(&self, format: &str) -> bool {
        self.names.iter().any(|n| n == format)
    }

    // bsdtar's -a picks the compression from the destination's own extension, so one argv covers
    // every tar flavour and zip; 7z is its own tool and its own shape.
    // corner: bsdtar reads -C positionally, so it has to precede the names it applies to. Putting it
    // after them archives nothing, prints "Cannot stat" per name, and still leaves an empty archive.
    pub fn compress_argv(&self, format: &str, dest: &Path, parent: &Path, names: &[String]) -> Option<Vec<String>> {
        if !self.offers(format) {
            return None;
        }
        let mut a: Vec<String> = Vec::with_capacity(names.len() + 7);
        if format == SEVENZIP_FORMAT {
            a.push(SEVENZIP.to_string());
            a.push("a".to_string());
            a.push("-bd".to_string());
            a.push(dest.to_string_lossy().to_string());
            // 7z stores the basename of an absolute path, so it needs no working-directory flag.
            a.extend(names.iter().map(|n| parent.join(n).to_string_lossy().to_string()));
            return Some(a);
        }
        a.push(BSDTAR.to_string());
        a.push("-a".to_string());
        a.push("-c".to_string());
        a.push("-f".to_string());
        a.push(dest.to_string_lossy().to_string());
        a.push("-C".to_string());
        a.push(parent.to_string_lossy().to_string());
        // Members are relative names from the selection; one that begins with a dash would be read
        // as a bsdtar option (`--use-compress-program` runs an arbitrary program), so option parsing
        // is terminated first, the same way trash.rs does before its own paths.
        a.push("--".to_string());
        a.extend(names.iter().cloned());
        Some(a)
    }

    // Extraction is chosen by what the archive is, not by what the caller says it is.
    pub fn extract_argv(&self, archive: &Path, dest: &Path) -> Option<Vec<String>> {
        let name = archive.to_string_lossy().to_lowercase();
        if name.ends_with(".7z") {
            if !self.have_7z {
                return None;
            }
            return Some(vec![
                SEVENZIP.to_string(),
                "x".to_string(),
                "-bd".to_string(),
                "-y".to_string(),
                archive.to_string_lossy().to_string(),
                format!("-o{}", dest.to_string_lossy()),
            ]);
        }
        if !self.have_bsdtar {
            return None;
        }
        // bsdtar's own secure-extraction default refuses .. components and absolute paths; the live
        // battery proves that on this build rather than trusting it.
        Some(vec![
            BSDTAR.to_string(),
            "-x".to_string(),
            "-f".to_string(),
            archive.to_string_lossy().to_string(),
            "-C".to_string(),
            dest.to_string_lossy().to_string(),
        ])
    }
}

impl Formats {
    // The argv that lists an archive without extracting a byte of it.
    pub fn list_argv(&self, archive: &Path) -> Option<(Vec<String>, ListSpec)> {
        let name = archive.to_string_lossy().to_lowercase();
        if name.ends_with(".7z") {
            if !self.have_7z {
                return None;
            }
            return Some((
                vec![SEVENZIP.to_string(), "l".to_string(), "-ba".to_string(),
                     archive.to_string_lossy().to_string()],
                seven_spec(),
            ));
        }
        if !self.have_bsdtar {
            return None;
        }
        Some((
            vec![BSDTAR.to_string(), "-t".to_string(), "-v".to_string(), "-f".to_string(),
                 archive.to_string_lossy().to_string()],
            tar_spec(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_what_is_installed_and_never_a_fixed_list() {
        let both = Formats::from_tools(true, true);
        assert_eq!(both.names(), &["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "7z"]);
        let no_seven = Formats::from_tools(true, false);
        assert!(!no_seven.offers("7z"), "a box without 7zip must never offer .7z");
        assert!(no_seven.offers("tar.zst"));
        let nothing = Formats::from_tools(false, false);
        assert!(nothing.names().is_empty(), "with no tool at all the whole entry self-hides");
    }

    #[test]
    fn a_format_the_table_does_not_offer_builds_no_argv_at_all() {
        let f = Formats::from_tools(true, false);
        assert!(f.compress_argv("7z", Path::new("/x/out.7z"), Path::new("/src"), &["a".to_string()]).is_none());
        assert!(f.compress_argv("rar", Path::new("/x/out.rar"), Path::new("/src"), &["a".to_string()]).is_none());
    }

    #[test]
    fn bsdtar_lets_the_destination_extension_choose_the_compression() {
        let f = Formats::from_tools(true, true);
        let a = f.compress_argv("tar.zst", Path::new("/x/out.tar.zst"), Path::new("/src"),
                                &["a.txt".to_string(), "b".to_string()]).unwrap();
        assert_eq!(a[0], "bsdtar");
        assert!(a.contains(&"-a".to_string()), "-a is what reads the extension");
        assert_eq!(a.last().unwrap(), "b");
        // The names are relative, so the archive holds "a.txt" and not "/src/a.txt".
        assert!(!a.iter().any(|s| s.starts_with("/src/")));
        // -C is positional: after the names it applies to nothing and the archive comes out empty.
        let dash_c = a.iter().position(|s| s == "-C").expect("a working directory");
        let first_name = a.iter().position(|s| s == "a.txt").expect("the first name");
        assert!(dash_c < first_name, "-C must precede the names it applies to");
    }

    #[test]
    fn a_member_name_that_looks_like_an_option_sits_after_the_terminator() {
        let f = Formats::from_tools(true, true);
        let a = f
            .compress_argv("tar", Path::new("/x/out.tar"), Path::new("/src"),
                           &["--use-compress-program=sh".to_string(), "ok.txt".to_string()])
            .unwrap();
        let term = a.iter().position(|s| s == "--").expect("a -- terminator before the names");
        let bad = a.iter().position(|s| s == "--use-compress-program=sh").unwrap();
        assert!(term < bad, "a dash-leading member must sit after the -- terminator");
        // 7z stores absolute paths, so its members can never be read as options and it needs no --.
        let seven = f
            .compress_argv("7z", Path::new("/x/out.7z"), Path::new("/src"), &["--evil".to_string()])
            .unwrap();
        assert!(!seven.iter().any(|s| s == "--"));
        assert_eq!(seven.last().unwrap(), "/src/--evil");
    }

    #[test]
    fn seven_zip_is_its_own_tool_and_its_own_shape() {
        let f = Formats::from_tools(true, true);
        let a = f.compress_argv("7z", Path::new("/x/out.7z"), Path::new("/src"), &["a.txt".to_string()]).unwrap();
        assert_eq!(a[0], "7z");
        assert_eq!(a[1], "a");
        // 7z takes absolute sources and stores their basenames, so it carries no -C at all.
        assert_eq!(a.last().unwrap(), "/src/a.txt");
        assert!(!a.iter().any(|s| s == "-C"));
    }

    #[test]
    fn the_listing_argv_names_the_right_tool_and_column_for_each_archive() {
        let f = Formats::from_tools(true, true);
        let (seven, spec) = f.list_argv(Path::new("/x/a.7z")).unwrap();
        assert_eq!(seven[0], "7z");
        assert_eq!(spec.size_column, seven_spec().size_column);
        assert!(spec.name_after_double_space, "7z leaves its packed column blank, so fields cannot locate the name");
        let (tar, spec) = f.list_argv(Path::new("/x/a.tar.zst")).unwrap();
        assert_eq!(tar[0], "bsdtar");
        assert_eq!(spec.size_column, tar_spec().size_column);
        assert_eq!(spec.name_after_fields, tar_spec().name_after_fields);
        assert!(!spec.name_after_double_space, "bsdtar's fields are fixed, so the name is found by counting them");
        assert!(Formats::from_tools(false, false).list_argv(Path::new("/x/a.zip")).is_none());
    }

    #[test]
    fn extraction_is_chosen_by_what_the_archive_is() {
        let f = Formats::from_tools(true, true);
        let seven = f.extract_argv(Path::new("/x/a.7z"), Path::new("/out")).unwrap();
        assert_eq!(seven[0], "7z");
        assert!(seven.iter().any(|s| s == "-o/out"));
        let tar = f.extract_argv(Path::new("/x/a.tar.zst"), Path::new("/out")).unwrap();
        assert_eq!(tar[0], "bsdtar");
        assert!(tar.contains(&"-C".to_string()));
        // A .7z on a box with no 7zip cannot be extracted, and says so by building nothing.
        let no_seven = Formats::from_tools(true, false);
        assert!(no_seven.extract_argv(Path::new("/x/a.7z"), Path::new("/out")).is_none());
    }

}
