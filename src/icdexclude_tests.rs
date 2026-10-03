use super::*;
use crate::backend::fifotest::{mkfifo, within};
use crate::backend::testdir::TestDir;
use std::os::unix::ffi::OsStringExt;

const NVIDIA: &str = "{\"ICD\":{\"library_path\":\"libGLX_nvidia.so.0\"}}\n";
const RADEON: &str = "{\"ICD\":{\"library_path\":\"libvulkan_radeon.so\"}}\n";
const INTEL: &str = "{\"ICD\":{\"library_path\":\"libvulkan_intel.so\"}}\n";
const LAVAPIPE: &str = "{\"ICD\":{\"library_path\":\"libvulkan_lvp.so\"}}\n";
const HASVK: &str = "{\"ICD\":{\"library_path\":\"libvulkan_intel_hasvk.so\"}}\n";

fn pci_device(root: &Path, slot: &str, class: u32, vendor: u32) {
    let dir = root.join(slot);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("class"), format!("0x{class:06x}\n")).unwrap();
    std::fs::write(dir.join("vendor"), format!("0x{vendor:04x}\n")).unwrap();
}

// Sample bus: "pci", or "platform" and "virtio" for the cards no PCI vendor describes.
fn drm_card(root: &Path, card: &str, bus: &str, vendor: Option<u32>) {
    let dir = root.join(card).join("device");
    std::fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(format!("../../../bus/{bus}"), dir.join("subsystem")).unwrap();
    if let Some(vendor) = vendor {
        std::fs::write(dir.join("vendor"), format!("0x{vendor:04x}\n")).unwrap();
    }
}

// The reported box: two Radeons, nvidia-utils and vulkan-intel installed, lavapipe and hasvk beside them.
fn amd_box(t: &TestDir) -> (PathBuf, PathBuf, PathBuf) {
    let (pci, drm, icd) = (t.dir("pci"), t.dir("drm"), t.dir("icd"));
    pci_device(&pci, "0000:03:00.0", 0x030000, 0x1002);
    pci_device(&pci, "0000:7c:00.0", 0x030000, 0x1002);
    // An NVIDIA PCI device that is not a display controller owns no GPU.
    pci_device(&pci, "0000:05:00.0", 0x0c0330, 0x10de);
    drm_card(&drm, "card0", "pci", Some(0x1002));
    drm_card(&drm, "card1", "pci", Some(0x1002));
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
    assert_eq!(absent_vendor_manifests(&pci, &drm, &[icd]), vec!["nvidia_icd.json", "intel_icd.x86_64.json"]);
}

// A dGPU with nvidia-drm unloaded owns no DRM card, a 3D controller is still a display controller, and a card counts too.
#[test]
fn a_gpu_the_pci_bus_or_a_drm_card_names_keeps_its_driver() {
    let t = TestDir::new("icdexclude-present");
    let (pci, drm, icd) = amd_box(&t);
    let dirs = [icd];
    pci_device(&pci, "0000:01:00.0", 0x030200, 0x10de);
    assert_eq!(absent_vendor_manifests(&pci, &drm, &dirs), vec!["intel_icd.x86_64.json"]);
    std::fs::remove_dir_all(pci.join("0000:01:00.0")).unwrap();
    drm_card(&drm, "card2", "pci", Some(0x8086));
    assert_eq!(absent_vendor_manifests(&pci, &drm, &dirs), vec!["nvidia_icd.json"]);
}

#[test]
fn a_gpu_this_cannot_name_excludes_nothing() {
    let t = TestDir::new("icdexclude-unknown");
    let (pci, drm, icd) = amd_box(&t);
    let dirs = [icd];
    drm_card(&drm, "card2", "platform", None);
    assert!(absent_vendor_manifests(&pci, &drm, &dirs).is_empty(), "a SoC card");
    std::fs::remove_dir_all(drm.join("card2")).unwrap();
    // virtio-mmio's vendor file holds the transport's id, 0x554d4551 under QEMU, which is no PCI vendor.
    drm_card(&drm, "card2", "virtio", Some(0x554d4551));
    assert!(absent_vendor_manifests(&pci, &drm, &dirs).is_empty(), "a virtio-mmio card");
    std::fs::remove_dir_all(drm.join("card2")).unwrap();
    std::fs::remove_file(pci.join("0000:03:00.0/vendor")).unwrap();
    assert!(absent_vendor_manifests(&pci, &drm, &dirs).is_empty(), "a display controller with no vendor");
    assert!(absent_vendor_manifests(&t.join("no-pci"), &drm, &dirs).is_empty(), "no PCI bus to read");
    let empty = t.dir("pci-empty");
    let no_cards = t.dir("drm-empty");
    assert!(absent_vendor_manifests(&empty, &no_cards, &dirs).is_empty(), "no display controller at all");
}

// The loader compares names case-insensitively in every directory, so a kept driver protects its name.
#[test]
fn a_name_shared_with_a_kept_driver_is_never_disabled() {
    let t = TestDir::new("icdexclude-shared");
    let (pci, drm, icd) = amd_box(&t);
    let user = t.dir("user");
    std::fs::write(user.join("NVIDIA_ICD.json"), LAVAPIPE).unwrap();
    std::fs::create_dir(user.join("intel_icd.x86_64.json")).unwrap();
    assert!(absent_vendor_manifests(&pci, &drm, &[icd, user]).is_empty());
}

// The loader reads ICD.library_path as C strings, so a library_path elsewhere names nothing and a NUL keeps the driver.
#[test]
fn only_the_icd_library_path_names_the_driver() {
    let t = TestDir::new("icdexclude-scope");
    let (pci, drm, _) = amd_box(&t);
    let icd = t.dir("scope");
    std::fs::write(icd.join("amd_icd.json"), "{\"library_path\":\"libGLX_nvidia.so.0\",\"ICD\":{\"library_path\":\"libvulkan_radeon.so\"}}\n").unwrap();
    std::fs::write(icd.join("escaped_icd.json"), "{\"ICD\":{\"library_path\":\"\\/usr\\/lib\\/libGLX_nvidia.so.0\"}}\n").unwrap();
    // Both drive the AMD GPU here: the loader stops at the NUL, in the path and in the first ICD's name.
    std::fs::write(icd.join("nul_value_icd.json"), "{\"ICD\":{\"library_path\":\"libvulkan_radeon.so\\u0000/../libGLX_nvidia.so.0\"}}\n").unwrap();
    std::fs::write(icd.join("nul_key_icd.json"), "{\"ICD\\u0000x\":{\"library_path\":\"libvulkan_radeon.so\"},\"ICD\":{\"library_path\":\"libGLX_nvidia.so.0\"}}\n").unwrap();
    assert_eq!(absent_vendor_manifests(&pci, &drm, &[icd]), vec!["escaped_icd.json"]);
}

#[test]
fn a_driver_named_only_by_its_directory_or_past_the_size_cap_is_kept() {
    let t = TestDir::new("icdexclude-path");
    let (pci, drm, _) = amd_box(&t);
    let icd = t.dir("opt");
    std::fs::write(icd.join("lvp_icd.json"), "{\"ICD\":{\"library_path\":\"/opt/intel/nvidia/libvulkan_lvp.so\"}}\n").unwrap();
    std::fs::write(icd.join("big_icd.json"), format!("{}{}", NVIDIA, " ".repeat(MANIFEST_BYTES))).unwrap();
    assert!(absent_vendor_manifests(&pci, &drm, &[icd]).is_empty());
}

// Only a packaged driver's own library name is known to drive one vendor alone, so a vendor word in any other name
// names nothing, and that unknown driver protects every manifest that shares its file name.
#[test]
fn an_unfamiliar_library_is_kept_whatever_vendor_its_name_mentions() {
    let t = TestDir::new("icdexclude-unfamiliar");
    let (pci, drm, icd) = amd_box(&t);
    let user = t.dir("user");
    std::fs::write(user.join("emu_icd.json"), "{\"ICD\":{\"library_path\":\"libvulkan_intel-emulation.so\"}}\n").unwrap();
    std::fs::write(user.join("nvidia_icd.json"), "{\"ICD\":{\"library_path\":\"/opt/wrap/libGLX_nvidia-wrapper.so.0\"}}\n").unwrap();
    assert_eq!(absent_vendor_manifests(&pci, &drm, &[icd, user]), vec!["intel_icd.x86_64.json"]);
}

#[test]
fn a_name_the_loader_would_read_as_a_glob_is_never_written() {
    let t = TestDir::new("icdexclude-glob");
    let (pci, drm, _) = amd_box(&t);
    let icd = t.dir("odd");
    for name in ["nv*_icd.json", "a,b_icd.json", "nvidia icd.json", "~nv_icd.json"] {
        std::fs::write(icd.join(name), NVIDIA).unwrap();
    }
    assert!(absent_vendor_manifests(&pci, &drm, &[icd]).is_empty());
}

// A fifo is read only when something writes to it, so opening one would hold the launch for good.
#[test]
fn a_fifo_named_like_a_manifest_neither_holds_the_launch_nor_loses_its_name() {
    let t = TestDir::new("icdexclude-fifo");
    let (pci, drm, icd) = amd_box(&t);
    let pipes = t.dir("pipes");
    mkfifo(&pipes.join("nvidia_icd.json"));
    let names = within("a fifo named like a manifest", move || absent_vendor_manifests(&pci, &drm, &[icd, pipes]));
    assert_eq!(names, vec!["intel_icd.x86_64.json"]);
}

// One packaged driver reached through a symlink in /etc and a case variant in the config home is still one absent driver.
#[test]
fn one_absent_driver_seen_through_a_symlink_and_a_case_variant_is_disabled_once() {
    let t = TestDir::new("icdexclude-dup");
    let (pci, drm, icd) = amd_box(&t);
    let (etc, user) = (t.dir("etc"), t.dir("user"));
    std::os::unix::fs::symlink(icd.join("nvidia_icd.json"), etc.join("nvidia_icd.json")).unwrap();
    std::fs::write(user.join("NVIDIA_ICD.json"), NVIDIA).unwrap();
    assert_eq!(absent_vendor_manifests(&pci, &drm, &[user, etc, icd]), vec!["nvidia_icd.json", "intel_icd.x86_64.json"]);
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
fn the_search_covers_every_loader_root_and_keeps_a_path_that_is_not_utf8() {
    let odd = OsString::from_vec(b"/data/\xff".to_vec());
    let mut data_dirs = odd.clone();
    data_dirs.push(":/opt/share");
    let roots = xdg_roots(|name| match name {
        "HOME" => Some(OsString::from("/home/u")),
        "XDG_CONFIG_DIRS" => Some(OsString::from("/cfg/a::/cfg/b")),
        "XDG_DATA_DIRS" => Some(data_dirs.clone()),
        "XDG_DATA_HOME" => Some(OsString::new()),
        _ => None,
    });
    for want in ["/home/u/.config", "/cfg/a", "/cfg/b", "/etc/xdg", "/etc", "/home/u/.local/share", "/opt/share", "/usr/local/share", "/usr/share"] {
        assert!(roots.contains(&PathBuf::from(want)), "{want} missing from {roots:?}");
    }
    assert!(roots.contains(&PathBuf::from(odd)), "the non-UTF-8 data dir");
    assert!(!roots.contains(&PathBuf::new()), "an empty list entry is no root");
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
