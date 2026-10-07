// Which archive formats this box can actually write and read, probed once at startup, and the argv
// each tool needs. The jobs themselves are archiveops.rs and reading an index is archivelist.rs.
use crate::backend::archivespec::{seven_spec, tar_spec, ListSpec};
use crate::backend::archiveuser::{self, UserFormat};
use std::path::Path;

const BSDTAR: &str = "bsdtar";
const SEVENZIP: &str = "7z";

// The formats bsdtar writes on this box, in the order the submenu offers them.
const TAR_FORMATS: &[&str] = &["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst"];
const SEVENZIP_FORMAT: &str = "7z";
// Ids a user format can never take: everything built in, and rar, which Flea reads and never writes.
const RESERVED: &[&str] = &["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "7z", "rar", "tgz"];

// Either tool reads a .zip and a .rar, which is what a 7z-only box can still extract.
const ZIP_SUFFIXES: &[&str] = &[".zip", ".rar"];
// Tar stays on bsdtar: `7z x` on a .tar.gz writes the inner .tar, not the files.
const TAR_SUFFIXES: &[&str] = &[".tar.zst", ".tar.bz2", ".tar.gz", ".tar.xz", ".tgz", ".tar"];

// Which program reads one archive, decided from the name's own extension and what this box has.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Reader {
    Bsdtar,
    SevenZip,
}

pub struct Formats {
    names: Vec<String>,
    have_bsdtar: bool,
    have_7z: bool,
    // $XDG_CONFIG_HOME/flea/compressors, offered after the built-in formats; see archiveuser.rs.
    user: Vec<UserFormat>,
    // Test seam: with the probe set, compress() itself runs the RLIMIT_CPU check (see K1).
    #[cfg(test)]
    probe: bool,
    // Test seam: with the block set, compress() itself runs a tool that stays alive to be cancelled.
    #[cfg(test)]
    block: bool,
}

fn on_path(prog: &str) -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':')
        .filter(|d| !d.is_empty())
        .any(|d| Path::new(d).join(prog).is_file())
}

impl Formats {
    pub fn probe() -> Formats {
        Formats::from_tools(on_path(BSDTAR), on_path(SEVENZIP)).with_user(archiveuser::load(RESERVED))
    }

    // Nothing writes rar here: WinRAR's own tool is the only thing on Linux that does, it is not free
    // software and it is in no Arch repository, so the compress submenu cannot offer it. Reading one
    // is separate and needs neither that tool nor a flag, see extract_argv below.
    pub fn from_tools(have_bsdtar: bool, have_7z: bool) -> Formats {
        let mut names = Vec::new();
        if have_bsdtar {
            names.extend(TAR_FORMATS.iter().map(|s| s.to_string()));
        }
        if have_7z {
            names.push(SEVENZIP_FORMAT.to_string());
        }
        Formats { names, have_bsdtar, have_7z, user: Vec::new(),
            #[cfg(test)]
            probe: false,
            #[cfg(test)]
            block: false,
        }
    }

    // The user's formats join the table after the built-in ones, so they reach the submenu like any other.
    pub fn with_user(mut self, mut user: Vec<UserFormat>) -> Formats {
        user.retain(|u| !RESERVED.contains(&u.id.as_str()));
        self.names.extend(user.iter().map(|u| u.id.clone()));
        self.user = user;
        self
    }

    // The program a user format runs, which the jail has to bind; None for every built-in format.
    pub fn user_program(&self, format: &str) -> Option<&Path> {
        self.user.iter().find(|u| u.id == format).map(|u| u.program.as_path())
    }

    // A Formats whose compressor is the RLIMIT_CPU probe, so the uncapped-jail pin drives compress() itself.
    #[cfg(test)]
    pub fn test_probe() -> Formats {
        Formats { names: vec!["zip".to_string()], have_bsdtar: true, have_7z: false, user: Vec::new(), probe: true, block: false }
    }

    // A Formats whose compressor blocks until killed, so a mid-run cancel always has a live child.
    #[cfg(test)]
    pub fn test_block() -> Formats {
        Formats { names: vec!["zip".to_string()], have_bsdtar: true, have_7z: false, user: Vec::new(), probe: false, block: true }
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
        // Test seam: with the block set, the compressor stays alive until the cancel stops it.
        #[cfg(test)]
        if self.block {
            return Some(vec!["/usr/bin/sleep".to_string(), "30".to_string()]);
        }
        // Test seam: with the probe set, the compressor is the RLIMIT_CPU probe.
        #[cfg(test)]
        if self.probe {
            return Some(vec!["/usr/bin/python3".to_string(), "-c".to_string(),
                "import resource,sys; open(sys.argv[1],'wb').write(b'probe'); sys.exit(0 if resource.getrlimit(resource.RLIMIT_CPU)[0]==resource.RLIM_INFINITY else 1)".to_string(),
                dest.to_string_lossy().to_string()]);
        }
        if let Some(user) = self.user.iter().find(|u| u.id == format) {
            return Some(user.argv(dest, parent, names));
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

    // The one reader choice, shared by extract_argv and list_argv; None when no tool reads it.
    pub fn reader_for(&self, archive: &Path) -> Option<Reader> {
        let name = archive.to_string_lossy().to_lowercase();
        if name.ends_with(".7z") {
            return self.have_7z.then_some(Reader::SevenZip);
        }
        if ZIP_SUFFIXES.iter().any(|suffix| name.ends_with(*suffix)) {
            if self.have_bsdtar {
                // A rar prefers 7z's reference reader; a zip stays with libarchive where it was.
                return Some(if name.ends_with(".rar") && self.have_7z { Reader::SevenZip } else { Reader::Bsdtar });
            }
            return self.have_7z.then_some(Reader::SevenZip);
        }
        if TAR_SUFFIXES.iter().any(|suffix| name.ends_with(*suffix)) {
            return self.have_bsdtar.then_some(Reader::Bsdtar);
        }
        // An unrecognised name keeps the libarchive attempt; the client only asks about the classes above.
        self.have_bsdtar.then_some(Reader::Bsdtar)
    }

    // The zip-class bit the client's Extract row reads; see formats_line in archivereq.rs.
    pub fn zip_readable(&self) -> bool {
        self.have_bsdtar || self.have_7z
    }

    // Extraction is chosen by what the archive is, not by what the caller says it is.
    pub fn extract_argv(&self, archive: &Path, dest: &Path) -> Option<Vec<String>> {
        match self.reader_for(archive)? {
            Reader::SevenZip => Some(vec![
                SEVENZIP.to_string(),
                "x".to_string(),
                "-bd".to_string(),
                "-y".to_string(),
                archive.to_string_lossy().to_string(),
                format!("-o{}", dest.to_string_lossy()),
            ]),
            // bsdtar's secure-extraction default is exercised by the native archive tests.
            Reader::Bsdtar => Some(vec![
                BSDTAR.to_string(),
                "-x".to_string(),
                "-f".to_string(),
                archive.to_string_lossy().to_string(),
                "-C".to_string(),
                dest.to_string_lossy().to_string(),
            ]),
        }
    }

    // The argv that lists an archive without extracting a byte of it.
    pub fn list_argv(&self, archive: &Path) -> Option<(Vec<String>, ListSpec)> {
        match self.reader_for(archive)? {
            Reader::SevenZip => Some((
                vec![SEVENZIP.to_string(), "l".to_string(), "-ba".to_string(),
                     archive.to_string_lossy().to_string()],
                seven_spec(),
            )),
            Reader::Bsdtar => Some((
                vec![BSDTAR.to_string(), "-t".to_string(), "-v".to_string(), "-f".to_string(),
                     archive.to_string_lossy().to_string()],
                tar_spec(),
            )),
        }
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

    // Flea reads rar and writes none. 7z's own rar and rar5 readers are the reference ones, so they
    // are preferred where 7z is installed; libarchive reads both too and is the fallback.
    #[test]
    fn a_rar_is_read_by_7z_where_it_is_installed_and_by_libarchive_where_it_is_not() {
        let both = Formats::from_tools(true, true);
        let seven = both.extract_argv(Path::new("/x/holiday.rar"), Path::new("/dest")).unwrap();
        assert_eq!(seven[0], "7z");
        let (listed, _) = both.list_argv(Path::new("/x/holiday.rar")).unwrap();
        assert_eq!(listed[0], "7z", "the index and the extract must agree on the tool");
        let tar_only = Formats::from_tools(true, false);
        let fallback = tar_only.extract_argv(Path::new("/x/holiday.rar"), Path::new("/dest")).unwrap();
        assert_eq!(fallback[0], "bsdtar");
        let (fallback_list, _) = tar_only.list_argv(Path::new("/x/holiday.rar")).unwrap();
        assert_eq!(fallback_list[0], "bsdtar");
        // #165: a box with only 7z reads the zip class too; a tar stays unreadable there.
        let seven_only = Formats::from_tools(false, true);
        let zip_7z = seven_only.extract_argv(Path::new("/x/holiday.zip"), Path::new("/dest")).unwrap();
        assert_eq!(zip_7z[0], "7z");
        let (zip_list, spec) = seven_only.list_argv(Path::new("/x/holiday.zip")).unwrap();
        assert_eq!(zip_list[0], "7z", "the index agrees with the extract about the tool");
        assert!(spec.name_after_double_space);
        assert_eq!(seven_only.extract_argv(Path::new("/x/holiday.rar"), Path::new("/dest")).unwrap()[0], "7z");
        assert!(seven_only.extract_argv(Path::new("/x/holiday.tar.gz"), Path::new("/dest")).is_none());
        assert!(seven_only.extract_argv(Path::new("/x/holiday.7z"), Path::new("/dest")).is_some());
        // Upper case reaches the same reader: the name is lowercased before it is matched.
        assert_eq!(both.extract_argv(Path::new("/x/HOLIDAY.RAR"), Path::new("/d")).unwrap()[0], "7z");
        // No tool at all reads nothing, rather than building an argv for a program that is absent.
        assert!(Formats::from_tools(false, false).extract_argv(Path::new("/x/a.rar"), Path::new("/d")).is_none());
        // And rar is never offered as something to write, on any box.
        assert!(!both.offers("rar"));
        assert!(!both.names().iter().any(|n| n == "rar"));
    }

    // A user format follows the built-in ones in the submenu, builds its own argv and names its program
    // for the jail; one claiming a built-in id never reaches the table, whatever the parser let through.
    #[test]
    fn a_user_format_joins_the_table_after_the_built_in_ones() {
        let line = "zjx /opt/zjx {out} --base {dir} -- {names}\nzip /opt/fake {out} {names}";
        let (user, _) = archiveuser::parse(line, &[], |p| Some(p.to_path_buf()));
        let f = Formats::from_tools(true, false).with_user(user);
        assert_eq!(f.names(), &["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "zjx"]);
        let a = f.compress_argv("zjx", Path::new("/x/s.zjx"), Path::new("/src"), &["a".to_string()]).unwrap();
        assert_eq!(a, ["/opt/zjx", "/x/s.zjx", "--base", "/src", "--", "a"]);
        assert_eq!(f.user_program("zjx"), Some(Path::new("/opt/zjx")));
        assert_eq!(f.compress_argv("zip", Path::new("/x/s.zip"), Path::new("/src"), &["a".to_string()]).unwrap()[0], "bsdtar");
        assert_eq!(f.user_program("zip"), None, "the built-in zip keeps the plain archive jail");
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

    // #165: each wire class bit and the argv it gates are asserted together, tool by tool.
    #[test]
    fn the_extract_capability_bits_agree_with_the_argv_they_gate() {
        let readable = |f: &Formats, name: &str| f.extract_argv(Path::new(name), Path::new("/out")).is_some();
        for (have_bsdtar, have_7z) in [(true, true), (true, false), (false, true), (false, false)] {
            let f = Formats::from_tools(have_bsdtar, have_7z);
            assert_eq!(f.zip_readable(), readable(&f, "/x/a.zip"), "zip bit vs argv: {} {}", have_bsdtar, have_7z);
            assert_eq!(f.zip_readable(), readable(&f, "/x/a.rar"), "rar follows the zip class: {} {}", have_bsdtar, have_7z);
            assert_eq!(f.offers("tar"), readable(&f, "/x/a.tar.zst"), "tar bit vs argv: {} {}", have_bsdtar, have_7z);
            assert_eq!(f.offers("7z"), readable(&f, "/x/a.7z"), "7z bit vs argv: {} {}", have_bsdtar, have_7z);
        }
    }

}
