use super::*;
use crate::backend::testdir::TestDir;
use std::path::{Path, PathBuf};

const NVIDIA: (u32, u32, u32) = (0x10de, 0x27e0, 2);
const INTEL: (u32, u32, u32) = (0x8086, 0xa788, 1);
const AMD: (u32, u32, u32) = (0x1002, 0x744c, 2);

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

fn matrix(label: &str, gpus: &[(u32, u32, u32)], display: Option<usize>) {
    let root = TestDir::new(label);
    let pci = root.dir("pci");
    let drm = root.dir("drm");
    let icd = manifests(&root);
    let omitted = match label {
        "nvidia-only" => Some("nvk.json"),
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
    let (filter, applied) =
        crate::icdexclude::decide_exclusion(|_| None, &pci, &drm, &[root.path().join("data")]);
    let nvidia = gpus.iter().any(|gpu| gpu.0 == NVIDIA.0);
    // An NVIDIA GPU lighting the display keeps its drivers; one beside another GPU's display is idle and loses them.
    let idle = nvidia && display.is_some_and(|index| gpus[index].0 != NVIDIA.0);
    assert_eq!(applied, idle, "{label}: idle rule");
    if nvidia {
        let filter = filter.as_deref().unwrap_or_default();
        for name in ["nvidia_icd.json", "nvk.json"] {
            let installed = icd.join(name).exists();
            assert_eq!(
                filter.split(',').any(|entry| entry == name),
                idle && installed,
                "{label}: {filter}"
            );
        }
    }
    let names = configured_pin(&root, false);
    assert!(names.is_empty(), "{label}: GPUI forced an ICD: {names}");
    println!("MATRIX {label}: filter={filter:?} GPUI_pin={names:?}");
}

#[test]
fn l_nvidia_nvidia_proprietary_only() {
    matrix("nvidia-only", &[NVIDIA], Some(0));
}

#[test]
fn l_nvidia_nvidia_nvk_only() {
    matrix("nvk-only", &[NVIDIA], Some(0));
}

#[test]
fn l_nvidia_both_nvidia_drivers_installed_does_not_pin_an_icd() {
    matrix("both-nvidia-drivers", &[NVIDIA], Some(0));
}

#[test]
fn l_nvidia_intel_compositor_disables_the_idle_nvidia_drivers() {
    matrix("intel-display", &[INTEL, NVIDIA], Some(0));
}

#[test]
fn l_nvidia_nvidia_compositor_keeps_nvidia_driver() {
    matrix("nvidia-display", &[INTEL, NVIDIA], Some(1));
}

#[test]
fn l_nvidia_amd_nvidia_both_compositor_directions() {
    matrix("amd-display", &[AMD, NVIDIA], Some(0));
    matrix("nvidia-amd-display", &[AMD, NVIDIA], Some(1));
}

#[test]
fn l_nvidia_missing_library_does_not_pin_an_icd() {
    matrix("missing-nvidia-lib", &[NVIDIA], Some(0));
}

#[test]
fn l_nvidia_gpu_less_box_does_not_pin_an_icd() {
    matrix("no-gpu", &[], None);
}

#[test]
fn l_nvidia_render_node_only_nvidia_is_present_but_idle() {
    let root = TestDir::new("nvidia-render-only");
    let pci = root.dir("pci");
    let drm = root.dir("drm");
    manifests(&root);
    card(&drm, "card0", INTEL, true);
    card(&drm, "renderD129", NVIDIA, false);
    let data = [root.path().join("data")];
    let filter = crate::icdexclude::decide_exclusion(|_| None, &pci, &drm, &data);
    assert_eq!(
        filter,
        (Some("nvidia_icd.json,nvk.json,radeon.json".into()), true)
    );
    // Present all the same: asking for the dGPU keeps its drivers and disables only the absent vendor's.
    let keep = |name: &str| (name == "FLEA_VK_KEEP_NVIDIA").then(|| "1".into());
    let filter = crate::icdexclude::decide_exclusion(keep, &pci, &drm, &data);
    assert_eq!(filter, (Some("radeon.json".into()), false));
}

#[test]
fn l_nvidia_idle_nvidia_keeps_its_drivers_when_the_display_gpu_has_none() {
    let root = TestDir::new("nvidia-idle-no-intel-driver");
    let pci = root.dir("pci");
    let drm = root.dir("drm");
    let icd = manifests(&root);
    let path = icd.join("intel.json");
    root.assert_contains(&path);
    std::fs::remove_file(path).unwrap();
    card(&drm, "card0", INTEL, true);
    card(&drm, "card1", NVIDIA, false);
    // Disabling NVIDIA here would leave GPUI only Lavapipe, a software renderer, for the Intel panel.
    let data = [root.path().join("data")];
    let filter = crate::icdexclude::decide_exclusion(|_| None, &pci, &drm, &data);
    assert_eq!(filter, (Some("radeon.json".into()), false));
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

fn configured_pin(root: &TestDir, operator: bool) -> String {
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
        .env("FLEA_NVIDIA_CHILD", "1");
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
    assert_eq!(configured_pin(&root, true), "/operator/driver.json");
}

#[test]
fn l_nvidia_inherited_flea_pin_is_reconsidered() {
    let root = TestDir::new("nvidia-inherited");
    manifests(&root);
    root.file("inherit", "inherited Flea pin fixture");
    assert!(configured_pin(&root, false).is_empty());
}
