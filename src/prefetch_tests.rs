// src/prefetch.rs's tests, kept beside it so that file stays a mechanism file.
use super::*;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn only_present_file_pages_are_kept_in_first_mapped_order_and_merged() {
    // The font is mapped first and sorts last, so a path-sorted list would put the library ahead of it.
    let maps = "1000-3000 r--p 00002000 00:1f 3                /usr/share/fonts/a b.ttf\n\
                3000-6000 r--p 00000000 00:1f 1 /usr/lib/libQt6Qml.so.6\n\
                6000-7000 rw-p 00000000 00:00 0 [heap]\n\
                7000-8000 r--p 00000000 00:1f 2 /tmp/gone.so (deleted)\n\
                8000-a000 r-xp 00003000 00:1f 1 /usr/lib/libQt6Qml.so.6\n";
    let present = |start: u64, count: u64| -> Vec<bool> {
        match start {
            0x1000 => vec![false, true],
            0x3000 => vec![true, true, false],
            0x8000 => vec![true, true],
            _ => vec![true; count as usize],
        }
    };
    let ranges = ranges_from(maps, 0x1000, present);
    let want = vec![
        Range { path: "/usr/share/fonts/a b.ttf".into(), offset: 0x3000, length: 0x1000 },
        Range { path: "/usr/lib/libQt6Qml.so.6".into(), offset: 0, length: 0x2000 },
        Range { path: "/usr/lib/libQt6Qml.so.6".into(), offset: 0x3000, length: 0x2000 },
    ];
    assert_eq!(ranges, want);
}

#[test]
fn a_list_needs_its_header_skips_bad_lines_and_is_capped() {
    assert!(parse_list("0 4096 /usr/lib/libc.so.6\n").is_empty());
    let text = format!("{HEADER}\n0 4096 /usr/lib/libc.so.6\nx 1 /a\n0 0 /b\n0 4096 relative\n8192 4096 /a b\n");
    let parsed: Vec<(&str, u64, u64)> = parse_list(&text).iter().map(|r| (r.path, r.offset, r.length)).collect();
    assert_eq!(parsed, vec![("/usr/lib/libc.so.6", 0, 4096), ("/a b", 8192, 4096)]);
    let mut long = String::from(HEADER);
    for i in 0..(MAX_RANGES + 10) {
        long.push_str(&format!("\n0 4096 /usr/lib/lib{i}.so"));
    }
    assert_eq!(parse_list(&long).len(), MAX_RANGES);
}

#[test]
fn the_list_is_written_whole_private_and_read_back() {
    let dir = crate::backend::testdir::TestDir::new("prefetch-write");
    let list = dir.path().join("cache/flea/prefetch");
    write_list(&list, "shell 4242 1234567", &[Range { path: "/usr/lib/libc.so.6".into(), offset: 0, length: 4096 }]);
    let text = read_bounded(&list).unwrap();
    let parsed: Vec<(&str, u64, u64)> = parse_list(&text).iter().map(|r| (r.path, r.offset, r.length)).collect();
    assert_eq!(parsed, vec![("/usr/lib/libc.so.6", 0, 4096)], "the shell line is not read as a range");
    assert_eq!(recorded_by(&list).as_deref(), Some("shell 4242 1234567"));
    assert_eq!(fs::metadata(&list).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(fs::read_dir(list.parent().unwrap()).unwrap().count(), 1, "no temp file is left behind");
}

#[test]
fn only_a_regular_file_is_opened() {
    let dir = crate::backend::testdir::TestDir::new("prefetch-open");
    let file = dir.file("real.so", "x");
    let link = dir.join("link.so");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    let fifo = dir.join("fifo");
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    assert!(open_regular(file.to_str().unwrap()).is_some());
    assert!(open_regular(link.to_str().unwrap()).is_none(), "a symlink is refused");
    assert!(open_regular(fifo.to_str().unwrap()).is_none(), "a FIFO is refused without blocking");
    assert!(open_regular(dir.path().to_str().unwrap()).is_none(), "a directory is refused");
}

#[test]
fn the_open_itself_refuses_what_a_swap_can_leave_at_the_path() {
    // open_checked runs after the pre-check, so these reach the flags rather than the stat.
    let dir = crate::backend::testdir::TestDir::new("prefetch-swap");
    let file = dir.file("real.so", "x");
    let link = dir.join("link.so");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    assert!(open_checked(link.to_str().unwrap()).is_none(), "O_NOFOLLOW refuses a final symlink");
    let fifo = dir.join("fifo");
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    let (done, refused) = mpsc::channel();
    let path = fifo.to_str().unwrap().to_string();
    // Without O_NONBLOCK the open waits for a writer forever, so it runs aside and a timeout reads as red.
    std::thread::spawn(move || {
        let _ = done.send(open_checked(&path).is_none());
    });
    assert_eq!(refused.recv_timeout(Duration::from_secs(5)), Ok(true), "a FIFO opens without blocking and is refused");
}

#[test]
fn a_fifo_left_at_the_list_path_is_never_waited_on() {
    let dir = crate::backend::testdir::TestDir::new("prefetch-listfifo");
    let fifo = dir.join("prefetch");
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    let (done, read) = mpsc::channel();
    // The launcher reads the list before the window, so a blocking open here is a launch with no window.
    std::thread::spawn(move || {
        let _ = done.send(read_bounded(&fifo).is_none());
    });
    assert_eq!(read.recv_timeout(Duration::from_secs(5)), Ok(true));
}

#[test]
fn only_the_shell_the_launcher_named_records() {
    assert!(is_launch_shell(4242, Some("4242")));
    assert!(!is_launch_shell(4242, Some("4241")), "a backend under another parent");
    assert!(!is_launch_shell(4242, None), "no shell named");
    assert!(!is_launch_shell(4242, Some("shell")), "not a pid");
}

#[test]
fn start_time_is_stat_field_22_even_when_the_name_holds_parentheses() {
    // Every field before starttime differs, so an index off by one reads another number.
    let stat = "4242 (qs (x) y) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 0 5561234 123456 789\n";
    assert_eq!(start_time(stat), Some("5561234"));
    assert_eq!(start_time("4242 no name"), None, "a line with no closing parenthesis names no field");
}

#[test]
fn a_live_shell_is_named_by_its_pid_and_a_real_start_time() {
    let me = std::process::id();
    let identity = shell_identity(me).unwrap();
    let start: u64 = identity.rsplit(' ').next().unwrap().parse().unwrap();
    assert_eq!(identity, format!("shell {me} {start}"));
    // itrealvalue, the field before starttime, is always 0 on Linux.
    assert!(start > 0, "{identity}");
    let mut later = Command::new("sleep").arg("5").spawn().unwrap();
    let theirs: u64 = shell_identity(later.id()).unwrap().rsplit(' ').next().unwrap().parse().unwrap();
    let _ = later.kill();
    let _ = later.wait();
    assert!(theirs >= start, "a process started after this one read {theirs}, before {start}");
}

#[test]
fn foreign_and_newer_lists_survive_warm_and_first_record() {
    let d = crate::backend::testdir::TestDir::new("prefetch-preserve");
    let path = d.join("prefetch");
    for original in ["{\"foreign\":true}\n", "flea-prefetch 999\nshell 1 1\n0 4096 /a\n", "unparsable"] {
        std::fs::write(&path, original).unwrap();
        do_warm(&path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        write_list(&path, "shell 1 1", &[Range { path: "/a".into(), offset: 0, length: 4096 }]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }
}
