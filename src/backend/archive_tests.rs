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

    // Flea reads rar and writes none: 7z's reader is preferred where installed, and libarchive is the fallback.
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
    fn a_zip_is_written_by_7z_when_it_is_installed_and_links_stay_links() {
        let both = Formats::from_tools(true, true);
        let zip = both.compress_argv("zip", Path::new("/x/out.zip"), Path::new("/src"), &["a".to_string()]).unwrap();
        assert_eq!(&zip[..6], &["7z", "a", "-bd", "-spd", "-snl", "-tzip"], "{:?}", zip);
        assert_eq!(zip.last().unwrap(), "/src/a");
        let seven = both.compress_argv("7z", Path::new("/x/out.7z"), Path::new("/src"), &["a".to_string()]).unwrap();
        assert!(seven.iter().any(|s| s == "-snl") && !seven.iter().any(|s| s == "-tzip"), "{:?}", seven);
        let tar_only = Formats::from_tools(true, false);
        let zip = tar_only.compress_argv("zip", Path::new("/x/out.zip"), Path::new("/src"), &["a".to_string()]).unwrap();
        assert_eq!(zip[0], "bsdtar", "a box without 7z keeps bsdtar's zip: {:?}", zip);
        let tgz = both.compress_argv("tar.gz", Path::new("/x/out.tar.gz"), Path::new("/src"), &["a".to_string()]).unwrap();
        assert_eq!(tgz[0], "bsdtar", "a tar stays with bsdtar: {:?}", tgz);
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

    // ISO, deb, rpm, apk, jar, cab, cpio and AppImage read through 7z in the same sandboxed path as every other archive.
    #[test]
    fn seven_zip_reads_the_package_and_disk_formats() {
        let both = Formats::from_tools(true, true);
        for name in ["/x/driver.deb", "/x/pkg.rpm", "/x/app.apk", "/x/lib.jar",
                     "/x/data.cab", "/x/dump.cpio", "/x/run.AppImage", "/x/debian-13.iso"] {
            let reader = both.reader_for(Path::new(name));
            assert_eq!(reader, Some(Reader::SevenZip), "{} must route to 7z", name);
            assert!(both.list_argv(Path::new(name)).is_some(), "{} must list", name);
        }
    }

    // AA11: without 7z the widened names libarchive reads fall back to bsdtar as v0.3.8 did; with 7z they use it.
    #[test]
    fn a_box_without_7z_reads_the_widened_names_through_bsdtar() {
        let tar_only = Formats::from_tools(true, false);
        for name in ["/x/driver.deb", "/x/pkg.rpm", "/x/app.apk", "/x/lib.jar", "/x/data.cab",
                     "/x/dump.cpio", "/x/debian-13.iso"] {
            assert_eq!(tar_only.reader_for(Path::new(name)), Some(Reader::Bsdtar), "{} falls back to libarchive", name);
            assert_eq!(tar_only.list_argv(Path::new(name)).expect("it lists").0[0], "bsdtar");
        }
        assert_eq!(tar_only.extract_argv(Path::new("/x/driver.deb"), Path::new("/out")).expect("a package extracts")[0], "bsdtar");
        assert_eq!(Formats::from_tools(false, false).reader_for(Path::new("/x/driver.deb")), None, "no tool reads nothing");
        assert_eq!(tar_only.reader_for(Path::new("/x/run.AppImage")), None, "bsdtar cannot read a squashfs AppImage");
        assert!(tar_only.list_argv(Path::new("/x/run.AppImage")).is_none());
    }

    // A disk image lists only: extracting one builds no argv, so the client never starts a job for it.
    #[test]
    fn a_disk_image_lists_but_never_extracts() {
        let both = Formats::from_tools(true, true);
        assert!(both.list_argv(Path::new("/x/debian-13.iso")).is_some(), "an ISO must list");
        assert!(both.extract_argv(Path::new("/x/debian-13.iso"), Path::new("/out")).is_none(),
                "and it must never extract");
        assert!(both.extract_argv(Path::new("/x/driver.deb"), Path::new("/out")).is_some(),
                "while a package extracts");
    }

    // A selected name holding * or ? is taken literally: 7z gets -spd, so it never widens one into a wildcard.
    #[test]
    fn a_7z_compress_takes_names_literally() {
        let f = Formats::from_tools(true, true);
        let seven = f.compress_argv("7z", Path::new("/x/out.7z"), Path::new("/src"), &["report*".to_string()]).unwrap();
        assert!(seven.iter().any(|s| s == "-spd"), "a 7z compress must disable wildcard matching: {:?}", seven);
        assert_eq!(seven.last().unwrap(), "/src/report*");
        let zip = f.compress_argv("zip", Path::new("/x/out.zip"), Path::new("/src"), &["report*".to_string()]).unwrap();
        assert_eq!(zip[0], "7z");
        assert!(zip.iter().any(|s| s == "-spd"), "a zip through 7z takes names literally too: {:?}", zip);
    }

    // A dash-leading member is still a name on the 7z path: its members are absolute paths, so no dash reads as an option.
    #[test]
    fn a_dash_leading_member_stays_a_name_on_the_7z_path() {
        let f = Formats::from_tools(true, true);
        let a = f.compress_argv("7z", Path::new("/x/out.7z"), Path::new("/src"),
                                &["--evil".to_string()]).unwrap();
        assert_eq!(a.last().unwrap(), "/src/--evil");
        assert!(!a.iter().any(|s| s.contains("--evil") && s.starts_with('-')),
                "no 7z argument may start with the member's own dashes: {:?}", a);
    }

// An encrypted zip needs 7z even where bsdtar is the zip's plain reader: the probe and the password extract name 7z, the plain extract stays.
#[test]
fn a_zip_is_probed_and_unlocked_by_7z_while_its_plain_extract_stays_with_bsdtar() {
    use crate::backend::archivepass::{extract_argv, reads_password, runs_on_seven};
    let both = Formats::from_tools(true, true);
    let zip = Path::new("/x/a.ZIP");
    assert_eq!(both.technical_list_argv(zip).expect("7z probes a zip")[0], "7z");
    assert!(reads_password(&both, zip));
    assert_eq!(extract_argv(&both, zip, Path::new("/out")).expect("7z unlocks a zip")[0], "7z");
    assert!(runs_on_seven(&both, zip, Some("pw")));
    assert!(!runs_on_seven(&both, zip, None), "a plain zip extract runs bsdtar's argv");
    assert_eq!(both.extract_argv(zip, Path::new("/out")).expect("a plain zip extracts")[0], "bsdtar");
    let tar_only = Formats::from_tools(true, false);
    assert!(tar_only.technical_list_argv(zip).is_none() && !reads_password(&tar_only, zip), "no 7z, no password route");
}

/// A user format follows the built-in ones in the submenu, builds its own argv and names its program
/// for the jail; one claiming a built-in id never reaches the table, whatever the parser let through.
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
