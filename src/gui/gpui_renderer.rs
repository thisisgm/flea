use crate::backend::regfile;
use crate::vulkan;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn configure(cmd: &mut Command) {
    clear_qt_renderer(cmd);
    cmd.env_remove(vulkan::PIN_MARKER);
    crate::icdexclude::exclude_absent_vendors();
    // An inherited Flea pin is dropped and chosen again; an operator's own list stands untouched.
    if std::env::var_os(vulkan::PIN_MARKER).is_some_and(|value| !value.is_empty()) {
        vulkan::drop_display_pin(cmd);
    } else if ["VK_DRIVER_FILES", "VK_ICD_FILENAMES"]
        .iter()
        .any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()))
    {
        return;
    }
    let devices = crate::vkprobecache::usable();
    if let Ok(devices) = &devices {
        super::apply_display_pin(cmd, vulkan::display_pin(devices));
    }
    let hardware = devices
        .as_ref()
        .is_ok_and(|devices| devices.iter().any(|device| device.2 != 4));
    let pinned = cmd
        .get_envs()
        .find(|(name, _)| *name == OsStr::new("VK_DRIVER_FILES"))
        .and_then(|(_, value)| value.map(OsStr::to_os_string));
    pin_software(cmd, pinned.as_deref(), hardware);
}

fn pin_software(cmd: &mut Command, pinned: Option<&OsStr>, hardware: bool) {
    if hardware && pinned.is_none() {
        return;
    }
    if let Some(list) = fallback_list(pinned, hardware, lavapipe(&icd_dirs()).as_deref()) {
        cmd.env("VK_DRIVER_FILES", &list);
        cmd.env("VK_ICD_FILENAMES", &list);
        cmd.env(vulkan::PIN_MARKER, "1");
    }
}

fn clear_qt_renderer(cmd: &mut Command) {
    for key in [
        "QSG_RHI_BACKEND",
        "FLEA_RENDERER_AUTOMATIC",
        "QT_VK_PHYSICAL_DEVICE_INDEX",
    ] {
        cmd.env_remove(key);
    }
}

fn fallback_list(
    pinned: Option<&OsStr>,
    hardware: bool,
    software: Option<&Path>,
) -> Option<OsString> {
    let software = software?;
    if hardware && pinned.is_none() {
        return None;
    }
    let mut paths: Vec<PathBuf> = pinned
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect();
    if !paths.iter().any(|path| path == software) {
        paths.push(software.to_owned());
    }
    std::env::join_paths(paths).ok()
}

fn icd_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(config) = crate::userfile::env_dir("XDG_CONFIG_HOME") {
        dirs.push(config.join("vulkan/icd.d"));
    } else if let Some(home) = crate::userfile::env_dir("HOME") {
        dirs.push(home.join(".config/vulkan/icd.d"));
    }
    for key in ["XDG_CONFIG_DIRS", "XDG_DATA_DIRS"] {
        if let Some(value) = std::env::var_os(key) {
            dirs.extend(
                std::env::split_paths(&value)
                    .filter(|p| p.is_absolute())
                    .map(|p| p.join("vulkan/icd.d")),
            );
        }
    }
    if let Some(data) = crate::userfile::env_dir("XDG_DATA_HOME") {
        dirs.push(data.join("vulkan/icd.d"));
    }
    for dir in ["/etc/xdg", "/etc", "/usr/local/share", "/usr/share"] {
        dirs.push(Path::new(dir).join("vulkan/icd.d"));
    }
    dirs
}

fn lavapipe(dirs: &[PathBuf]) -> Option<PathBuf> {
    let mut seen = 0;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries {
            seen += 1;
            if seen > 256 {
                return None;
            }
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.extension() != Some(OsStr::new("json")) {
                continue;
            }
            let Some(file) = regfile::open_regular(&path) else {
                continue;
            };
            let mut body = String::new();
            if file
                .take(vulkan::MANIFEST_BYTES as u64 + 1)
                .read_to_string(&mut body)
                .is_err()
            {
                continue;
            }
            let Some(library) = vulkan::icd_library(&body) else {
                continue;
            };
            if Path::new(&library).file_name() == Some(OsStr::new("libvulkan_lvp.so")) {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn hardware_pin_keeps_software_available_before_the_forks_gl_fallback() {
        let software = Path::new("/drivers/lvp.json");
        assert_eq!(
            fallback_list(Some(OsStr::new("/drivers/gpu.json")), true, Some(software)),
            Some("/drivers/gpu.json:/drivers/lvp.json".into())
        );
        assert_eq!(
            fallback_list(None, false, Some(software)),
            Some(software.as_os_str().to_owned())
        );
        assert_eq!(fallback_list(None, true, Some(software)), None);
        assert_eq!(fallback_list(None, false, None), None);
        assert_eq!(
            fallback_list(Some(software.as_os_str()), true, Some(software)),
            Some(software.as_os_str().to_owned())
        );
    }

    #[test]
    fn gpui_removes_qt_renderer_choices_and_retry_markers() {
        let mut cmd = Command::new("flea-gui");
        clear_qt_renderer(&mut cmd);
        for key in [
            "QSG_RHI_BACKEND",
            "FLEA_RENDERER_AUTOMATIC",
            "QT_VK_PHYSICAL_DEVICE_INDEX",
        ] {
            assert!(cmd
                .get_envs()
                .any(|(name, value)| name == key && value.is_none()));
        }
    }

    #[test]
    fn only_a_regular_bounded_manifest_naming_the_lavapipe_library_is_pinned() {
        let root = TestDir::new("gpui-lavapipe");
        let dir = root.dir("icd");
        root.file(
            "icd/wrong.json",
            r#"{"ICD":{"library_path":"libvulkan_fake_lvp.so"}}"#,
        );
        root.file(
            "icd/decoy.json",
            r#"{"ICD":{"library_path":"libvulkan_lvp.so","library_path":"other.so"}}"#,
        );
        root.file("icd/large.json", &" ".repeat(vulkan::MANIFEST_BYTES + 1));
        assert!(lavapipe(std::slice::from_ref(&dir)).is_none());
        let path = root.file(
            "icd/lvp.json",
            r#"{"ICD":{"library_path":"/usr/lib/libvulkan_lvp.so"}}"#,
        );
        assert_eq!(lavapipe(&[dir]), Some(path));
    }
}
