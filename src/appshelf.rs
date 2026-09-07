//! Which files AppShelf opens, and how to hand one over.
//!
//! AppShelf is Omarchy's local application manager. It installs AppImages onto
//! a shelf of its own and installs Arch, Debian and RPM packages system-wide
//! through pacman, and it shows the same review window for all four before
//! anything happens. Handing those files to it is better than handing them to
//! `gio open`, which on this desktop offers an archive manager for a `.deb` and
//! nothing at all for an AppImage.
//!
//! Recognition is by content where the format has content to recognise, and by
//! name where it does not — an Arch package is a plain compressed tarball, so
//! only `.pkg.tar*` distinguishes one. AppShelf reads the file again and
//! refuses it properly if this guessed wrong; nothing here has to be certain.

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// Compare the final filename extension without ASCII case sensitivity.
fn extension_is(path: &Path, wanted: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(wanted))
}

/// Read up to `len` bytes from a regular file; return `None` on failure.
fn header(path: &Path, len: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    // Follow links like open does, but never open a FIFO that could wait forever for a writer.
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let mut bytes = vec![0u8; len];
    let mut filled = 0;
    while filled < len {
        match file.read(&mut bytes[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => return None,
        }
    }
    bytes.truncate(filled);
    Some(bytes)
}

/// Recognize an AppImage by extension or Type 2 ELF magic.
pub fn is_appimage(path: &Path) -> bool {
    if extension_is(path, "appimage") {
        return true;
    }
    // A type 2 AppImage is an ELF carrying `AI\x02` at offset 8, which is what
    // makes an extensionless one still recognisable.
    header(path, 12)
        .is_some_and(|h| h.len() == 12 && &h[..4] == b"\x7fELF" && &h[8..11] == b"AI\x02")
}

/// A Debian package: an `ar` archive whose first member is `debian-binary`.
/// The magic alone would also match a static library, which is not one.
pub fn is_deb(path: &Path) -> bool {
    if extension_is(path, "deb") {
        return true;
    }
    header(path, 24).is_some_and(|h| {
        h.len() == 24
            && h.starts_with(b"!<arch>\n")
            && matches!(&h[8..24], b"debian-binary   " | b"debian-binary/  ")
    })
}

/// Recognize an RPM by extension or its four-byte lead magic.
pub fn is_rpm(path: &Path) -> bool {
    extension_is(path, "rpm") || header(path, 4).is_some_and(|h| h.starts_with(b"\xed\xab\xee\xdb"))
}

/// An Arch package. There is no magic to read: the file is an ordinary
/// compressed tarball, and only the `.pkg.tar` in its name says what it holds.
pub fn is_arch_package(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|name| {
            [
                ".pkg.tar.zst",
                ".pkg.tar.xz",
                ".pkg.tar.gz",
                ".pkg.tar.bz2",
                ".pkg.tar.lzo",
                ".pkg.tar.lrz",
                ".pkg.tar.lz4",
                ".pkg.tar.lz",
                ".pkg.tar.z",
                ".pkg.tar",
            ]
            .iter()
            .any(|suffix| name.ends_with(suffix))
        })
}

/// Whether AppShelf is the right handler for this file.
pub fn handles(path: &Path) -> bool {
    is_appimage(path) || is_arch_package(path) || is_deb(path) || is_rpm(path)
}

/// Launch AppShelf in its own process group with detached standard streams.
/// Return zero on launch, or `None` so the caller can try the desktop opener.
pub fn open(target: &Path) -> Option<i32> {
    // corner: spawn and not exec or status, because appshelf outlives us; see AGENTS.md "Opening a file".
    let started = Command::new("appshelf")
        .arg(target)
        // The handler outlives us, so an inherited pipe would kill it on its first write; see AGENTS.md "Opening a file".
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Its own process group, so nothing that later kills Flea's group reaches the opened program.
        .process_group(0)
        .spawn();
    match started {
        Ok(_) => Some(0),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use crate::backend::fifotest::{mkfifo, within};

    #[test]
    fn header_rejects_non_regular_files_without_blocking() {
        let sandbox = TestDir::new("appshelf-file-kinds");
        let fifo = sandbox.join("pipe");
        mkfifo(&fifo);
        std::os::unix::fs::symlink("pipe", sandbox.join("pipe-link")).unwrap();
        for path in [fifo, sandbox.join("pipe-link"), sandbox.dir("directory")] {
            within("AppShelf recognition", move || {
                assert_eq!(header(&path, 12), None);
                assert!(!handles(&path));
            });
        }
        let real = sandbox.file("regular", "header bytes");
        assert_eq!(header(&real, 6), Some(b"header".to_vec()));
        std::os::unix::fs::symlink("regular", sandbox.join("regular-link")).unwrap();
        assert_eq!(header(&sandbox.join("regular-link"), 6), Some(b"header".to_vec()));
    }

    #[test]
    fn appimage_extension_is_recognized() {
        let sandbox = TestDir::new("appimage-ext");
        let p1 = sandbox.file("test.AppImage", "not elf");
        assert!(is_appimage(&p1));

        let p2 = sandbox.file("test.appimage", "not elf");
        assert!(is_appimage(&p2));

        let p3 = sandbox.file("test.txt", "not elf");
        assert!(!is_appimage(&p3));
        assert!(!handles(&p3));
    }

    #[test]
    fn appimage_magic_is_recognized_without_extension() {
        let sandbox = TestDir::new("appimage-magic");
        let p1 = sandbox.join("extensionless-app");
        let mut bytes = vec![0u8; 16];
        bytes[..4].copy_from_slice(b"\x7fELF");
        bytes[8..11].copy_from_slice(b"AI\x02");
        std::fs::write(&p1, bytes).unwrap();
        assert!(is_appimage(&p1));

        let p2 = sandbox.join("regular-elf");
        let mut bytes2 = vec![0u8; 16];
        bytes2[..4].copy_from_slice(b"\x7fELF");
        std::fs::write(&p2, bytes2).unwrap();
        assert!(!is_appimage(&p2));
        assert!(!handles(&p2));
    }

    #[test]
    fn arch_packages_are_recognized_by_name() {
        let sandbox = TestDir::new("appshelf-arch");
        for name in [
            "a-1-1-x86_64.pkg.tar.zst",
            "a-1-1-x86_64.pkg.tar.xz",
            "a-1-1-x86_64.pkg.tar.gz",
            "a-1-1-x86_64.pkg.tar.bz2",
            "a-1-1-x86_64.pkg.tar.lzo",
            "a-1-1-x86_64.pkg.tar.lrz",
            "a-1-1-x86_64.pkg.tar.lz4",
            "a-1-1-x86_64.pkg.tar.lz",
            "a-1-1-x86_64.pkg.tar.Z",
            "a.PKG.TAR.ZST",
            "a-1-1-any.pkg.tar",
        ] {
            let path = sandbox.file(name, "not really a package");
            assert!(is_arch_package(&path), "{name}");
            assert!(handles(&path), "{name}");
        }
        // The compression alone is not the claim: an ordinary tarball is not a
        // package and must keep going to the desktop's archive handler.
        for name in [
            "photos.tar.zst",
            "photos.tar.lz4",
            "a.pkg.tar.zst.sig",
            "a.pkg.tarball",
        ] {
            let plain = sandbox.file(name, "not a package");
            assert!(!is_arch_package(&plain), "{name}");
            assert!(!handles(&plain), "{name}");
        }
    }

    #[test]
    fn deb_is_recognized_by_extension_and_by_magic() {
        let sandbox = TestDir::new("appshelf-deb");
        let named = sandbox.file("hello_2.10-3_amd64.deb", "not really a deb");
        assert!(is_deb(&named));
        assert!(handles(&named));

        for member in ["debian-binary   ", "debian-binary/  "] {
            let bare = sandbox.file("extensionless-deb", &format!("!<arch>\n{member}"));
            assert!(is_deb(&bare), "{member}");
            assert!(handles(&bare), "{member}");
        }
        for member in [
            "debian-binaryx   ",
            "debian-binary/x  ",
            "debian-binary x  ",
            "debian-binary",
            "debian-binary/",
        ] {
            let bare = sandbox.file("not-a-deb", &format!("!<arch>\n{member}"));
            assert!(!is_deb(&bare), "{member}");
            assert!(!handles(&bare), "{member}");
        }

        // The same container holds a static library, which AppShelf must not
        // be offered.
        let library = sandbox.join("libfoo.a");
        std::fs::write(
            &library,
            b"!<arch>\nfoo.o/          1700000000  0     0     100644  4  ",
        )
        .unwrap();
        assert!(!is_deb(&library));
        assert!(!handles(&library));
    }

    #[test]
    fn rpm_is_recognized_by_extension_and_by_magic() {
        let sandbox = TestDir::new("appshelf-rpm");
        let named = sandbox.file("hello-2.12.1-4.x86_64.rpm", "not really an rpm");
        assert!(is_rpm(&named));
        assert!(handles(&named));

        let bare = sandbox.join("extensionless-rpm");
        std::fs::write(&bare, b"\xed\xab\xee\xdb\x03\x00\x00\x00padding").unwrap();
        assert!(is_rpm(&bare));
        assert!(handles(&bare));
    }
}
