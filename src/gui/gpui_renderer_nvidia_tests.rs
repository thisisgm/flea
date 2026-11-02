use super::*;
use crate::backend::testdir::TestDir;
use std::path::{Path, PathBuf};

const NVIDIA: (u32, u32, u32) = (0x10de, 0x27e0, 2);
const INTEL: (u32, u32, u32) = (0x8086, 0xa788, 1);
const AMD: (u32, u32, u32) = (0x1002, 0x744c, 2);
const CPU: (u32, u32, u32) = (0x10005, 0, 4);

fn card(drm: &Path, name: &str, gpu: (u32, u32, u32), connected: bool) {
    let device = drm.join(name).join("device");
    std::fs::create_dir_all(&device).unwrap();
    std::fs::write(device.join("vendor"), format!("0x{:04x}", gpu.0)).unwrap();
    std::fs::write(device.join("device"), format!("0x{:04x}", gpu.1)).unwrap();
    std::os::unix::fs::symlink("/sys/bus/pci", device.join("subsystem")).unwrap();
    if connected {
        let connector = drm.join(format!("{name}-DP-1"));
        std::fs::create_dir_all(&connector).unwrap();
        std::fs::write(connector.join("status"), "connected").unwrap();
    }
}

fn manifests(root: &TestDir) -> PathBuf {
    let dir = root.dir("data/vulkan/icd.d");
    for (name, lib) in [
        ("nvidia_icd.json", "libGLX_nvidia.so.0"),
        ("nvk.json", "libvulkan_nouveau.so"),
        ("intel.json", "libvulkan_intel.so"),
        ("radeon.json", "libvulkan_radeon.so"),
        ("lvp.json", "libvulkan_lvp.so"),
    ] {
        std::fs::write(
            dir.join(name),
            format!(r#"{{"ICD":{{"library_path":"{lib}"}}}}"#),
        )
        .unwrap();
    }
    dir
}

fn matrix(
    label: &str,
    gpus: &[(u32, u32, u32)],
    display: Option<usize>,
    probe: Result<Vec<(u32, u32, u32)>, String>,
    expected_pin: &[&str],
) {
    let root = TestDir::new(label);
    let pci = root.dir("pci");
    let drm = root.dir("drm");
    let icd = manifests(&root);
    let omitted = match label {
        "nvidia-only" | "nvidia-and-lvp" => Some("nvk.json"),
        "nvk-only" => Some("nvidia_icd.json"),
        _ => None,
    };
    if let Some(name) = omitted {
        let path = icd.join(name);
        root.assert_contains(&path);
        assert!(path.is_absolute() && !path.as_os_str().is_empty());
        std::fs::remove_file(path).unwrap();
    }
    if label == "missing-nvidia-lib" {
        let missing = root.path().join("missing/libGLX_nvidia.so.0");
        assert!(!missing.exists());
        std::fs::write(
            icd.join("nvidia_icd.json"),
            format!(r#"{{"ICD":{{"library_path":"{}"}}}}"#, missing.display()),
        )
        .unwrap();
    }
    for (index, &gpu) in gpus.iter().enumerate() {
        card(&drm, &format!("card{index}"), gpu, display == Some(index));
    }
    let filter =
        crate::icdexclude::decide_exclusion(|_| None, &pci, &drm, &[root.path().join("data")]);
    if gpus.iter().any(|gpu| gpu.0 == NVIDIA.0) {
        let filter = filter.as_deref().unwrap_or_default();
        assert!(
            !filter.contains("nvidia") && !filter.contains("nvk"),
            "{label}: {filter}"
        );
    }
    std::fs::write(
        icd.join("decoy.json"),
        r#"{"ICD":{"library_path":"/opt/nvidia/libvulkan_lvp.so"}}"#,
    )
    .unwrap();
    let legacy = probe
        .as_ref()
        .ok()
        .map(|devices| vulkan::display_pin_in(devices, &drm, std::slice::from_ref(&icd)));
    let names = configured_pin(&root, probe.is_err(), &probe.unwrap_or_default(), false);
    assert!(names.is_empty(), "{label}: GPUI forced an ICD: {names}");
    let legacy_names: Vec<_> = match legacy {
        Some(vulkan::DisplayPin::Pin { icd, .. }) => std::env::split_paths(&icd)
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect(),
        _ => Vec::new(),
    };
    for name in expected_pin {
        assert!(
            legacy_names.iter().any(|actual| actual == name),
            "{label}: missing legacy {name}: {legacy_names:?}"
        );
    }
    assert_eq!(
        legacy_names.len(),
        expected_pin.len(),
        "{label}: {legacy_names:?}"
    );
    assert!(
        !legacy_names.iter().any(|name| name.contains("lvp")),
        "{label}: software was pinned"
    );
    println!("MATRIX {label}: filter={filter:?} GPUI_pin={names:?} Qt_pin={legacy_names:?}");
}

#[test]
fn l_nvidia_nvidia_proprietary_only() {
    matrix(
        "nvidia-only",
        &[NVIDIA],
        Some(0),
        Ok(vec![NVIDIA, CPU]),
        &[],
    );
}

#[test]
fn l_nvidia_nvidia_nvk_only() {
    matrix("nvk-only", &[NVIDIA], Some(0), Ok(vec![NVIDIA]), &[]);
}

#[test]
fn l_nvidia_nouveau_probe_failure_does_not_pin_an_icd() {
    matrix(
        "nouveau-no-vulkan",
        &[NVIDIA],
        Some(0),
        Err("vkEnumeratePhysicalDevices listed no device".into()),
        &[],
    );
}

#[test]
fn l_nvidia_intel_compositor_keeps_nvidia_driver() {
    matrix(
        "intel-display",
        &[INTEL, NVIDIA],
        Some(0),
        Ok(vec![NVIDIA, INTEL, CPU]),
        &["intel.json"],
    );
}

#[test]
fn l_nvidia_nvidia_compositor_keeps_nvidia_driver() {
    matrix(
        "nvidia-display",
        &[INTEL, NVIDIA],
        Some(1),
        Ok(vec![INTEL, NVIDIA, CPU]),
        &["nvidia_icd.json", "nvk.json"],
    );
}

#[test]
fn l_nvidia_amd_nvidia_both_compositor_directions() {
    matrix(
        "amd-display",
        &[AMD, NVIDIA],
        Some(0),
        Ok(vec![NVIDIA, AMD, CPU]),
        &["radeon.json"],
    );
    matrix(
        "nvidia-amd-display",
        &[AMD, NVIDIA],
        Some(1),
        Ok(vec![NVIDIA, AMD, CPU]),
        &["nvidia_icd.json", "nvk.json"],
    );
}

#[test]
fn l_nvidia_missing_library_probe_failure_does_not_pin_an_icd() {
    matrix(
        "missing-nvidia-lib",
        &[NVIDIA],
        Some(0),
        Err("vkCreateInstance answered -9 for VK_KHR_surface".into()),
        &[],
    );
}

#[test]
fn l_nvidia_nvidia_with_lavapipe_is_never_software_pinned() {
    matrix(
        "nvidia-and-lvp",
        &[NVIDIA],
        Some(0),
        Ok(vec![CPU, NVIDIA]),
        &[],
    );
}

#[test]
fn l_nvidia_cpu_only_probe_does_not_pin_an_icd() {
    matrix("no-gpu", &[], None, Ok(vec![CPU]), &[]);
}

#[test]
fn l_nvidia_render_node_only_nvidia_is_not_excluded() {
    let root = TestDir::new("nvidia-render-only");
    let pci = root.dir("pci");
    let drm = root.dir("drm");
    manifests(&root);
    card(&drm, "card0", INTEL, true);
    card(&drm, "renderD129", NVIDIA, false);
    let filter =
        crate::icdexclude::decide_exclusion(|_| None, &pci, &drm, &[root.path().join("data")]);
    assert_eq!(filter.as_deref(), Some("radeon.json"));
}

#[test]
fn l_nvidia_gpui_removes_qt_renderer_choices_and_retry_markers() {
    let mut command = Command::new("flea-gui");
    clear_qt_renderer(&mut command);
    for key in [
        "QSG_RHI_BACKEND",
        "FLEA_RENDERER_AUTOMATIC",
        "QT_VK_PHYSICAL_DEVICE_INDEX",
    ] {
        assert!(command
            .get_envs()
            .any(|(name, value)| name == key && value.is_none()));
    }
}

fn configured_pin(
    root: &TestDir,
    failed: bool,
    devices: &[(u32, u32, u32)],
    operator: bool,
) -> String {
    let loader = root.dir("loader");
    let source = root.file("probe.c", MOCK_LOADER);
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(loader.join("libvulkan.so.1"))
        .arg(source)
        .status()
        .expect("compile the fixture Vulkan loader");
    assert!(status.success());
    let devices = devices
        .iter()
        .map(|id| format!("{} {} {};", id.0, id.1, id.2))
        .collect::<String>();
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "gui::launch::gpui_renderer::nvidia_tests::l_nvidia_configure_child",
            "--nocapture",
        ])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", root.dir("home"))
        .env("XDG_CONFIG_HOME", root.dir("config"))
        .env("XDG_DATA_HOME", root.path().join("data"))
        .env("XDG_CACHE_HOME", root.dir("cache"))
        .env("TMPDIR", root.dir("tmp"))
        .env("LD_LIBRARY_PATH", loader)
        .env("FLEA_VK_PROBE_CACHE", "off")
        .env("FLEA_NVIDIA_CHILD", "1")
        .env("FLEA_NVIDIA_PROBE_ERROR", if failed { "1" } else { "0" })
        .env("FLEA_NVIDIA_DEVICES", devices);
    if operator {
        child.env("VK_DRIVER_FILES", "/operator/driver.json");
    }
    if root.join("inherit").is_file() {
        child
            .env("VK_DRIVER_FILES", "/parent/lvp.json")
            .env("VK_ICD_FILENAMES", "/parent/lvp.json")
            .env(vulkan::PIN_MARKER, "1");
    }
    let output = child.output().expect("configure child");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("running 1 test"),
        "child test did not run: {stdout}"
    );
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("FLEA_NVIDIA_PIN="))
        .expect("configured pin")
        .to_owned()
}

#[test]
fn l_nvidia_configure_child() {
    if std::env::var("FLEA_NVIDIA_CHILD").as_deref() != Ok("1") {
        return;
    }
    let mut cmd = Command::new("flea-gui");
    configure(&mut cmd);
    let pin = cmd
        .get_envs()
        .find(|(name, _)| *name == "VK_DRIVER_FILES")
        .map(|(_, value)| value.map(OsStr::to_os_string))
        .unwrap_or_else(|| std::env::var_os("VK_DRIVER_FILES"));
    println!(
        "FLEA_NVIDIA_PIN={}",
        pin.unwrap_or_default().to_string_lossy()
    );
}

#[test]
fn l_nvidia_operator_driver_selection_survives() {
    let root = TestDir::new("nvidia-operator");
    manifests(&root);
    assert_eq!(
        configured_pin(&root, false, &[CPU], true),
        "/operator/driver.json"
    );
}

#[test]
fn l_nvidia_inherited_flea_pin_is_reconsidered() {
    let root = TestDir::new("nvidia-inherited");
    manifests(&root);
    root.file("inherit", "inherited Flea pin fixture");
    assert!(configured_pin(&root, true, &[], false).is_empty());
}

#[test]
fn l_nvidia_display_pin_never_reads_a_fifo_manifest() {
    let root = TestDir::new("nvidia-fifo");
    let drm = root.dir("drm");
    card(&drm, "card0", INTEL, false);
    card(&drm, "card1", NVIDIA, true);
    let dir = root.dir("icd");
    crate::backend::fifotest::mkfifo(&dir.join("nvidia.json"));
    crate::backend::fifotest::within("NVIDIA fifo manifest", move || {
        assert!(matches!(
            vulkan::display_pin_in(&[INTEL, NVIDIA], &drm, &[dir]),
            vulkan::DisplayPin::Unmatched { vendor: 0x10de }
        ));
        drop(root);
    });
}

const MOCK_LOADER: &str = r#"
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
int vkCreateInstance(void *request, void *alloc, void **instance) {
    *instance = (void *)1;
    return strcmp(getenv("FLEA_NVIDIA_PROBE_ERROR"), "1") == 0 ? -9 : 0;
}
int vkEnumeratePhysicalDevices(void *instance, uint32_t *count, void **handles) {
    const char *text = getenv("FLEA_NVIDIA_DEVICES");
    unsigned n = 0;
    for (const char *p = text; *p; p++) if (*p == ';') n++;
    if (handles) for (unsigned i = 0; i < n && i < *count; i++) handles[i] = (void *)(uintptr_t)(i + 1);
    *count = n;
    return 0;
}
void vkGetPhysicalDeviceProperties(void *device, unsigned char *properties) {
    const char *text = getenv("FLEA_NVIDIA_DEVICES");
    for (uintptr_t i = 1; i < (uintptr_t)device; i++) text = strchr(text, ';') + 1;
    unsigned values[3] = {0};
    sscanf(text, "%u %u %u", &values[0], &values[1], &values[2]);
    memcpy(properties + 8, values, sizeof(values));
}
void vkDestroyInstance(void *instance, void *alloc) {}
"#;
