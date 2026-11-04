use super::*;

#[test]
fn executables_written_beside_forking_siblings_exec() {
    let stop = Arc::new(AtomicBool::new(false));
    let siblings: Vec<_> = (0..4)
        .map(|_| {
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let mut command = Command::new("/bin/true");
                    // Holds each child before exec, so it keeps every descriptor this process has open.
                    unsafe {
                        command.pre_exec(|| {
                            thread::sleep(Duration::from_millis(10));
                            Ok(())
                        });
                    }
                    command.status().unwrap();
                }
            })
        })
        .collect();
    for _ in 0..50 {
        let sandbox = Sandbox::new();
        let status = Command::new(sandbox.root.join("flea"))
            .arg("--version")
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        sandbox.frontend();
        let status = Command::new(sandbox.root.join("flea-gui"))
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }
    stop.store(true, Ordering::Relaxed);
    for sibling in siblings {
        sibling.join().unwrap();
    }
}

#[test]
fn sandbox_is_removed_on_drop() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    let root = sandbox.root.clone();
    assert!(root.join(".flea-test-sandbox").is_file());
    drop(sandbox);
    assert!(
        !root.exists(),
        "sandbox was not removed: {}",
        root.display()
    );
}

#[test]
fn sandbox_cleanup_refuses_unmarked_or_out_of_root_paths() {
    let _serial = EXECUTABLE_FIXTURES.lock().unwrap();
    let sandbox = Sandbox::new();
    let root = sandbox.root.clone();
    let unmarked = root.join("flea-gpui-launch.unmarked");
    fs::create_dir(&unmarked).unwrap();
    let probe = Sandbox {
        root: unmarked.clone(),
        temporary: root.clone(),
    };
    assert!(!probe.removable());
    drop(probe);
    assert!(unmarked.exists());
    let probe = Sandbox {
        root: root.clone(),
        temporary: root.join("another-temp-root"),
    };
    assert!(!probe.removable());
    drop(probe);
    assert!(root.join(".flea-test-sandbox").is_file());
    drop(sandbox);
    assert!(!root.exists());
}
