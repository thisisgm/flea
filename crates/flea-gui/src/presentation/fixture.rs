use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

const MARKER: &str = ".flea-test-sandbox";
const OWNER: &str = "flea-u01-owned-fixture\n";

pub(crate) struct Fixture(PathBuf);

impl Fixture {
    pub(crate) fn parent() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap()
            .join(".superpowers/u01")
    }

    pub(crate) fn new(name: &str) -> Self {
        assert!(!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
        let parent = Self::parent();
        let mut dir = parent.parent().unwrap().parent().unwrap().to_path_buf();
        for part in [".superpowers", "u01"] {
            dir.push(part);
            match fs::symlink_metadata(&dir) {
                Ok(meta) => assert!(meta.is_dir() && !meta.file_type().is_symlink()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if let Err(error) = fs::create_dir(&dir) {
                        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
                    }
                    let meta = fs::symlink_metadata(&dir).unwrap();
                    assert!(meta.is_dir() && !meta.file_type().is_symlink());
                }
                Err(error) => panic!("fixture parent: {error}"),
            }
        }
        assert_eq!(parent.canonicalize().unwrap(), parent);
        let output = Command::new("/usr/bin/mktemp")
            .args(["-d", "--"])
            .arg(parent.join(format!("{name}.XXXXXX")))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
        assert!(path.is_absolute() && path.parent() == Some(parent.as_path()));
        assert_eq!(path.canonicalize().unwrap(), path);
        let mut marker = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.join(MARKER))
            .unwrap();
        marker.write_all(OWNER.as_bytes()).unwrap();
        Self::open(path).unwrap()
    }

    pub(crate) fn open(path: PathBuf) -> io::Result<Self> {
        let fixture = Self(path);
        fixture.check(fixture.path())?;
        Ok(fixture)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn check(&self, path: &Path) -> io::Result<()> {
        let refused = || io::Error::from(io::ErrorKind::PermissionDenied);
        let root = self.path();
        let parent = Self::parent();
        if root.as_os_str().is_empty()
            || !root.is_absolute()
            || root == parent
            || !root.starts_with(&parent)
            || root.canonicalize()? != root
            || !path.is_absolute()
            || path.as_os_str().is_empty()
            || !path.starts_with(root)
            || path
                .components()
                .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
        {
            return Err(refused());
        }
        let marker = root.join(MARKER);
        let meta = fs::symlink_metadata(&marker)?;
        if !meta.is_file() || meta.file_type().is_symlink() || fs::read_to_string(marker)? != OWNER
        {
            return Err(refused());
        }
        let mut current = root.to_path_buf();
        for part in path.strip_prefix(root).map_err(|_| refused())?.components() {
            current.push(part);
            match fs::symlink_metadata(&current) {
                Ok(meta) if meta.file_type().is_symlink() => return Err(refused()),
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                    ) =>
                {
                    break
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    pub(crate) fn mkdir(&self, path: &Path) {
        self.check(path).unwrap();
        fs::create_dir_all(path).unwrap();
    }

    pub(crate) fn write(&self, path: &Path, bytes: impl AsRef<[u8]>) {
        self.mkdir(path.parent().unwrap());
        self.check(path).unwrap();
        fs::write(path, bytes).unwrap();
    }

    pub(crate) fn executable(&self, path: &Path) {
        self.check(path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    pub(crate) fn cleanup(self) {
        self.check(self.path()).unwrap();
        fs::remove_dir_all(self.path()).unwrap();
    }
}

pub(crate) const PROBE_OPEN: &str = "late-probe-open";
// The probe's own kill ends a blocked stub; this bound ends one whose test process died first.
const BLOCKED_STUB_SECONDS: u32 = 30;

pub(crate) fn service_after(
    before_watch: impl FnOnce() + Send + 'static,
) -> (super::Service, super::Snapshot) {
    super::Service::start_with(
        super::source::Sources::current(),
        super::service::TICK,
        before_watch,
    )
}

pub(crate) fn gated_probe(script: &str) -> String {
    let body = script.strip_prefix("#!/bin/sh\n").unwrap();
    format!("#!/bin/sh\nroot=\"${{0%/bin/fc-match}}\"\n[ \"$(cat \"$root/.flea-test-sandbox\")\" = flea-u01-owned-fixture ] || exit 91\nif [ ! -f \"$root/{PROBE_OPEN}\" ]; then\nmkfifo \"$root/launch-block\" || exit 92\nexec timeout {BLOCKED_STUB_SECONDS} cat \"$root/launch-block\"\nfi\n{body}")
}

pub(crate) const SHELL_GUARD: &str = r#"guard() {
    [ -n "$1" ] && [ -n "$2" ] && [ -n "$3" ] || exit 91
    case "$1" in "$3"/*) ;; *) exit 92 ;; esac
    [ "$(/usr/bin/realpath -e -- "$1")" = "$1" ] || exit 93
    [ ! -L "$1/.flea-test-sandbox" ] || exit 94
    [ "$(/usr/bin/cat -- "$1/.flea-test-sandbox")" = flea-u01-owned-fixture ] || exit 95
    case "$(/usr/bin/realpath -m -- "$2")" in "$1"/*) ;; *) exit 96 ;; esac
}
"#;
