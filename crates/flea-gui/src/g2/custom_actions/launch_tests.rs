//! Open in Agent's spawn: a fixture argv records the folder, process group, stdio targets and argv.
use super::launch::{launch_argv, launch_reaped};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn scratch() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g2-mn2")
}

// A marked root made by the test, removed on drop only while its marker stands under the unit's scratch.
struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Sandbox {
        let parent = scratch();
        fs::create_dir_all(&parent).unwrap();
        let output = Command::new("mktemp")
            .arg("-d")
            .arg(parent.join("launch.XXXXXX"))
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = fs::canonicalize(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
        fs::write(root.join(".flea-test-sandbox"), "mn2 launch test root\n").unwrap();
        Sandbox(root)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Hard rule 9: an absolute root, inside the unit's scratch, that still holds its marker.
        let Ok(parent) = fs::canonicalize(scratch()) else {
            return;
        };
        let root = &self.0;
        if root.is_absolute()
            && root.starts_with(&parent)
            && root != &parent
            && root.join(".flea-test-sandbox").is_file()
        {
            let _ = fs::remove_dir_all(root);
        }
    }
}

// Stands in for omarchy: it writes its working directory, process group, stdio targets and argv, one per line.
fn fixture(root: &Path) -> PathBuf {
    let record = root.join("record");
    let program = root.join("omarchy");
    let script = format!(
        "#!/bin/sh\nr0=$(readlink /proc/$$/fd/0); r1=$(readlink /proc/$$/fd/1); r2=$(readlink /proc/$$/fd/2)\n{{\npwd -P\nawk '{{print $5}}' /proc/$$/stat\nprintf '%s\\n' \"$r0\" \"$r1\" \"$r2\"\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done\n}} > '{}'\n",
        record.display()
    );
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    program
}

// The test process's own group, read from its stat line: the fields after the command name's closing paren.
// Sample input: "4242 (flea_gui-1a2b) S 4200 4242 4242 0 -1 ...": the third field after ") " is the process group.
fn own_group() -> String {
    let stat = fs::read_to_string("/proc/self/stat").unwrap();
    let after = &stat[stat.rfind(')').unwrap() + 2..];
    after.split(' ').nth(2).unwrap().to_string()
}

#[test]
fn mn2_r2_the_agent_runs_detached_in_the_folder_with_its_argv() {
    let root = Sandbox::new();
    let program = fixture(&root.0);
    let folder = root.0.join("photos with space");
    fs::create_dir(&folder).unwrap();
    // The runner may hand this test /dev/null as stdin (the suite and the CI lane do), so the spawn runs one level down: the helper inherits pipes on every fd and only a real null stdio in the launch reads back /dev/null.
    let helper = Command::new(std::env::current_exe().unwrap())
        .arg("mn2_launch_helper_spawn_from_pipes")
        .arg("--ignored")
        .env("MN2_FIXTURE_PROGRAM", &program)
        .env("MN2_FIXTURE_FOLDER", &folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the piped helper spawns");
    let out = helper.wait_with_output().expect("the helper exits");
    assert!(
        out.status.success(),
        "the helper drove the agent: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let record = fs::read_to_string(root.0.join("record")).expect("the fixture recorded");
    let mut lines = record.lines();
    let cwd = folder.canonicalize().unwrap();
    assert_eq!(
        lines.next(),
        cwd.to_str(),
        "the folder is the working directory"
    );
    let group = lines.next().expect("the fixture's group");
    assert_ne!(group, own_group(), "the agent leads its own process group");
    let fds: Vec<_> = (0..3).map(|_| lines.next()).collect();
    assert_eq!(
        fds,
        [Some("/dev/null"), Some("/dev/null"), Some("/dev/null")],
        "fds 0, 1 and 2 are null"
    );
    assert_eq!(
        lines.collect::<Vec<_>>(),
        ["agent", "--pick"],
        "argv runs with no shell in front"
    );
}

// Re-executed by the stdio test with piped stdio on every fd: it spawns the agent from fds that are never /dev/null, so only `launch_argv` nulling them reads back null.
#[test]
#[ignore]
fn mn2_launch_helper_spawn_from_pipes() {
    let (Ok(program), Ok(folder)) = (
        std::env::var("MN2_FIXTURE_PROGRAM"),
        std::env::var("MN2_FIXTURE_FOLDER"),
    ) else {
        return;
    };
    let argv = [program, "agent".into(), "--pick".into()];
    let mut child = launch_argv(&argv, Path::new(&folder)).expect("the agent spawns");
    assert!(child.wait().expect("the agent exits").success());
}

#[test]
fn mn2_r2_a_folder_that_is_gone_spawns_nothing() {
    let root = Sandbox::new();
    let program = fixture(&root.0);
    let gone = root.0.join("removed");
    let argv = [program.display().to_string(), "agent".to_string()];
    assert!(launch_argv(&argv, &gone).is_err(), "no folder, no agent");
    assert!(!root.0.join("record").exists(), "the fixture never ran");
}

#[test]
fn int2_the_reaped_launch_runs_the_agent_and_says_a_failed_spawn() {
    let root = Sandbox::new();
    let program = fixture(&root.0);
    let argv = [program.display().to_string(), "agent".to_string()];
    assert!(
        launch_reaped(&argv, &root.0.join("removed")).is_err(),
        "a failed spawn is an error to say"
    );
    launch_reaped(&argv, &root.0).expect("the reaper and the agent start");
    let record = root.0.join("record");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !record.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(record.exists(), "the agent ran under the reaper");
}
