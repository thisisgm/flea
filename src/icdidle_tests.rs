use super::*;
use crate::backend::testdir::TestDir;
use std::path::PathBuf;

const INTEL: u32 = 0x8086;
const AMD: u32 = 0x1002;

// Sample: card0 with vendor 0x8086 and its connector card0-eDP-1 reading "connected".
fn card(drm: &Path, name: &str, vendor: Option<u32>, connectors: &[(&str, &str)]) {
    let device = drm.join(name).join("device");
    std::fs::create_dir_all(&device).unwrap();
    if let Some(vendor) = vendor {
        std::fs::write(device.join("vendor"), format!("0x{vendor:04x}\n")).unwrap();
    }
    for (connector, status) in connectors {
        let dir = drm.join(format!("{name}-{connector}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("status"), format!("{status}\n")).unwrap();
    }
}

// The reported laptop: the panel on the Intel card, the NVIDIA card's HDMI port empty.
fn hybrid(t: &TestDir) -> PathBuf {
    let drm = t.dir("drm");
    card(&drm, "card1", Some(INTEL), &[("eDP-1", "connected"), ("DP-1", "disconnected")]);
    card(&drm, "card0", Some(PCI_VENDOR_NVIDIA), &[("HDMI-A-1", "disconnected")]);
    drm
}

fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
    move |name| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| OsString::from(value))
}

#[test]
fn an_nvidia_card_with_no_lit_connector_beside_a_lit_panel_is_idle() {
    let t = TestDir::new("icdidle-hybrid");
    assert!(nvidia_idle(env(&[]), &hybrid(&t)));
    // AMD lighting the display is the same case.
    let t = TestDir::new("icdidle-amd");
    let drm = t.dir("drm");
    card(&drm, "card0", Some(AMD), &[("DP-1", "connected")]);
    card(&drm, "card1", Some(PCI_VENDOR_NVIDIA), &[]);
    assert!(nvidia_idle(env(&[]), &drm));
}

#[test]
fn an_nvidia_card_lighting_any_display_is_not_idle() {
    let t = TestDir::new("icdidle-external");
    let drm = hybrid(&t);
    std::fs::write(drm.join("card0-HDMI-A-1/status"), "connected\n").unwrap();
    assert!(!nvidia_idle(env(&[]), &drm));
}

#[test]
fn no_lit_connector_anywhere_decides_nothing() {
    let t = TestDir::new("icdidle-dark");
    let drm = t.dir("drm");
    card(&drm, "card1", Some(INTEL), &[("eDP-1", "unknown")]);
    card(&drm, "card0", Some(PCI_VENDOR_NVIDIA), &[]);
    assert!(!nvidia_idle(env(&[]), &drm));
}

#[test]
fn a_lit_connector_on_an_unreadable_card_decides_nothing() {
    let t = TestDir::new("icdidle-unreadable");
    let drm = hybrid(&t);
    card(&drm, "card2", None, &[("DP-2", "connected")]);
    assert!(!nvidia_idle(env(&[]), &drm));
}

#[test]
fn the_operator_keep_switch_and_prime_offload_keep_nvidia() {
    let t = TestDir::new("icdidle-operator");
    let drm = hybrid(&t);
    assert!(!nvidia_idle(env(&[("FLEA_VK_KEEP_NVIDIA", "1")]), &drm));
    assert!(nvidia_idle(env(&[("FLEA_VK_KEEP_NVIDIA", "0")]), &drm));
    let offload = [("__NV_PRIME_RENDER_OFFLOAD", "1"), ("__VK_LAYER_NV_optimus", "NVIDIA_only"), ("DRI_PRIME", "1"), ("DRI_PRIME", "pci-0000_01_00_0")];
    for (name, value) in offload {
        let var = move |key: &str| (key == name).then(|| OsString::from(value));
        assert!(!nvidia_idle(var, &drm), "{name}={value}");
    }
    // These ask for the default GPU, or for nothing, so the dGPU stays idle.
    let default = [("__NV_PRIME_RENDER_OFFLOAD", "0"), ("__VK_LAYER_NV_optimus", "non_NVIDIA_only"), ("DRI_PRIME", "0"), ("DRI_PRIME", "")];
    for (name, value) in default {
        let var = move |key: &str| (key == name).then(|| OsString::from(value));
        assert!(nvidia_idle(var, &drm), "{name}={value}");
    }
}

#[test]
fn a_compositor_whose_primary_gpu_is_nvidia_or_unknown_keeps_nvidia() {
    let t = TestDir::new("icdidle-compositor");
    let drm = hybrid(&t);
    assert!(nvidia_idle(env(&[("AQ_DRM_DEVICES", "/dev/dri/card1:/dev/dri/card0")]), &drm));
    assert!(!nvidia_idle(env(&[("AQ_DRM_DEVICES", "/dev/dri/card0:/dev/dri/card1")]), &drm));
    assert!(!nvidia_idle(env(&[("WLR_DRM_DEVICES", "/dev/dri/card0")]), &drm));
    // A name that resolves to no card here is unknown, and unknown keeps the drivers.
    assert!(!nvidia_idle(env(&[("AQ_DRM_DEVICES", "/dev/dri/flea-no-such-gpu")]), &drm));
}

#[test]
fn a_compositor_gpu_named_by_a_udev_link_resolves_to_its_card() {
    let t = TestDir::new("icdidle-udev-link");
    let drm = hybrid(&t);
    let dev = t.dir("dev");
    std::fs::write(dev.join("card1"), "").unwrap();
    std::fs::write(dev.join("card0"), "").unwrap();
    std::os::unix::fs::symlink("card1", dev.join("intel-igpu")).unwrap();
    std::os::unix::fs::symlink("card0", dev.join("nvidia-dgpu")).unwrap();
    let list = |link: &str| OsString::from(dev.join(link));
    let (igpu, dgpu) = (list("intel-igpu"), list("nvidia-dgpu"));
    assert!(nvidia_idle(|key: &str| (key == "AQ_DRM_DEVICES").then(|| igpu.clone()), &drm));
    assert!(!nvidia_idle(|key: &str| (key == "AQ_DRM_DEVICES").then(|| dgpu.clone()), &drm));
}
