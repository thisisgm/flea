// The GVFS FUSE root /run/user/<uid>/gvfs and the share folder under it; a folder named gvfs anywhere else is no mount.
use flea::bytename::ExactText as _;
use std::path::{Path, PathBuf};

// The directory whose children are the per-user run folders; a test points it at a sandbox, since /run/user is not writable.
#[cfg(test)]
thread_local! {
    static RUN_USER: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

// Names the sandbox that stands for /run/user on this thread, or clears it with None.
#[cfg(test)]
pub(crate) fn set_test_run_user(root: Option<&Path>) {
    RUN_USER.with(|slot| {
        *slot.borrow_mut() = root.map(|r| format!("{}/", r.exact_text().trim_end_matches('/')))
    });
}

fn run_user_root() -> String {
    #[cfg(test)]
    if let Some(root) = RUN_USER.with(|slot| slot.borrow().clone()) {
        return root;
    }
    "/run/user/".to_string()
}

// Sample input: "/run/user/1000//gvfs/../gvfs/smb-share:x/a" answers "/run/user/1000/gvfs/smb-share:x/a"; the text of a clean path is kept as is.
fn folded(path: &Path) -> std::borrow::Cow<'_, str> {
    let text = path.exact_text();
    if !text.contains("//") && !text.contains("/./") && !text.ends_with("/.") && !text.contains("/../") && !text.ends_with("/..") {
        return text;
    }
    let mut out = PathBuf::new();
    for part in path.components() {
        // A ".." pops one named segment and never climbs above the root.
        match part {
            std::path::Component::ParentDir => {
                if matches!(out.components().next_back(), Some(std::path::Component::Normal(_))) {
                    out.pop();
                }
            }
            other => out.push(other),
        }
    }
    std::borrow::Cow::Owned(out.exact_text().into_owned())
}

// Splits "<run>/<uid>/gvfs/<share...>" into the root through "gvfs/" and the rest; None for any path off the gvfs mount root.
fn gvfs_split(text: &str) -> Option<(String, &str)> {
    let run = run_user_root();
    let rest = text.strip_prefix(run.as_str())?;
    let uid = rest.find('/').filter(|&at| at > 0)?;
    let share = rest[uid + 1..].strip_prefix("gvfs/")?;
    Some((format!("{run}{}gvfs/", &rest[..=uid]), share))
}

// The kernel resolves ".." after a symlink, so a text that names a share keeps it; the lexical fold decides only a text whose segment under gvfs/ is empty, "." or "..", or that is off the mount root.
fn mounted(path: &Path) -> Option<(String, String)> {
    let text = path.exact_text();
    if let Some((root, share)) = gvfs_split(&text) {
        let mut segments = share.split('/');
        let named = !matches!(segments.next(), Some("" | "." | ".."));
        // A ".." straight after the share name leaves it: the share is the FUSE mount's own folder, never a symlink.
        let leaves = segments.find(|segment| !segment.is_empty() && *segment != ".") == Some("..");
        if named && !leaves {
            return Some((root, share.to_string()));
        }
    }
    let text = folded(path);
    gvfs_split(&text).map(|(root, share)| (root, share.to_string()))
}

// Sample input: "/run/user/1000/gvfs/mtp:host=X/" answers Some("phone"); "/home/gm/gvfs/mtp:host=X" answers None, since only the mount root counts.
pub fn gvfs_class(path: &Path) -> Option<&'static str> {
    let (_, share) = mounted(path)?;
    let share = share.to_ascii_lowercase();
    if share.starts_with("mtp:") || share.starts_with("gphoto2:") || share.starts_with("afc:") {
        Some("phone")
    } else {
        Some("network")
    }
}

// Sample input: /run/user/1000/gvfs/smb-share:server=n,share=x/dir gives backend smb-share, so false; SFTP and unknown backends keep their bits.
pub(crate) fn gvfs_modes_are_real(path: &Path) -> Option<bool> {
    let (_, share) = mounted(path)?;
    let backend = share.split(':').next().unwrap_or_default().to_ascii_lowercase();
    Some(!matches!(backend.as_str(), "smb-share" | "smb-server" | "mtp" | "afc" | "gphoto2"
        | "dav" | "davs" | "http" | "https" | "google-drive" | "onedrive"))
}

// Sample input: "/run/user/1000/gvfs/smb-share:server=n,share=x/dir" answers "/run/user/1000/gvfs/smb-share:server=n,share=x".
pub fn gvfs_root(path: &Path) -> Option<PathBuf> {
    let (root, share) = mounted(path)?;
    let name = share.split('/').next().filter(|name| !name.is_empty())?;
    Some(flea::bytename::exact(root + name))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Doubled slashes, dot and dot-dot segments fold away before the gvfs root is matched, so no spelling of a mount path escapes the guard.
    #[test]
    fn an_unnormalized_gvfs_path_is_still_a_gvfs_path() {
        for spelled in ["/run/user/1000//gvfs/smb-share:server=n,share=x/a", "/run/user/1000/./gvfs/smb-share:server=n,share=x/a",
            "/run/user/1000/gvfs//smb-share:server=n,share=x/a", "//run/user/1000/gvfs/smb-share:server=n,share=x/a", "/run/user/1000/gvfs/smb-share:server=n,share=x/.",
            "/run/user/1000/gvfs/../gvfs/smb-share:server=n,share=x/a", "/run/user/1000/x/../gvfs/smb-share:server=n,share=x/a",
            "/../run/user/1000/gvfs/smb-share:server=n,share=x/a", "/run/user/1000/gvfs/smb-share:server=n,share=x/a/b/.."] {
            assert_eq!(gvfs_class(Path::new(spelled)), Some("network"), "{spelled} is a gvfs path");
            assert_eq!(gvfs_root(Path::new(spelled)).as_deref(), Some(Path::new("/run/user/1000/gvfs/smb-share:server=n,share=x")), "{spelled} roots at its share");
        }
        assert_eq!(gvfs_class(Path::new("/home/u//gvfs/smb-share:server=n,share=x/a")), None, "a doubled slash does not turn a plain folder into a mount");
        let out = "/run/user/1000/gvfs/../../../home/u/a";
        assert_eq!((gvfs_class(Path::new(out)), gvfs_root(Path::new(out))), (None, None), "a .. that climbs out of the mount root is not a mount path");
        // A link inside the share may point deeper, so the share a text names stands whatever its .. segments fold to.
        let left = "/run/user/1000/gvfs/smb-share:server=n,share=x/./../mtp:host=X/a";
        assert_eq!(gvfs_class(Path::new(left)), Some("phone"), "{left} left the share for the phone");
        assert_eq!(gvfs_root(Path::new(left)).as_deref(), Some(Path::new("/run/user/1000/gvfs/mtp:host=X")), "{left} roots at the phone");
        let out = "/run/user/1000/gvfs/smb-share:server=n,share=x/../../../home/u";
        assert_eq!((gvfs_class(Path::new(out)), gvfs_root(Path::new(out))), (None, None), "{out} climbed out of the mount root");
        let linked = "/run/user/1000/gvfs/smb-share:server=n,share=x/link/../../../..";
        assert_eq!(gvfs_class(Path::new(linked)), Some("network"), "{linked} names its share");
        assert_eq!(gvfs_root(Path::new(linked)).as_deref(), Some(Path::new("/run/user/1000/gvfs/smb-share:server=n,share=x")), "{linked} roots at its share");
    }
}
