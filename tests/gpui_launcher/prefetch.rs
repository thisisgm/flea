use super::*;
use std::io::Read;
use std::os::fd::FromRawFd;
use std::os::unix::ffi::OsStrExt;

fn watch_list_open(path: &Path) -> fs::File {
    extern "C" {
        fn inotify_init1(flags: i32) -> i32;
        fn inotify_add_watch(fd: i32, path: *const std::ffi::c_char, mask: u32) -> i32;
    }
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    let descriptor = unsafe { inotify_init1(0x800 | 0x80000) };
    assert!(descriptor >= 0, "{}", std::io::Error::last_os_error());
    let file = unsafe { fs::File::from_raw_fd(descriptor) };
    let watch = unsafe { inotify_add_watch(descriptor, path.as_ptr(), 0x20) };
    assert!(watch >= 0, "{}", std::io::Error::last_os_error());
    file
}

fn assert_list_was_opened(mut watch: fs::File) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut event = [0u8; 256];
    loop {
        match watch.read(&mut event) {
            Ok(size) if size >= 16 => {
                let mask = u32::from_ne_bytes(event[4..8].try_into().unwrap());
                assert_ne!(mask & 0x20, 0, "expected an IN_OPEN event, got {mask}");
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            result => panic!("reading prefetch open event failed: {result:?}"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the launch helper never opened the recorded prefetch list"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

// The stub window reports its own pid and the page-cache handoff it inherited.
fn prefetch_frontend(sandbox: &Sandbox) {
    let path = sandbox.root.join("flea-gui");
    write_in_child(
        &path,
        r#"#!/usr/bin/python3
import json,os,sys
keys=('FLEA_PREFETCH','FLEA_PREFETCH_SHELL')
print(json.dumps({'pid':str(os.getpid()),'env':{key:os.environ.get(key) for key in keys}}))
"#
        .as_bytes(),
    );
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn handoff(output: &Output) -> (String, Option<String>, Option<String>) {
    let document = reply(output);
    let env = document.get("env").unwrap();
    let value = |key: &str| env.get(key).and_then(Json::as_str).map(str::to_owned);
    (
        document
            .get("pid")
            .and_then(Json::as_str)
            .unwrap()
            .to_owned(),
        value("FLEA_PREFETCH"),
        value("FLEA_PREFETCH_SHELL"),
    )
}

#[test]
fn gpui_main_window_gets_the_prefetch_list_and_records_as_its_own_shell() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    prefetch_frontend(&sandbox);
    let list = sandbox.root.join("cache/flea/prefetch");
    for arguments in [vec![], vec!["files"]] {
        let output = sandbox
            .command()
            .current_dir(&sandbox.root)
            .args(&arguments)
            .env("FLEA_PREFETCH", "/stale-parent/prefetch")
            .env("FLEA_PREFETCH_SHELL", "1")
            .output()
            .unwrap();
        let (pid, prefetch, shell) = handoff(&output);
        assert_eq!(prefetch.as_deref(), list.to_str(), "{arguments:?}");
        // exec keeps the launcher's pid, so the window is the shell its backend records.
        assert_eq!(shell, Some(pid), "{arguments:?}");
    }
    // A recorded list is warmed by the helper before the exec, and the launch still opens the window.
    fs::create_dir_all(list.parent().unwrap()).unwrap();
    fs::write(
        &list,
        format!(
            "flea-prefetch 2\n0 4096 {}\n",
            sandbox.root.join("flea").display()
        ),
    )
    .unwrap();
    let watch = watch_list_open(&list);
    let (pid, prefetch, shell) = handoff(&sandbox.command().output().unwrap());
    assert_eq!(prefetch.as_deref(), list.to_str());
    assert_eq!(shell, Some(pid));
    assert_list_was_opened(watch);
}

#[test]
fn gpui_unlock_launch_neither_warms_nor_records_the_window_list() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    prefetch_frontend(&sandbox);
    let output = sandbox
        .launch_command()
        .args(["--unlock", "/dev/mapper/vault"])
        .env("FLEA_PREFETCH", "/stale-parent/prefetch")
        .env("FLEA_PREFETCH_SHELL", "1")
        .output()
        .unwrap();
    let (_, prefetch, shell) = handoff(&output);
    assert_eq!(prefetch, None);
    assert_eq!(shell, None);
}
