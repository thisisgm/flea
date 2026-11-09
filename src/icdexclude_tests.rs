use super::manifests::MAX_MANIFESTS;
use super::*;
use crate::backend::testdir::TestDir;

pub(super) const NVIDIA: &str = "{\"ICD\":{\"library_path\":\"libGLX_nvidia.so.0\"}}\n";
const RADEON: &str = "{\"ICD\":{\"library_path\":\"libvulkan_radeon.so\"}}\n";
const INTEL: &str = "{\"ICD\":{\"library_path\":\"libvulkan_intel.so\"}}\n";
pub(super) const LAVAPIPE: &str = "{\"ICD\":{\"library_path\":\"libvulkan_lvp.so\"}}\n";
const HASVK: &str = "{\"ICD\":{\"library_path\":\"libvulkan_intel_hasvk.so\"}}\n";

fn pci_device(root: &Path, slot: &str, class: u32, vendor: u32, device: u32) {
    let dir = root.join(slot);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("class"), format!("0x{class:06x}\n")).unwrap();
    std::fs::write(dir.join("vendor"), format!("0x{vendor:04x}\n")).unwrap();
    std::fs::write(dir.join("device"), format!("0x{device:04x}\n")).unwrap();
}

// Sample bus: "pci", or "platform" and "virtio" for the cards no PCI vendor describes.
fn drm_card(root: &Path, card: &str, bus: &str, vendor: Option<u32>, device: Option<u32>) {
    let dir = root.join(card).join("device");
    std::fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(format!("../../../bus/{bus}"), dir.join("subsystem")).unwrap();
    if let Some(vendor) = vendor {
        std::fs::write(dir.join("vendor"), format!("0x{vendor:04x}\n")).unwrap();
    }
    if let Some(device) = device {
        std::fs::write(dir.join("device"), format!("0x{device:04x}\n")).unwrap();
    }
}

// The reported box: two Radeons, nvidia-utils and vulkan-intel installed, lavapipe and hasvk beside them.
pub(super) fn amd_box(t: &TestDir) -> (PathBuf, PathBuf, PathBuf) {
    let (pci, drm, icd) = (t.dir("pci"), t.dir("drm"), t.dir("icd"));
    pci_device(&pci, "0000:03:00.0", 0x030000, 0x1002, 0x744c);
    pci_device(&pci, "0000:7c:00.0", 0x030000, 0x1002, 0x13c0);
    // An NVIDIA PCI device that is not a display controller owns no GPU.
    pci_device(&pci, "0000:05:00.0", 0x0c0330, 0x10de, 0x1c82);
    drm_card(&drm, "card0", "pci", Some(0x1002), Some(0x744c));
    drm_card(&drm, "card1", "pci", Some(0x1002), Some(0x13c0));
    for (name, body) in [("nvidia_icd.json", NVIDIA), ("radeon_icd.x86_64.json", RADEON), ("intel_icd.x86_64.json", INTEL), ("lvp_icd.x86_64.json", LAVAPIPE), ("intel_hasvk_icd.x86_64.json", HASVK)] {
        std::fs::write(icd.join(name), body).unwrap();
    }
    (pci, drm, icd)
}

// Venus reaches a GPU through vtest as well as through a virtio GPU, so it is kept on a box holding no virtio device.
#[test]
fn only_a_known_vendor_with_no_gpu_here_is_disabled_and_nvidia_comes_first() {
    let t = TestDir::new("icdexclude-amd");
    let (pci, drm, icd) = amd_box(&t);
    std::fs::write(icd.join("virtio_icd.x86_64.json"), "{\"ICD\":{\"library_path\":\"/usr/lib/libvulkan_virtio.so\"}}\n").unwrap();
    assert_eq!(absent_vendor_manifests(&pci, &drm, &[icd]), vec!["nvidia_icd.json", "intel_hasvk_icd.x86_64.json", "intel_icd.x86_64.json"]);
}

// A dGPU with nvidia-drm unloaded owns no DRM card, a 3D controller is still a display controller, and a card counts too.
#[test]
fn a_gpu_the_pci_bus_or_a_drm_card_names_keeps_its_driver() {
    let t = TestDir::new("icdexclude-present");
    let (pci, drm, icd) = amd_box(&t);
    let dirs = [icd];
    pci_device(&pci, "0000:01:00.0", 0x030200, 0x10de, 0x27e0);
    assert_eq!(absent_vendor_manifests(&pci, &drm, &dirs), vec!["intel_hasvk_icd.x86_64.json", "intel_icd.x86_64.json"]);
    std::fs::remove_dir_all(pci.join("0000:01:00.0")).unwrap();
    drm_card(&drm, "card2", "pci", Some(0x8086), Some(0xa788));
    assert_eq!(absent_vendor_manifests(&pci, &drm, &dirs), vec!["nvidia_icd.json", "intel_hasvk_icd.x86_64.json"]);
}

// hasvk drives Gen7 and Gen8 Intel only, so a Raptor Lake iGPU keeps ANV and loses hasvk.
#[test]
fn a_modern_intel_gpu_excludes_hasvk_and_keeps_anv() {
    let t = TestDir::new("icdexclude-hasvk-modern");
    let (pci, drm, icd) = (t.dir("pci"), t.dir("drm"), t.dir("icd"));
    pci_device(&pci, "0000:00:02.0", 0x030000, 0x8086, 0xa788);
    drm_card(&drm, "card0", "pci", Some(0x8086), Some(0xa788));
    std::fs::write(icd.join("intel_icd.x86_64.json"), INTEL).unwrap();
    std::fs::write(icd.join("intel_hasvk_icd.x86_64.json"), HASVK).unwrap();
    assert_eq!(absent_vendor_manifests(&pci, &drm, &[icd]), vec!["intel_hasvk_icd.x86_64.json"]);
}

// A Haswell GPU is hasvk-class, so its box keeps the hasvk driver for the probe and the pin to decide on.
#[test]
fn a_hasvk_class_gpu_keeps_hasvk() {
    let t = TestDir::new("icdexclude-hasvk-old");
    let (pci, drm, icd) = (t.dir("pci"), t.dir("drm"), t.dir("icd"));
    pci_device(&pci, "0000:00:02.0", 0x030000, 0x8086, 0x0412);
    drm_card(&drm, "card0", "pci", Some(0x8086), Some(0x0412));
    std::fs::write(icd.join("intel_icd.x86_64.json"), INTEL).unwrap();
    std::fs::write(icd.join("intel_hasvk_icd.x86_64.json"), HASVK).unwrap();
    assert!(absent_vendor_manifests(&pci, &drm, &[icd]).is_empty(), "a Haswell box");
}

// One hasvk-class GPU beside a modern one still keeps hasvk.
#[test]
fn a_hasvk_class_gpu_beside_a_modern_one_keeps_hasvk() {
    let t = TestDir::new("icdexclude-hasvk-mixed");
    let (pci, drm, icd) = (t.dir("pci"), t.dir("drm"), t.dir("icd"));
    pci_device(&pci, "0000:00:02.0", 0x030000, 0x8086, 0x0412);
    pci_device(&pci, "0000:00:03.0", 0x030000, 0x8086, 0xa788);
    drm_card(&drm, "card0", "pci", Some(0x8086), Some(0x0412));
    drm_card(&drm, "card1", "pci", Some(0x8086), Some(0xa788));
    std::fs::write(icd.join("intel_icd.x86_64.json"), INTEL).unwrap();
    std::fs::write(icd.join("intel_hasvk_icd.x86_64.json"), HASVK).unwrap();
    assert!(absent_vendor_manifests(&pci, &drm, &[icd]).is_empty(), "a Haswell beside a Raptor Lake");
}

// A device id that cannot be read decides nothing, exactly as an unreadable vendor does.
#[test]
fn an_unreadable_device_id_excludes_nothing() {
    let t = TestDir::new("icdexclude-hasvk-nodevice");
    let (pci, drm, icd) = (t.dir("pci"), t.dir("drm"), t.dir("icd"));
    pci_device(&pci, "0000:00:02.0", 0x030000, 0x8086, 0xa788);
    drm_card(&drm, "card0", "pci", Some(0x8086), Some(0xa788));
    std::fs::write(icd.join("intel_icd.x86_64.json"), INTEL).unwrap();
    std::fs::write(icd.join("intel_hasvk_icd.x86_64.json"), HASVK).unwrap();
    std::fs::remove_file(pci.join("0000:00:02.0/device")).unwrap();
    assert!(absent_vendor_manifests(&pci, &drm, std::slice::from_ref(&icd)).is_empty(), "a display controller with no device id");
    std::fs::write(pci.join("0000:00:02.0/device"), "0xa788\n").unwrap();
    std::fs::remove_file(drm.join("card0/device/device")).unwrap();
    assert!(absent_vendor_manifests(&pci, &drm, &[icd]).is_empty(), "a DRM card with no device id");
}

#[test]
fn a_gpu_this_cannot_name_excludes_nothing() {
    let t = TestDir::new("icdexclude-unknown");
    let (pci, drm, icd) = amd_box(&t);
    let dirs = [icd];
    drm_card(&drm, "card2", "platform", None, None);
    assert!(absent_vendor_manifests(&pci, &drm, &dirs).is_empty(), "a SoC card");
    std::fs::remove_dir_all(drm.join("card2")).unwrap();
    // virtio-mmio's vendor file holds the transport's id, 0x554d4551 under QEMU, which is no PCI vendor.
    drm_card(&drm, "card2", "virtio", Some(0x554d4551), None);
    assert!(absent_vendor_manifests(&pci, &drm, &dirs).is_empty(), "a virtio-mmio card");
    std::fs::remove_dir_all(drm.join("card2")).unwrap();
    std::fs::remove_file(pci.join("0000:03:00.0/vendor")).unwrap();
    assert!(absent_vendor_manifests(&pci, &drm, &dirs).is_empty(), "a display controller with no vendor");
    assert!(absent_vendor_manifests(&t.join("no-pci"), &drm, &dirs).is_empty(), "no PCI bus to read");
    let empty = t.dir("pci-empty");
    let no_cards = t.dir("drm-empty");
    assert!(absent_vendor_manifests(&empty, &no_cards, &dirs).is_empty(), "no display controller at all");
}

// Past 16 the loader would drop NVIDIA's name unread, and past 256 manifests the launch does not read on.
#[test]
fn the_filter_fits_the_loader_and_a_flood_of_manifests_excludes_nothing() {
    let t = TestDir::new("icdexclude-many");
    let (pci, drm, icd) = amd_box(&t);
    for n in 0..20 {
        std::fs::write(icd.join(format!("intel{n:02}_icd.json")), INTEL).unwrap();
    }
    let names = absent_vendor_manifests(&pci, &drm, std::slice::from_ref(&icd));
    assert_eq!(names.len(), 16, "the loader's MAX_ADDITIONAL_FILTERS, whatever the constant here says");
    assert_eq!(names[0], "nvidia_icd.json");
    for n in 20..MAX_MANIFESTS {
        std::fs::write(icd.join(format!("intel{n:03}_icd.json")), INTEL).unwrap();
    }
    assert!(absent_vendor_manifests(&pci, &drm, &[icd]).is_empty());
}

#[test]
fn only_a_filter_its_marker_names_is_flea_s() {
    fn f(s: &str) -> Option<&OsStr> {
        Some(OsStr::new(s))
    }
    assert!(owned(f("nvidia_icd.json"), f("nvidia_icd.json")));
    assert!(!owned(f("operator_icd.json"), f("nvidia_icd.json")), "a filter changed after the marker leaked");
    assert!(!owned(f("nvidia_icd.json"), f("1")));
    assert!(!owned(f(""), f("")));
    assert!(!owned(f("nvidia_icd.json"), None));
}

#[test]
fn any_operator_driver_variable_is_a_choice_and_an_empty_one_is_not() {
    for name in ["VK_DRIVER_FILES", "VK_ICD_FILENAMES", "VK_ADD_DRIVER_FILES", "VK_LOADER_DRIVERS_SELECT", "VK_LOADER_DRIVERS_DISABLE"] {
        assert!(operator_chose_drivers(|asked| (asked == name).then(|| OsString::from("x.json"))), "{name}");
        assert!(!operator_chose_drivers(|asked| (asked == name).then(OsString::new)), "{name} empty");
    }
    assert!(!operator_chose_drivers(|_| None));
}

// Extra operator variables for one decide call, as owned pairs the lookup reads.
fn decide_pairs(extra: &[(&str, &str)]) -> Vec<(String, String)> {
    extra.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect()
}

// One absent-vendor manifest under a temp data home, decided against a fake AMD sysfs.
fn decide_setup(tag: &str, extra: &[(&str, &str)]) -> (TestDir, Vec<(String, String)>, PathBuf, PathBuf, Vec<PathBuf>) {
    let t = TestDir::new(tag);
    let (pci, drm, _) = amd_box(&t);
    let (config, data) = (t.dir("config"), t.dir("data"));
    std::fs::create_dir_all(data.join("vulkan/icd.d")).unwrap();
    std::fs::write(data.join("vulkan/icd.d").join("probe_nvidia_icd.json"), NVIDIA).unwrap();
    (t, decide_pairs(extra), pci, drm, vec![config, data])
}

fn decide_answer(pairs: &[(String, String)], pci: &Path, drm: &Path, roots: &[PathBuf]) -> Option<String> {
    decide_exclusion(|name| pairs.iter().find(|(key, _)| key == name).map(|(_, value)| OsString::from(value.as_str())), pci, drm, roots).0
}

// The positive case: the planted NVIDIA manifest is excluded on the fake AMD box.
#[test]
fn decide_writes_a_filter_for_an_absent_vendor() {
    let (_t, pairs, pci, drm, roots) = decide_setup("icdexclude-decide", &[]);
    let answer = decide_answer(&pairs, &pci, &drm, &roots);
    assert!(answer.as_ref().is_some_and(|filter| filter.contains("probe_nvidia_icd.json")), "{answer:?}");
}

// An operator driver variable wins over the same planted manifest.
#[test]
fn decide_treats_an_operator_driver_variable_as_a_choice() {
    let (_t, pairs, pci, drm, roots) = decide_setup("icdexclude-decide-operator", &[("VK_DRIVER_FILES", "/tmp/flea-operator-driver.json")]);
    let answer = decide_answer(&pairs, &pci, &drm, &roots);
    assert_eq!(answer, None, "an operator choice excludes nothing");
}

// A loader settings file wins over the same planted manifest.
#[test]
fn decide_treats_a_loader_settings_file_as_a_choice() {
    let (t, pairs, pci, drm, roots) = decide_setup("icdexclude-decide-settings", &[]);
    std::fs::create_dir_all(t.join("config/vulkan/loader_settings.d")).unwrap();
    std::fs::write(t.join("config/vulkan/loader_settings.d/vk_loader_settings.json"), "{}\n").unwrap();
    let answer = decide_answer(&pairs, &pci, &drm, &roots);
    assert_eq!(answer, None, "a loader settings file excludes nothing");
}

// A filter its marker names is a parent Flea's, so the planted manifest is still excluded.
#[test]
fn decide_redecides_a_filter_its_marker_names() {
    let (_t, pairs, pci, drm, roots) = decide_setup("icdexclude-decide-stale", &[("VK_LOADER_DRIVERS_DISABLE", "stale_icd.json"), ("FLEA_VK_DISABLE", "stale_icd.json")]);
    let answer = decide_answer(&pairs, &pci, &drm, &roots);
    assert!(answer.as_ref().is_some_and(|filter| filter.contains("probe_nvidia_icd.json")), "{answer:?}");
}
