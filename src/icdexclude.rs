// A Vulkan driver for a GPU vendor this box does not hold is disabled before the probe and before qs.
// Measured on an AMD-only box with nvidia-utils installed: vkCreateInstance took 0.63 to 1.1 s, most of it
// waiting on two nvidia-modprobe children the NVIDIA ICD starts, against 6 to 8 ms with that manifest disabled.
use crate::backend::regfile;
use crate::vulkan::{icd_library, is_card, parse_hex_id, MANIFEST_BYTES};
use crate::vulkan::{PCI_VENDOR_AMD, PCI_VENDOR_INTEL, PCI_VENDOR_NVIDIA};
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

// The loader's own filter: comma-separated, case-insensitive globs over manifest file names, read at vkCreateInstance.
const DISABLE: &str = "VK_LOADER_DRIVERS_DISABLE";
// Holds the exact filter Flea wrote, so a filter that differs from it is the operator's even when the marker leaked.
pub const MARKER: &str = "FLEA_VK_DISABLE";
// Each of these is the operator choosing drivers, which a launch never second-guesses.
const OPERATOR_CHOICES: [&str; 5] = ["VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES", "VK_LOADER_DRIVERS_SELECT", DISABLE];
// A loader settings file can add drivers or choose them outright, so its presence is an operator choice too.
const SETTINGS_FILE: &str = "vulkan/loader_settings.d/vk_loader_settings.json";
// PCI base class 0x03 is a display controller: VGA, 3D and other display all carry it.
const PCI_CLASS_DISPLAY: u32 = 0x03;
// More manifests than this is not a desktop's driver set, and they are counted before any is opened.
const MAX_MANIFESTS: usize = 256;
// The loader reads only the first 16 entries of its filter (MAX_ADDITIONAL_FILTERS) and drops the rest silently.
const LOADER_FILTERS: usize = 16;
// The library file names the packaged hardware drivers carry, matched whole: any other name, however vendor-like, is a
// driver this cannot name and is kept, because only these are known to drive nothing but their own vendor's GPUs.
// Venus (libvulkan_virtio.so) is not one: its vtest transport runs on whatever GPU or software renderer serves it.
const KNOWN_LIBRARIES: [(&str, u32); 6] = [
    ("libGLX_nvidia.so.0", PCI_VENDOR_NVIDIA),
    ("libvulkan_nouveau.so", PCI_VENDOR_NVIDIA),
    ("libvulkan_radeon.so", PCI_VENDOR_AMD),
    ("amdvlk64.so", PCI_VENDOR_AMD),
    ("amdvlk32.so", PCI_VENDOR_AMD),
    ("libvulkan_intel.so", PCI_VENDOR_INTEL),
];

// Before the probe and before qs, in every arm: the OpenGL retry inherits the shell's filter, and zink draws GL through Vulkan.
// The launcher is single threaded until it execs qs, which is what makes the environment writes here sound.
pub fn exclude_absent_vendors() {
    // A filter its marker names is a parent Flea's, from a new window or the retry, so it is decided again; a stale marker goes.
    if owned(std::env::var_os(DISABLE).as_deref(), std::env::var_os(MARKER).as_deref()) {
        std::env::remove_var(DISABLE);
    }
    std::env::remove_var(MARKER);
    if operator_chose_drivers(|name| std::env::var_os(name)) {
        return;
    }
    let roots = xdg_roots(|name| std::env::var_os(name));
    if roots.iter().any(|root| root.join(SETTINGS_FILE).exists()) {
        return;
    }
    let dirs: Vec<PathBuf> = roots.iter().map(|root| root.join("vulkan/icd.d")).collect();
    let names = absent_vendor_manifests(Path::new("/sys/bus/pci/devices"), Path::new("/sys/class/drm"), &dirs);
    if names.is_empty() {
        return;
    }
    let filter = names.join(",");
    std::env::set_var(DISABLE, &filter);
    std::env::set_var(MARKER, &filter);
}

// Undo this launcher's exclusion for a child, leaving an operator's own filter untouched.
pub fn drop_exclusion(command: &mut Command) {
    let marker = std::env::var_os(MARKER);
    if owned(std::env::var_os(DISABLE).as_deref(), marker.as_deref()) {
        command.env_remove(DISABLE);
    }
    if marker.is_some() {
        command.env_remove(MARKER);
    }
}

// Sample input: filter "nvidia_icd.json" beside marker "nvidia_icd.json" is Flea's; beside "1" or nothing it is the operator's.
fn owned(filter: Option<&OsStr>, marker: Option<&OsStr>) -> bool {
    matches!((filter, marker), (Some(filter), Some(marker)) if !marker.is_empty() && filter == marker)
}

// Empty is absent, the rule QSG_RHI_BACKEND and the pin's loader variables already follow.
fn operator_chose_drivers(var: impl Fn(&str) -> Option<OsString>) -> bool {
    OPERATOR_CHOICES.iter().any(|name| var(name).is_some_and(|value| !value.is_empty()))
}

// Every root the loader searches on Linux, plus the XDG defaults, because a disabled name reaches every directory.
// Raw OsStrings throughout: a list holding one path that is not UTF-8 must not lose the others.
fn xdg_roots(var: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let set = |name: &str| var(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    let list = |name: &str| var(name).map(|value| std::env::split_paths(&value).filter(|dir| !dir.as_os_str().is_empty()).collect::<Vec<_>>()).unwrap_or_default();
    let home = set("HOME");
    let mut roots: Vec<PathBuf> = Vec::new();
    roots.extend(set("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|h| h.join(".config"))));
    roots.extend(list("XDG_CONFIG_DIRS"));
    roots.extend(["/etc/xdg", "/etc"].map(PathBuf::from));
    roots.extend(set("XDG_DATA_HOME").or_else(|| home.as_ref().map(|h| h.join(".local/share"))));
    roots.extend(list("XDG_DATA_DIRS"));
    roots.extend(["/usr/local/share", "/usr/share"].map(PathBuf::from));
    let mut unique: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !unique.contains(&root) {
            unique.push(root);
        }
    }
    unique
}

// Split from exclude_absent_vendors so a test can drive it against a fake sysfs and fake manifest directories.
fn absent_vendor_manifests(pci: &Path, drm: &Path, dirs: &[PathBuf]) -> Vec<String> {
    let Some(present) = present_vendors(pci, drm) else {
        return Vec::new();
    };
    let Some(manifests) = manifests_in(dirs) else {
        return Vec::new();
    };
    excluded(&manifests, &present)
}

// Every display controller's PCI vendor, or None when the box holds a GPU this cannot name, so nothing is excluded.
// Sample input: /sys/bus/pci/devices/0000:03:00.0 with class 0x030000 and vendor 0x1002, and drm card1 on that device.
fn present_vendors(pci: &Path, drm: &Path) -> Option<Vec<u32>> {
    let mut vendors = Vec::new();
    for entry in std::fs::read_dir(pci).ok()? {
        let device = entry.ok()?.path();
        let class = parse_hex_id(&std::fs::read_to_string(device.join("class")).ok()?)?;
        if class >> 16 != PCI_CLASS_DISPLAY {
            continue;
        }
        let vendor = parse_hex_id(&std::fs::read_to_string(device.join("vendor")).ok()?)?;
        if !vendors.contains(&vendor) {
            vendors.push(vendor);
        }
    }
    for entry in std::fs::read_dir(drm).ok()? {
        let entry = entry.ok()?;
        if !is_card(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let device = entry.path().join("device");
        // A card off the PCI bus, a SoC GPU, a virtio-mmio GPU or an evdi dock, has no PCI vendor, and any ICD might drive it.
        if std::fs::read_link(device.join("subsystem")).ok()?.file_name()? != "pci" {
            return None;
        }
        let vendor = parse_hex_id(&std::fs::read_to_string(device.join("vendor")).ok()?)?;
        if !vendors.contains(&vendor) {
            vendors.push(vendor);
        }
    }
    (!vendors.is_empty()).then_some(vendors)
}

// Every manifest in those directories as (file name, vendor of its library), or None when the inventory cannot be had whole.
fn manifests_in(dirs: &[PathBuf]) -> Option<Vec<(String, Option<u32>)>> {
    let mut found = Vec::new();
    for dir in dirs {
        // A directory that is not there, or that the loader could not read either, holds nothing to compare.
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.to_ascii_lowercase().ends_with(".json") {
                continue;
            }
            if found.len() == MAX_MANIFESTS {
                return None;
            }
            found.push((name, entry.path()));
        }
    }
    Some(found.into_iter().map(|(name, path)| (name, manifest_vendor(&path))).collect())
}

// Sample input: the packaged nvidia_icd.json, whose ICD.library_path libGLX_nvidia.so.0 names NVIDIA.
fn manifest_vendor(path: &Path) -> Option<u32> {
    // Only a regular file, and opened non-blocking, so a fifo swapped in after the check cannot hold the launch.
    let file = regfile::open_regular(path)?;
    let mut body = String::new();
    file.take(MANIFEST_BYTES as u64 + 1).read_to_string(&mut body).ok()?;
    // The library's file name, whole, so neither a directory such as /opt/intel nor a name like libvulkan_intel-emulation.so names Intel.
    let library = icd_library(&body)?;
    let name = Path::new(&library).file_name()?.to_str()?;
    KNOWN_LIBRARIES.iter().find(|(known, _)| *known == name).map(|(_, vendor)| *vendor)
}

// A name is disabled only when every manifest carrying it, compared as the loader compares, names a known vendor absent here.
// NVIDIA's come first, because its ICD is the one measured slow, and the loader reads only the first 16.
fn excluded(manifests: &[(String, Option<u32>)], present: &[u32]) -> Vec<String> {
    let absent = |vendor: &Option<u32>| vendor.is_some_and(|id| !present.contains(&id));
    let mut names: Vec<(bool, String)> = manifests
        .iter()
        .filter(|(name, _)| literal_glob(name))
        .filter(|(name, _)| {
            manifests
                .iter()
                .filter(|(other, _)| other.eq_ignore_ascii_case(name))
                .all(|(_, vendor)| absent(vendor))
        })
        .map(|(name, vendor)| (*vendor != Some(PCI_VENDOR_NVIDIA), name.to_ascii_lowercase()))
        .collect();
    // One entry a name, kept NVIDIA's if any namesake is, then NVIDIA's names ahead of the rest.
    names.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    names.dedup_by(|later, earlier| later.1 == earlier.1);
    names.sort();
    names.into_iter().take(LOADER_FILTERS).map(|(_, name)| name).collect()
}

// A comma splits the loader's list and a star or tilde is special in it, so only a name the filter reads literally is written.
fn literal_glob(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}

#[cfg(test)]
#[path = "icdexclude_tests.rs"]
mod tests;
