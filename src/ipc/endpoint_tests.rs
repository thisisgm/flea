use super::*;
use std::os::unix::fs::symlink;

fn root() -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".superpowers/g04e");
    fs::create_dir_all(&parent).unwrap();
    let output = std::process::Command::new("mktemp")
        .args(["-d"])
        .arg(parent.join("endpoint.XXXXXX"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    fs::write(root.join(".flea-test-sandbox"), "IPC tests\n").unwrap();
    root
}

#[test]
fn socket_and_directory_are_private_and_symlinks_are_refused() {
    let root = root();
    let endpoint = Endpoint::bind(&root).unwrap();
    assert_eq!(
        fs::metadata(root.join("flea-ipc")).unwrap().mode() & 0o777,
        0o700
    );
    assert_eq!(fs::metadata(&endpoint.path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        instances(&root).unwrap(),
        [(std::process::id(), endpoint.path.clone())]
    );
    assert!(
        Endpoint::bind(&root).is_err(),
        "a live endpoint must never be replaced"
    );
    let path = endpoint.path.clone();
    drop(endpoint);
    assert!(!path.exists());
    let other = root.join("other");
    fs::create_dir(&other).unwrap();
    let linked = root.join("linked");
    fs::create_dir(&linked).unwrap();
    fs::set_permissions(&linked, fs::Permissions::from_mode(0o700)).unwrap();
    symlink(&other, linked.join("flea-ipc")).unwrap();
    assert!(directory(&linked).is_err());
    fs::set_permissions(root.join("flea-ipc"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(directory(&root).is_err());
}

#[test]
fn stale_requires_both_a_dead_pid_and_a_refused_connection() {
    let root = root();
    let directory = directory(&root).unwrap();
    let child = std::process::Command::new("true").spawn().unwrap();
    let dead = child.id();
    let mut child = child;
    child.wait().unwrap();
    assert!(!alive(dead));
    let stale = directory.join(format!("{dead}.sock"));
    let listener = UnixListener::bind(&stale).unwrap();
    fs::set_permissions(&stale, fs::Permissions::from_mode(0o600)).unwrap();
    prune(&directory).unwrap();
    assert!(
        stale.exists(),
        "a connectable endpoint is retained even under a dead pid"
    );
    drop(listener);
    prune(&directory).unwrap();
    assert!(!stale.exists());
    let live = directory.join(format!("{}.sock", std::process::id()));
    let listener = UnixListener::bind(&live).unwrap();
    fs::set_permissions(&live, fs::Permissions::from_mode(0o600)).unwrap();
    drop(listener);
    prune(&directory).unwrap();
    assert!(
        live.exists(),
        "a live pid with a refusing socket is retained"
    );
    let file = directory.join(format!("{dead}.sock"));
    fs::write(&file, "not a socket").unwrap();
    prune(&directory).unwrap();
    assert_eq!(fs::read_to_string(file).unwrap(), "not a socket");
}
