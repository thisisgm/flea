// Window icons against one mount table, including mixed-path listings and SMB extensions.
use super::tests::dbs;
use super::*;
use std::os::unix::fs::MetadataExt;

thread_local! {
    pub(super) static MODE_PATH_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn device(path: &Path) -> u64 {
    std::fs::metadata(path).unwrap().dev()
}

fn lexical_lookups() -> usize {
    mountinfo::LEXICAL_LOOKUPS.with(|lookups| lookups.get())
}

fn second_device(d: &crate::backend::testdir::TestDir) -> u64 {
    let link = d.join("proc-device");
    std::os::unix::fs::symlink("/proc", &link).unwrap();
    let dev = device(&link);
    assert_ne!(dev, device(d.path()), "the fixture needs two real filesystem devices");
    dev
}

// A FAT or exFAT mount reports 0755 on every file, so its execute bits are the mount's, never the file's.
#[test]
fn an_extensionless_program_draws_the_program_mark_only_where_its_mode_is_its_own() {
    let d = crate::backend::testdir::TestDir::new("rows-own-mode");
    let dev = device(d.path());
    let mut l = Listing::new();
    l.push("PinyonShiftLauncher", false);
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o100755, target_is_dir: false, target: String::new(), dev }];
    let exfat = format!("36 25 {} / /mnt/usb rw,relatime - exfat /dev/sdb1 rw\n", mountinfo::dev_majmin(dev));
    let btrfs = exfat.replace("exfat", "btrfs");
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let on_exfat = rows_line_in(Path::new("/mnt/usb"), &exfat, &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(on_exfat.contains(r#""p":33261,"i":"text-x-generic""#), "the mount's 0755 is not a program: {}", on_exfat);
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let on_btrfs = rows_line_in(Path::new("/mnt/usb"), &btrfs, &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(on_btrfs.contains(r#""p":33261,"i":"application-x-executable""#), "a btrfs file keeps its own bits: {}", on_btrfs);
}

#[test]
fn modeless_rows_mark_execute_bits_without_changing_modes_or_plain_window_bytes() {
    let d = crate::backend::testdir::TestDir::new("rows-execute-marker");
    let dev = device(d.path());
    let mut l = Listing::new();
    l.push("Launcher", false);
    let metas = [Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o100755, target_is_dir: false, target: String::new(), dev }];
    let plain = r#"{"t":"rows","start":0,"rows":[{"n":"Launcher","d":false,"s":1,"m":2,"b":null,"p":33261,"i":"application-x-executable","t":false,"k":0}],"kinds":["Data"],"ms":0.000}"#;
    for fstype in ["vfat", "exfat", "ext4"] {
        let body = format!("1 0 {} / / rw - {fstype} /dev/a rw\n", mountinfo::dev_majmin(dev));
        let (mime, icons, aliases, thumbs, mut kinds) = dbs();
        let line = rows_line_in(d.path(), &body, &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
        let parsed = crate::jsondoc::parse(&line).unwrap();
        let row = &parsed.get("rows").unwrap().as_array().unwrap()[0];
        assert_eq!(row.get("p").and_then(crate::jsondoc::Json::as_f64), Some(33261.0), "{fstype} keeps the reported 0755 mode");
        assert_eq!(row.get("x").and_then(crate::jsondoc::Json::as_bool), (fstype != "ext4").then_some(false));
        if fstype == "ext4" {
            assert_eq!(line.len(), plain.len(), "a plain window gains no wire bytes");
            assert_eq!(line, plain, "every original byte of a plain window is preserved");
        }
    }
}

// One window reads the mount table once through the counted seam, so no call site adds a read.
#[test]
fn a_row_window_reads_the_mount_table_once() {
    let mut l = Listing::new();
    l.push("mnt/usb/Launcher", false);
    l.push("home/programs/Launcher", false);
    let metas = (0..2).map(|_| Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o100755, target_is_dir: false, target: String::new(), dev: 0 }).collect::<Vec<_>>();
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    crate::backend::mountinfo::test_reset();
    rows_line(Path::new("/"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert_eq!(crate::backend::mountinfo::test_reads(), 1);
}

#[test]
fn listpaths_rows_use_their_own_mount_modes() {
    let d = crate::backend::testdir::TestDir::new("rows-listpaths-devices");
    let usb_dev = device(d.path());
    let native_dev = second_device(&d);
    let mut l = Listing::new();
    l.push("mnt/usb/Launcher", false);
    l.push("home/programs/Launcher", false);
    let metas = [usb_dev, native_dev].into_iter().map(|dev| Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o100755, target_is_dir: false, target: String::new(), dev }).collect::<Vec<_>>();
    let body = format!("1 0 {} / / rw - ext4 /dev/a rw\n2 1 {} / /mnt/usb rw - exfat /dev/sdb1 rw\n",
        mountinfo::dev_majmin(native_dev), mountinfo::dev_majmin(usb_dev));
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let line = rows_line_in(Path::new("/"), &body, &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    let parsed = crate::jsondoc::parse(&line).unwrap();
    let rows = parsed.get("rows").unwrap().as_array().unwrap();
    assert_eq!(rows[0].get("i").and_then(crate::jsondoc::Json::as_str), Some("text-x-generic"), "an exFAT Recent row has no real execute bits");
    assert_eq!(rows[1].get("i").and_then(crate::jsondoc::Json::as_str), Some("application-x-executable"), "an ext4 Recent row keeps its execute bits");
    for row in rows {
        assert_eq!(row.get("p").and_then(crate::jsondoc::Json::as_f64), Some(33261.0), "the reported mode is preserved");
    }
}

#[test]
fn search_rows_on_an_ext4_mount_keep_modes_under_an_exfat_base() {
    let d = crate::backend::testdir::TestDir::new("rows-search-device");
    let dev = device(d.path());
    let mut l = Listing::new();
    l.push("before", false);
    l.push("native/Launcher", false);
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o100755, target_is_dir: false, target: String::new(), dev }];
    let body = format!("1 0 0:0 / /mnt/usb rw - exfat /dev/sdb1 rw\n2 1 {} / /mnt/usb/native rw - ext4 /dev/a rw\n", mountinfo::dev_majmin(dev));
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let line = rows_line_in(Path::new("/mnt/usb"), &body, &l, &metas, crate::backend::rows::Window { start: 1, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert!(line.contains(r#""n":"native/Launcher","d":false,"s":1,"m":2,"b":null,"p":33261,"i":"application-x-executable""#), "the nested ext4 file keeps its own mode: {line}");
}

#[test]
fn a_gvfs_row_window_never_reads_the_mount_table() {
    let d = crate::backend::testdir::TestDir::new("a_gvfs_row_window_never_reads_the_mount_table");
    let dev = device(d.path());
    let mut l = Listing::new();
    l.push("sub/Launcher", false);
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o100755, target_is_dir: false, target: String::new(), dev }];
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    mountinfo::test_reset();
    rows_line(Path::new("/run/user/1000/gvfs/smb-share:server=nas,share=files"), &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    assert_eq!(mountinfo::test_reads(), 0);
}

#[test]
fn smb_rows_keep_real_modes_only_with_mode_preserving_super_options() {
    let d = crate::backend::testdir::TestDir::new("rows-smb-device");
    let dev = device(d.path());
    let mut l = Listing::new();
    l.push("Launcher", false);
    let metas = vec![Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o100755, target_is_dir: false, target: String::new(), dev }];
    for fstype in ["cifs", "smb3"] {
        for (options, icon) in [
            ("rw,posix", "application-x-executable"),
            ("rw,unix", "application-x-executable"),
            ("rw,cifsacl", "application-x-executable"),
            ("rw,modefromsid", "application-x-executable"),
            ("rw,nounix,noposix", "text-x-generic"),
            ("rw,posix=0,unix=0,cifsacl=0,modefromsid=0", "text-x-generic"),
            ("rw", "text-x-generic"),
        ] {
            let body = format!("1 0 0:0 / / rw - ext4 /dev/a rw\n2 1 {} / /mnt/share rw - {fstype} //server/share {options}\n", mountinfo::dev_majmin(dev));
            let (mime, icons, aliases, thumbs, mut kinds) = dbs();
            let line = rows_line_in(Path::new("/mnt/share"), &body, &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
            let parsed = crate::jsondoc::parse(&line).unwrap();
            let rows = parsed.get("rows").unwrap().as_array().unwrap();
            assert_eq!(rows[0].get("i").and_then(crate::jsondoc::Json::as_str), Some(icon), "{fstype} super options {options}");
            assert_eq!(mountinfo::modes_are_real(Path::new("/mnt/share/Launcher"), &body), icon == "application-x-executable", "the lexical route agrees for {fstype} {options}");
        }
    }
}

fn assert_program_icon(base: &Path, body: &str, names: &[&str], devs: &[u64], expected: &[&str]) {
    assert_eq!(names.len(), devs.len());
    let mut l = Listing::new();
    for name in names {
        l.push(name, false);
    }
    let metas = devs.iter().map(|&dev| Meta { ino: None, birth: None, size: 1, mtime: 2, mode: 0o100755, target_is_dir: false, target: String::new(), dev }).collect::<Vec<_>>();
    let (mime, icons, aliases, thumbs, mut kinds) = dbs();
    let line = rows_line_in(base, body, &l, &metas, crate::backend::rows::Window { start: 0, ms: 0.0 }, crate::backend::rows::Services { mime: &mime, icons: &icons, aliases: &aliases, thumbs: &thumbs, kinds: &mut kinds });
    let parsed = crate::jsondoc::parse(&line).unwrap();
    let rows = parsed.get("rows").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), expected.len());
    for (row, icon) in rows.iter().zip(expected) {
        assert_eq!(row.get("i").and_then(crate::jsondoc::Json::as_str), Some(*icon), "{base:?}: {line}");
        assert_eq!(row.get("p").and_then(crate::jsondoc::Json::as_f64), Some(33261.0), "the reported mode stays intact");
    }
}

#[test]
fn gvfs_phone_rows_have_no_real_execute_bits() {
    let d = crate::backend::testdir::TestDir::new("gvfs_phone_rows_have_no_real_execute_bits");
    let dev = device(d.path());
    for backend in ["mtp", "afc", "gphoto2", "MTP"] {
        let base = format!("/run/user/1000/gvfs/{backend}:host=phone");
        assert_eq!(gvfs_class(Path::new(&base)), Some("phone"));
        assert_program_icon(Path::new(&base), "", &["Launcher", "sub/Launcher"], &[dev; 2], &["text-x-generic"; 2]);
    }
}

#[test]
fn gvfs_network_rows_without_unix_modes_have_no_real_execute_bits() {
    let d = crate::backend::testdir::TestDir::new("gvfs_network_rows_without_unix_modes_have_no_real_execute_bits");
    let dev = device(d.path());
    for backend in ["smb-share", "smb-server", "dav", "davs", "http", "https", "google-drive", "onedrive", "SMB-SHARE"] {
        let base = format!("/run/user/1000/gvfs/{backend}:host=server");
        assert_eq!(gvfs_class(Path::new(&base)), Some("network"));
        assert_program_icon(Path::new(&base), "", &["Launcher", "sub/Launcher"], &[dev; 2], &["text-x-generic"; 2]);
    }
}

#[test]
fn gvfs_sftp_and_unknown_backends_keep_their_execute_bits() {
    let d = crate::backend::testdir::TestDir::new("gvfs_sftp_and_unknown_backends_keep_their_execute_bits");
    let dev = device(d.path());
    for backend in ["sftp", "SFTP", "unknown-backend"] {
        let base = format!("/run/user/1000/gvfs/{backend}:host=server");
        assert_eq!(gvfs_class(Path::new(&base)), Some("network"));
        assert_program_icon(Path::new(&base), "", &["Launcher", "sub/Launcher"], &[dev; 2], &["application-x-executable"; 2]);
    }
    assert_eq!(gvfs_class(Path::new("/unrecognized")), None);
    assert_program_icon(Path::new("/unrecognized"), "", &["Launcher"], &[dev], &["application-x-executable"]);
}

#[test]
fn gvfs_rows_decide_modes_without_resolving_the_base() {
    let d = crate::backend::testdir::TestDir::new("gvfs_rows_decide_modes_without_resolving_the_base");
    let dev = device(d.path());
    let base = Path::new("/run/user/1000/gvfs/smb-share:server=nas,share=files");
    mountinfo::test_reset();
    assert_program_icon(base, "", &["Launcher"], &[dev], &["text-x-generic"]);
    assert_eq!(mountinfo::test_device_lookups(), 0, "gvfs decides before any device lookup");
    assert_no_path_calls();
}

#[test]
fn gvfs_search_rows_decide_modes_without_resolving_their_parent() {
    let d = crate::backend::testdir::TestDir::new("gvfs_search_rows_decide_modes_without_resolving_their_parent");
    let dev = device(d.path());
    let base = Path::new("/run/user/1000/gvfs/smb-share:server=nas,share=files");
    mountinfo::test_reset();
    assert_program_icon(base, "", &["sub/Launcher", "sub/OtherLauncher"], &[dev; 2], &["text-x-generic"; 2]);
    assert_eq!(mountinfo::test_device_lookups(), 0, "gvfs search parents decide before any device lookup");
    assert_no_path_calls();
}

#[test]
fn a_symlinked_window_base_uses_the_target_mounts_modes() {
    let d = crate::backend::testdir::TestDir::new("rows-linked-base");
    let usb = d.dir("usb");
    let dev = device(&usb);
    let link = d.join("linked-usb");
    std::os::unix::fs::symlink(&usb, &link).unwrap();
    for fstype in ["vfat", "exfat", "ext4"] {
        let body = format!("1 0 0:0 / / rw - ext4 /dev/a rw\n2 1 {} / {} rw - {fstype} /dev/b rw\n", mountinfo::dev_majmin(dev), usb.display());
        let icon = if fstype == "ext4" { "application-x-executable" } else { "text-x-generic" };
        mountinfo::test_reset();
        assert_program_icon(&link, &body, &["Launcher", "OtherLauncher"], &[dev; 2], &[icon; 2]);
        assert_eq!(mountinfo::test_device_lookups(), 1, "plain children reuse the device decision");
    }
}

#[test]
fn a_symlinked_gvfs_base_cannot_infer_execute_bits_from_the_proxy_device() {
    let d = crate::backend::testdir::TestDir::new("rows-linked-gvfs");
    let share = d.dir("share");
    let dev = device(&share);
    let link = d.join("nas");
    std::os::unix::fs::symlink(&share, &link).unwrap();
    let mount = Path::new("/run/user/1000/gvfs");
    let body = format!("1 0 0:0 / / rw - ext4 /dev/a rw\n2 1 {} / {} rw - fuse.gvfsd-fuse gvfsd-fuse rw\n", mountinfo::dev_majmin(dev), mount.display());
    assert_eq!(crate::backend::gvfsroot::gvfs_modes_are_real(mount), None);
    assert!(!mountinfo::modes_are_real(mount, &body), "the lexical mountpoint fallback also refuses proxy modes");
    assert!(mountinfo::modes_are_real(&link.join("Launcher"), &body), "only the device lookup identifies the proxy through this symlink");
    mountinfo::test_reset();
    assert_program_icon(&link, &body, &["Launcher", "sub/OtherLauncher"], &[dev; 2], &["text-x-generic"; 2]);
    assert_eq!(mountinfo::test_device_lookups(), 1);
    assert_eq!(lexical_lookups(), 0, "a listed proxy device needs no lexical fallback");
}

#[test]
fn mixed_path_rows_decide_each_distinct_device_once() {
    let d = crate::backend::testdir::TestDir::new("rows-linked-parents");
    let usb = d.dir("usb");
    let native = d.join("native");
    std::os::unix::fs::symlink("/proc", &native).unwrap();
    let (usb_dev, native_dev) = (device(&usb), device(&native));
    assert_ne!(usb_dev, native_dev);
    std::os::unix::fs::symlink(&usb, d.join("linked-usb")).unwrap();
    std::os::unix::fs::symlink(&native, usb.join("linked-native")).unwrap();
    let body = format!("1 0 {} / / rw - ext4 /dev/a rw\n2 1 {} / {} rw - exfat /dev/b rw\n", mountinfo::dev_majmin(native_dev), mountinfo::dev_majmin(usb_dev), usb.display());
    mountinfo::test_reset();
    assert_program_icon(d.path(), &body, &["linked-usb/Launcher", "linked-usb/OtherLauncher", "linked-usb/linked-native/Launcher"],
        &[usb_dev, usb_dev, native_dev], &["text-x-generic", "text-x-generic", "application-x-executable"]);
    assert_eq!(mountinfo::test_device_lookups(), 2, "two devices decide three rows without resolving parents");
    assert_program_icon(&native, &body, &["../linked-usb/Launcher"], &[usb_dev], &["text-x-generic"]);
}

#[test]
fn failed_device_lookup_keeps_the_supplied_mount_path() {
    let d = crate::backend::testdir::TestDir::new("rows-missing-parent");
    let missing = d.join("missing-usb");
    let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n2 1 8:17 / {} rw - exfat /dev/b rw\n", missing.display());
    assert_program_icon(&missing, &body, &["Launcher", "sub/Launcher"], &[0; 2], &["text-x-generic"; 2]);
}

fn assert_no_path_calls() {
    for (file, source) in [
        ("rows.rs", include_str!("rows.rs")),
        ("mountinfo.rs", include_str!("mountinfo.rs")),
        ("gvfsroot.rs", include_str!("gvfsroot.rs")),
        ("fatnames.rs", include_str!("fatnames.rs")),
    ] {
        let production = source.split("\n#[cfg(test)]\nmod tests {").next().unwrap();
        let calls: String = production.chars().filter(|ch| !ch.is_whitespace()).collect();
        for api in ["canonicalize(", "read_link(", "fstatat(", "statx("] {
            assert_eq!(calls.matches(api).count(), 0, "{file} must not resolve or stat a row path through {api}");
        }
        let source_stats = usize::from(file == "fatnames.rs");
        assert_eq!(calls.matches("symlink_metadata(").count(), source_stats, "{file} keeps only its existing source lstat");
        assert_eq!(calls.matches("metadata(").count() - calls.matches("symlink_metadata(").count(), 0, "{file} must not add a metadata call");
    }
}

#[test]
fn row_mode_decisions_never_resolve_or_stat_paths() {
    assert_no_path_calls();
}

#[test]
fn an_unlisted_device_keeps_each_rows_lexical_mount_as_its_fallback() {
    let d = crate::backend::testdir::TestDir::new("rows-unlisted-device");
    let usb = d.dir("usb");
    for fstype in ["ext4", "btrfs"] {
        let body = format!("1 0 0:0 / / rw - {fstype} /dev/a rw\n2 1 0:0 / {} rw - exfat /dev/b rw\n", usb.display());
        for dev in [0, device(d.path())] {
            mountinfo::test_reset();
            assert_program_icon(d.path(), &body, &["usb/Launcher", "native/Launcher"], &[dev; 2],
                &["text-x-generic", "application-x-executable"]);
            assert_eq!(mountinfo::test_device_lookups(), 1, "a missing device is cached, but each row keeps its own lexical fallback");
            assert_eq!(lexical_lookups(), 2, "different parents keep separate lexical decisions");
        }
    }
}

#[test]
fn plain_windows_cache_lexical_fallback_per_unlisted_device() {
    let d = crate::backend::testdir::TestDir::new("rows-lexical-cache");
    let dev = device(d.path());
    let other = second_device(&d);
    let names: Vec<String> = (0..64).map(|i| format!("Launcher{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let body = "1 0 0:0 / / rw - btrfs /dev/a rw\n";
    for (devs, expected) in [
        (vec![dev; 64], 1),
        ((0..64).map(|i| if i % 2 == 0 { dev } else { other }).collect(), 2),
    ] {
        mountinfo::test_reset();
        MODE_PATH_BUILDS.with(|builds| builds.set(0));
        assert_program_icon(d.path(), body, &names, &devs, &["application-x-executable"; 64]);
        assert_eq!(mountinfo::test_device_lookups(), expected, "device decisions stay cached too");
        assert_eq!(lexical_lookups(), expected, "64 plain rows decide lexical modes once per unlisted device");
        assert_eq!(MODE_PATH_BUILDS.with(|builds| builds.get()), expected, "64 plain rows build a mode PathBuf only on a cache miss");
    }
}

#[test]
fn lexical_parent_keys_keep_absolute_root_children_separate() {
    let d = crate::backend::testdir::TestDir::new("rows-root-parent-key");
    let dev = device(d.path());
    let body = format!("1 0 0:0 / / rw - btrfs /dev/a rw\n2 1 0:0 / {} rw - exfat /dev/b rw\n", d.path().display());
    mountinfo::test_reset();
    assert_program_icon(d.path(), &body, &["Launcher", "/Launcher"], &[dev; 2],
        &["text-x-generic", "application-x-executable"]);
    assert_eq!(lexical_lookups(), 2, "root and relative parents cannot share a borrowed cache key");
}

#[test]
fn gvfs_spelling_wins_over_the_shared_proxy_device() {
    let d = crate::backend::testdir::TestDir::new("rows-gvfs-device");
    let dev = device(d.path());
    let body = format!("1 0 {} / / rw - exfat /dev/a rw\n", mountinfo::dev_majmin(dev));
    mountinfo::test_reset();
    assert_program_icon(Path::new("/"), &body,
        &["run/user/1000/gvfs/smb-share:server=nas,share=files/Launcher", "run/user/1000/gvfs/sftp:host=nas/Launcher"],
        &[dev; 2], &["text-x-generic", "application-x-executable"]);
    assert_eq!(mountinfo::test_device_lookups(), 0, "the same FUSE device cannot override different backend rules");
}

#[test]
fn smb_device_modes_use_only_the_matching_mounts_super_options() {
    let d = crate::backend::testdir::TestDir::new("rows-smb-device-options");
    let dev = device(d.path());
    for fstype in ["cifs", "smb3"] {
        let body = format!("1 0 0:0 / / rw - {fstype} //server/root rw,posix\n2 1 {} / /share rw,posix,unix - {fstype} //server/share rw\n", mountinfo::dev_majmin(dev));
        assert_eq!(mountinfo::modes_are_real_for_dev(dev, &body), Some(false), "raw and unrelated super options cannot enable modes");
        let unix = body.replace("//server/share rw\n", "//server/share rw,unix\n");
        assert_eq!(mountinfo::modes_are_real_for_dev(dev, &unix), Some(true));
        let posix = body.replace("//server/share rw\n", "//server/share rw,posix\n");
        assert_eq!(mountinfo::modes_are_real_for_dev(dev, &posix), Some(true));
        assert_eq!(mountinfo::modes_are_real_for_dev(0, &body), None, "zero always uses the lexical fallback");
    }
}
