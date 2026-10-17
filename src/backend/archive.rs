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
const ZIP_FORMAT: &str = "zip";
// 7z stores a symbolic link as a link instead of following it.
pub(crate) const STORE_LINKS: &str = "-snl";
// 7z writes a .zip rather than its own format.
pub(crate) const ZIP_TYPE: &str = "-tzip";

// Either tool reads a .zip and a .rar, which is what a 7z-only box can still extract.
const ZIP_SUFFIXES: &[&str] = &[".zip", ".rar"];
const ZIP_SUFFIX: &str = ".zip";
// Tar stays on bsdtar: `7z x` on a .tar.gz writes the inner .tar, not the files.
const TAR_SUFFIXES: &[&str] = &[".tar.zst", ".tar.bz2", ".tar.gz", ".tar.xz", ".tgz", ".tar"];
// Packages and disk images read through 7z, or bsdtar where 7z is missing except an AppImage; longest form first.
const SEVEN_SUFFIXES: &[&str] = &[".appimage", ".cpio", ".cab", ".jar", ".apk", ".rpm", ".deb", ".iso"];
// A type-2 AppImage is an ELF stub on a squashfs image, which only 7z reads.
const SQUASHFS_SUFFIX: &str = ".appimage";
// A disk image lists only: extracting one builds no argv, so the client never starts a job for it.
const LIST_ONLY_SUFFIXES: &[&str] = &[".iso"];

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
    // Test seam: stand-in argvs for the 7z probe and the extract, and a count of the index listings asked for.
    #[cfg(test)]
    stub: Option<Stub>,
}

#[cfg(test)]
struct Stub {
    technical: Vec<String>,
    extract: Vec<String>,
    indexed: std::sync::atomic::AtomicUsize,
}

fn on_path(prog: &str) -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':')
        .filter(|d| !d.is_empty())
        .any(|d| Path::new(d).join(prog).is_file())
}

impl Formats {
    /// Detect installed archive tools and load configured compression formats once.
    pub fn probe() -> Formats {
        Formats::from_tools(on_path(BSDTAR), on_path(SEVENZIP)).with_user(archiveuser::load(RESERVED))
    }

    /// Nothing writes rar here: WinRAR's own tool is the only thing on Linux that does, it is not free
    /// software and it is in no Arch repository, so the compress submenu cannot offer it. Reading one
    /// is separate and needs neither that tool nor a flag, see extract_argv below.
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
            #[cfg(test)]
            stub: None,
        }
    }

    /// The user's formats join the table after the built-in ones, so they reach the submenu like any other.
    pub fn with_user(mut self, mut user: Vec<UserFormat>) -> Formats {
        user.retain(|u| !RESERVED.contains(&u.id.as_str()));
        self.names.extend(user.iter().map(|u| u.id.clone()));
        self.user = user;
        self
    }

    /// The program a user format runs, which the jail has to bind; None for every built-in format.
    pub fn user_program(&self, format: &str) -> Option<&Path> {
        self.user.iter().find(|u| u.id == format).map(|u| u.program.as_path())
    }

    /// A Formats whose compressor is the RLIMIT_CPU probe, so the uncapped-jail pin drives compress() itself.
    #[cfg(test)]
    pub fn test_probe() -> Formats {
        Formats { names: vec!["zip".to_string()], have_bsdtar: true, have_7z: false, user: Vec::new(), probe: true, block: false, stub: None }
    }

    /// A Formats whose compressor blocks until killed, so a mid-run cancel always has a live child.
    #[cfg(test)]
    pub fn test_block() -> Formats {
        Formats { names: vec!["zip".to_string()], have_bsdtar: true, have_7z: false, user: Vec::new(), probe: false, block: true, stub: None }
    }

    /// A 7z-only Formats whose probe listing and extract are the given argvs, so a cancel can land on a live probe.
    #[cfg(test)]
    pub fn test_stub(technical: Vec<String>, extract: Vec<String>) -> Formats {
        let stub = Stub { technical, extract, indexed: std::sync::atomic::AtomicUsize::new(0) };
        Formats { names: vec![SEVENZIP_FORMAT.to_string()], have_bsdtar: false, have_7z: true, user: Vec::new(), probe: false, block: false, stub: Some(stub) }
    }

    // How many index listings the stubbed Formats was asked for.
    #[cfg(test)]
    pub fn indexed(&self) -> usize {
        self.stub.as_ref().map_or(0, |s| s.indexed.load(std::sync::atomic::Ordering::Relaxed))
    }

    // Exactly the table, which is what the compress submenu draws; an empty one self-hides the entry.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn offers(&self, format: &str) -> bool {
        self.names.iter().any(|n| n == format)
    }

    // A password is a 7z job whatever the format: bsdtar writes no encryption, so the With-password row hides where 7z is missing.
    pub fn has_7z(&self) -> bool {
        self.have_7z
    }

    /// bsdtar's -a picks the compression from the destination's own extension, so one argv covers
    /// every tar flavour, and zip on a box without 7z; 7z is its own tool and its own shape.
    /// corner: bsdtar reads -C positionally, so it has to precede the names it applies to. Putting it
    /// after them archives nothing, prints "Cannot stat" per name, and still leaves an empty archive.
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
        // A zip goes to 7z when it is installed: its deflate runs on every core, where bsdtar's runs on one.
        if format == SEVENZIP_FORMAT || (format == ZIP_FORMAT && self.have_7z) {
            a.push(SEVENZIP.to_string());
            a.push("a".to_string());
            a.push("-bd".to_string());
            // A selected name is taken literally, so a * or ? in one never widens into a wildcard.
            a.push("-spd".to_string());
            // A link is stored as a link, as bsdtar does; followed, a dangling one fails the whole archive.
            a.push(STORE_LINKS.to_string());
            if format == ZIP_FORMAT {
                a.push(ZIP_TYPE.to_string());
            }
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
        if SEVEN_SUFFIXES.iter().any(|suffix| name.ends_with(*suffix)) {
            // libarchive has no squashfs reader, so an AppImage waits for 7z instead of failing in bsdtar.
            let libarchive_reads = self.have_bsdtar && !name.ends_with(SQUASHFS_SUFFIX);
            return if self.have_7z { Some(Reader::SevenZip) } else { libarchive_reads.then_some(Reader::Bsdtar) };
        }
        // An unrecognised name keeps the libarchive attempt; the client only asks about the classes above.
        self.have_bsdtar.then_some(Reader::Bsdtar)
    }

    // The zip-class bit the client's Extract row reads; see formats_line in archivereq.rs.
    pub fn zip_readable(&self) -> bool {
        self.have_bsdtar || self.have_7z
    }

    // A disk image the client lists but never extracts; the extract refusal names this.
    pub fn is_list_only(archive: &Path) -> bool {
        let name = archive.to_string_lossy().to_lowercase();
        LIST_ONLY_SUFFIXES.iter().any(|suffix| name.ends_with(*suffix))
    }

    // Extraction is chosen by what the archive is; a disk image builds no argv, so the client never starts a job for one.
    pub fn extract_argv(&self, archive: &Path, dest: &Path) -> Option<Vec<String>> {
        #[cfg(test)]
        if let Some(stub) = &self.stub {
            return Some(stub.extract.clone());
        }
        let name = archive.to_string_lossy().to_lowercase();
        if LIST_ONLY_SUFFIXES.iter().any(|suffix| name.ends_with(*suffix)) {
            return None;
        }
        match self.reader_for(archive)? {
            Reader::SevenZip => Some(seven_extract_argv(archive, dest)),
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

    // Whether 7z can read this archive, which a zip stays readable by beside bsdtar and only 7z can unlock.
    pub fn seven_reads(&self, archive: &Path) -> bool {
        let zip = archive.to_string_lossy().to_lowercase().ends_with(ZIP_SUFFIX);
        self.have_7z && (zip || self.reader_for(archive) == Some(Reader::SevenZip))
    }

    // 7z's own extract argv, which the test stub answers as it does for the plain one.
    pub(crate) fn seven_extract(&self, archive: &Path, dest: &Path) -> Vec<String> {
        #[cfg(test)]
        if let Some(stub) = &self.stub {
            return stub.extract.clone();
        }
        seven_extract_argv(archive, dest)
    }

    // `7z l -slt`, the technical listing whose keys are not localized; None unless 7z reads the archive.
    pub fn technical_list_argv(&self, archive: &Path) -> Option<Vec<String>> {
        #[cfg(test)]
        if let Some(stub) = &self.stub {
            return Some(stub.technical.clone());
        }
        self.seven_reads(archive)
            .then(|| vec![SEVENZIP.to_string(), "l".to_string(), "-slt".to_string(), archive.to_string_lossy().to_string()])
    }

    // The argv that lists an archive without extracting a byte of it.
    pub fn list_argv(&self, archive: &Path) -> Option<(Vec<String>, ListSpec)> {
        #[cfg(test)]
        if let Some(stub) = &self.stub {
            stub.indexed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
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

// 7z's own extract, which asks its stdin for a password where the archive has one.
pub(crate) fn seven_extract_argv(archive: &Path, dest: &Path) -> Vec<String> {
    vec![
        SEVENZIP.to_string(),
        "x".to_string(),
        "-bd".to_string(),
        "-y".to_string(),
        archive.to_string_lossy().to_string(),
        format!("-o{}", dest.to_string_lossy()),
    ]
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
