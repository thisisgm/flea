//! flea --terminal against a stand-in xdg-terminal-exec: the folder reaches the terminal it resolves.
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// Ghostty's entry names --working-directory= and one instance; foot's names none, so the launcher cds.
const LAUNCHER: &str = r#"dir=
print=
for a; do case $a in --dir=*) dir=${a#--dir=};; '--print-cmd=\0') print=1;; --print-cmd*) exit 2;; esac; done
[ -n "$print" ] && [ "$FLEA_TEST_PRINTS" != 1 ] && exit 1
set -- "$FLEA_TEST_ROOT/bin/$FLEA_TEST_TERMINAL"
if [ "$FLEA_TEST_TERMINAL" = ghostty ]; then set -- "$@" --gtk-single-instance=true "--working-directory=$dir"; fi
if [ -n "$print" ]; then
  first=1
  for a; do [ -n "$first" ] || printf '\0'; first=; printf '%s' "$a"; done
  exit 0
fi
[ "$FLEA_TEST_TERMINAL" = ghostty ] || cd "$dir"
exec "$@"
"#;

// A running single instance opens every new window in the folder it was started in.
const GHOSTTY: &str = r#"single=false
wd=$PWD
for a; do case $a in --gtk-single-instance=*) single=${a#*=};; --working-directory=*) wd=${a#*=};; esac; done
[ "$single" = true ] && wd=$(cat "$FLEA_TEST_ROOT/primary")
printf '%s\n' "$wd" > "$FLEA_TEST_ROOT/opened.part" && mv "$FLEA_TEST_ROOT/opened.part" "$FLEA_TEST_ROOT/opened"
"#;

const FOOT: &str = r#"printf '%s\n' "$PWD" > "$FLEA_TEST_ROOT/opened.part" && mv "$FLEA_TEST_ROOT/opened.part" "$FLEA_TEST_ROOT/opened"
"#;

struct Sandbox(PathBuf);

// A sibling fork inherits any descriptor held open for writing, and exec then fails with ETXTBSY.
fn executable(path: &Path, body: &str) {
    let mut writer = Command::new("/bin/sh")
        .args(["-c", "/bin/cat > \"$1\"", "write-fixture"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let script = format!("#!/bin/sh\n{body}");
    writer
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    assert!(writer.wait().unwrap().success());
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

impl Sandbox {
    fn new() -> Self {
        let output = Command::new("mktemp").arg("-d").output().unwrap();
        assert!(output.status.success());
        let root = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(root.is_absolute());
        fs::write(root.join(".flea-test-sandbox"), "terminal-launch\n").unwrap();
        for dir in ["bin", "home", "target dir", "elsewhere"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("target dir/notes.txt"), "plain text\n").unwrap();
        fs::write(root.join("elsewhere/real.txt"), "plain text\n").unwrap();
        std::os::unix::fs::symlink(
            root.join("elsewhere/real.txt"),
            root.join("target dir/link.txt"),
        )
        .unwrap();
        std::os::unix::fs::symlink(root.join("gone"), root.join("target dir/dead")).unwrap();
        fs::write(
            root.join("primary"),
            format!("{}\n", root.join("home").display()),
        )
        .unwrap();
        executable(&root.join("bin/xdg-terminal-exec"), LAUNCHER);
        executable(&root.join("bin/ghostty"), GHOSTTY);
        executable(&root.join("bin/foot"), FOOT);
        Self(root)
    }

    fn open(&self, terminal: &str, path: &Path) -> (Option<i32>, Option<String>) {
        self.launch(terminal, path, true, &self.0.join("home"))
    }

    // The exit of flea --terminal and the folder the stand-in terminal opened in, if it opened.
    fn launch(
        &self,
        terminal: &str,
        path: &Path,
        prints: bool,
        cwd: &Path,
    ) -> (Option<i32>, Option<String>) {
        let status = Command::new(env!("CARGO_BIN_EXE_flea"))
            .arg("--terminal")
            .arg(path)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.0.join("bin").display()),
            )
            .env("FLEA_TEST_ROOT", &self.0)
            .env("FLEA_TEST_TERMINAL", terminal)
            .env("FLEA_TEST_PRINTS", if prints { "1" } else { "0" })
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        let opened = self.0.join("opened");
        let deadline = Instant::now() + Duration::from_secs(5);
        while status.success() && !opened.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let folder = fs::read_to_string(&opened)
            .ok()
            .map(|s| s.trim_end().to_owned());
        let _ = fs::remove_file(&opened);
        (status.code(), folder)
    }

    // Canonical, the way flea --terminal hands the folder on.
    fn target(&self) -> String {
        fs::canonicalize(self.0.join("target dir"))
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if self.0.join(".flea-test-sandbox").is_file() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

// #373: a single-instance Ghostty already running took the window to its own folder.
#[test]
fn a_running_single_instance_terminal_still_opens_in_the_folder() {
    let sandbox = Sandbox::new();
    let target = sandbox.target();
    assert_eq!(
        sandbox.open("ghostty", Path::new(&target)),
        (Some(0), Some(target))
    );
}

// An entry with no directory option keeps the launcher's own cd.
#[test]
fn an_entry_without_a_directory_option_opens_in_the_folder() {
    let sandbox = Sandbox::new();
    let target = sandbox.target();
    assert_eq!(
        sandbox.open("foot", Path::new(&target)),
        (Some(0), Some(target))
    );
}

// #373: a file row opens a terminal in the folder that holds it.
#[test]
fn a_file_opens_in_its_own_folder() {
    let sandbox = Sandbox::new();
    let target = sandbox.target();
    for terminal in ["ghostty", "foot"] {
        let file = Path::new(&target).join("notes.txt");
        assert_eq!(
            sandbox.open(terminal, &file),
            (Some(0), Some(target.clone())),
            "{terminal}"
        );
    }
}

// A link row opens where it is listed, whether its target is elsewhere or gone.
#[test]
fn a_link_opens_in_the_folder_that_lists_it() {
    let sandbox = Sandbox::new();
    let target = sandbox.target();
    for link in ["link.txt", "dead"] {
        let path = sandbox.0.join("target dir").join(link);
        assert_eq!(
            sandbox.open("foot", &path),
            (Some(0), Some(target.clone())),
            "{link}"
        );
    }
}

// A launcher that cannot print its command still gets the folder through --dir=.
#[test]
fn a_launcher_without_print_cmd_still_opens_in_the_folder() {
    let sandbox = Sandbox::new();
    let target = sandbox.target();
    assert_eq!(
        sandbox.launch("foot", Path::new(&target), false, &sandbox.0.join("home")),
        (Some(0), Some(target))
    );
}

// A bare file name on the command line is listed in the folder flea runs in.
#[test]
fn a_bare_file_name_opens_in_the_working_folder() {
    let sandbox = Sandbox::new();
    let target = sandbox.target();
    assert_eq!(
        sandbox.launch("foot", Path::new("notes.txt"), true, Path::new(&target)),
        (Some(0), Some(target))
    );
}

#[test]
fn a_missing_folder_opens_nothing() {
    let sandbox = Sandbox::new();
    let missing = sandbox.0.join("gone");
    assert_eq!(sandbox.open("foot", &missing), (Some(2), None));
}
