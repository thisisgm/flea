// A Vulkan driver for a GPU vendor this box does not hold is disabled before the probe and before qs.
use crate::vulkan::{is_card, is_hasvk_intel, is_render_node, parse_hex_id, PCI_VENDOR_NVIDIA};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

// The manifest inventory lives in its own file, read here and by the probe cache's key.
#[path = "icdmanifests.rs"]
pub(crate) mod manifests;
use manifests::{manifests_in, Manifest};
// Whether no NVIDIA card drives a display while another card does, so an NVIDIA driver would only wake its GPU.
#[path = "icdidle.rs"]
mod idle;

// The loader's own filter: comma-separated, case-insensitive globs over manifest file names, read at vkCreateInstance.
const DISABLE: &str = "VK_LOADER_DRIVERS_DISABLE";
// Holds the exact filter Flea wrote, so a filter that differs from it is the operator's even when the marker leaked.
pub const MARKER: &str = "FLEA_VK_DISABLE";
// Each of these is the operator choosing drivers, which a launch never second-guesses.
const OPERATOR_CHOICES: [&str; 5] = ["VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES", "VK_LOADER_DRIVERS_SELECT", DISABLE];
// A loader settings file can add drivers or choose them outright, so its presence is an operator choice too.
pub(crate) const SETTINGS_FILE: &str = "vulkan/loader_settings.d/vk_loader_settings.json";
// PCI base class 0x03 is a display controller: VGA, 3D and other display all carry it.
pub(crate) const PCI_CLASS_DISPLAY: u32 = 0x03;
// A PCI class code carries its base class in bits 16 to 23.
pub(crate) const PCI_CLASS_SHIFT: u32 = 16;
// The loader reads only the first 16 entries of its filter (MAX_ADDITIONAL_FILTERS) and drops the rest silently.
const LOADER_FILTERS: usize = 16;

// Before the probe and qs in every arm, single threaded until exec so these writes are sound; true when it disabled an idle NVIDIA GPU's drivers.
pub fn exclude_absent_vendors() -> bool {
    let roots = xdg_roots(|name| std::env::var_os(name));
    let (filter, idle) = decide_exclusion(|name| std::env::var_os(name), Path::new("/sys/bus/pci/devices"), Path::new("/sys/class/drm"), &roots);
    // A filter its marker names is a parent Flea's, so it goes with its marker before this launch writes its own.
    if owned(std::env::var_os(DISABLE).as_deref(), std::env::var_os(MARKER).as_deref()) {
        std::env::remove_var(DISABLE);
    }
    // A launch with nothing to remove writes nothing: a setenv racing another thread's getenv is the hazard.
    if std::env::var_os(MARKER).is_some() {
        std::env::remove_var(MARKER);
    }
    if let Some(filter) = filter {
        std::env::set_var(DISABLE, &filter);
        std::env::set_var(MARKER, &filter);
    }
    idle
}

// The filter to write, or None, and whether the idle NVIDIA rule added to it: a stale marker is re-decided, an operator choice or settings file wins.
pub(crate) fn decide_exclusion(var: impl Fn(&str) -> Option<OsString>, pci: &Path, drm: &Path, roots: &[PathBuf]) -> (Option<String>, bool) {
    let stale = owned(var(DISABLE).as_deref(), var(MARKER).as_deref());
    if operator_chose_drivers(|name| if stale && name == DISABLE { None } else { var(name) }) {
        return (None, false);
    }
    if roots.iter().any(|root| root.join(SETTINGS_FILE).exists()) {
        return (None, false);
    }
    let dirs: Vec<PathBuf> = roots.iter().map(|root| root.join("vulkan/icd.d")).collect();
    let (names, idle) = unused_manifests(pci, drm, &dirs, idle::nvidia_idle(&var, drm));
    ((!names.is_empty()).then(|| names.join(",")), idle)
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

// Sample input: XDG_DATA_DIRS="/usr/share:/usr/local/share" gives one root per entry, empties dropped.
// Every root the loader searches, in raw OsStrings so one non-UTF-8 path loses nothing.
pub(crate) fn xdg_roots(var: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
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

// The absent-vendor rule alone, so a test can drive it against a fake sysfs and fake manifest directories.
#[cfg(test)]
fn absent_vendor_manifests(pci: &Path, drm: &Path, dirs: &[PathBuf]) -> Vec<String> {
    unused_manifests(pci, drm, dirs, false).0
}

// The absent vendors' manifests, and NVIDIA's too when its GPU is here but idle, with whether that idle rule applied.
fn unused_manifests(pci: &Path, drm: &Path, dirs: &[PathBuf], nvidia_idle: bool) -> (Vec<String>, bool) {
    let Some(mut present) = present_devices(pci, drm) else {
        return (Vec::new(), false);
    };
    let Some(manifests) = manifests_in(dirs) else {
        return (Vec::new(), false);
    };
    let nvidia_here = present.iter().any(|(vendor, _)| *vendor == PCI_VENDOR_NVIDIA);
    let idle = nvidia_idle && nvidia_here && other_driver_here(&manifests, &present);
    if idle {
        present.retain(|(vendor, _)| *vendor != PCI_VENDOR_NVIDIA);
    }
    (excluded(&manifests, &present), idle)
}

// A present non-NVIDIA GPU has a hardware driver of its own, so disabling NVIDIA's never leaves GPUI only Lavapipe.
fn other_driver_here(manifests: &[Manifest], present: &[(u32, u32)]) -> bool {
    let drives = |gpu: (u32, u32), entry: &Manifest| if is_hasvk_intel(gpu) { entry.2 } else { entry.1 == Some(gpu.0) };
    present
        .iter()
        .filter(|(vendor, _)| *vendor != PCI_VENDOR_NVIDIA)
        .any(|gpu| manifests.iter().any(|entry| drives(*gpu, entry)))
}

// Every display device as (vendor, device), or None when the box holds a GPU this cannot name, so nothing is excluded.
// Sample input: /sys/bus/pci/devices/0000:03:00.0 with class 0x030000, vendor 0x1002 and device 0x744c, and drm card1 on that device.
fn present_devices(pci: &Path, drm: &Path) -> Option<Vec<(u32, u32)>> {
    let mut devices = Vec::new();
    for entry in std::fs::read_dir(pci).ok()? {
        let device = entry.ok()?.path();
        let class = parse_hex_id(&std::fs::read_to_string(device.join("class")).ok()?)?;
        if class >> PCI_CLASS_SHIFT != PCI_CLASS_DISPLAY {
            continue;
        }
        let vendor = parse_hex_id(&std::fs::read_to_string(device.join("vendor")).ok()?)?;
        // An unreadable device id names no class, so it decides nothing, exactly as an unreadable vendor does.
        let id = parse_hex_id(&std::fs::read_to_string(device.join("device")).ok()?)?;
        if !devices.contains(&(vendor, id)) {
            devices.push((vendor, id));
        }
    }
    for entry in std::fs::read_dir(drm).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !is_card(&name) && !is_render_node(&name) {
            continue;
        }
        let device = entry.path().join("device");
        // A card off the PCI bus, a SoC GPU, a virtio-mmio GPU or an evdi dock, has no PCI vendor, and any ICD might drive it.
        if std::fs::read_link(device.join("subsystem")).ok()?.file_name()? != "pci" {
            return None;
        }
        let vendor = parse_hex_id(&std::fs::read_to_string(device.join("vendor")).ok()?)?;
        let id = parse_hex_id(&std::fs::read_to_string(device.join("device")).ok()?)?;
        if !devices.contains(&(vendor, id)) {
            devices.push((vendor, id));
        }
    }
    (!devices.is_empty()).then_some(devices)
}

// Disabled only when every case-insensitive namesake names an absent vendor or a hasvk with no hasvk-class GPU here, NVIDIA's names first.
fn excluded(manifests: &[Manifest], present: &[(u32, u32)]) -> Vec<String> {
    let absent = |vendor: &Option<u32>| vendor.is_some_and(|id| !present.iter().any(|(held, _)| *held == id));
    let hasvk_class_here = present.iter().any(|id| is_hasvk_intel(*id));
    let useless = |entry: &Manifest| absent(&entry.1) || (entry.2 && !hasvk_class_here);
    let mut names: Vec<(bool, String)> = manifests
        .iter()
        .filter(|(name, _, _)| literal_glob(name))
        .filter(|(name, _, _)| {
            manifests
                .iter()
                .filter(|(other, _, _)| other.eq_ignore_ascii_case(name))
                .all(useless)
        })
        .map(|(name, vendor, _)| (*vendor != Some(PCI_VENDOR_NVIDIA), name.to_ascii_lowercase()))
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

#[cfg(test)]
#[path = "icdexclude_manifests_tests.rs"]
mod manifest_tests;
