// Quickshell hands QRhi::create a QVulkanInstance it never created, which kills the shell rather than raising.
use std::ffi::{c_void, CStr, OsStr};
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

// dlopen(3) RTLD_NOW, so a loader missing an entry point fails here and never at the first call.
const RTLD_NOW: c_int = 2;
// The soname is the only Vulkan file name the loader ABI guarantees.
const LIBVULKAN: &CStr = c"libvulkan.so.1";
// VkResult VK_SUCCESS, the one result that means the call did what was asked.
const VK_SUCCESS: i32 = 0;
// VkResult VK_INCOMPLETE: the second enumerate wrote some handles and the count grew after the first.
const VK_INCOMPLETE: i32 = 5;
// VkStructureType VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO.
const INSTANCE_CREATE_INFO: u32 = 1;

// VkInstanceCreateInfo, with no application info and no layers: only the extension list is filled in.
#[repr(C)]
struct InstanceCreateInfo {
    s_type: u32,
    p_next: *const c_void,
    flags: u32,
    p_application_info: *const c_void,
    enabled_layer_count: u32,
    pp_enabled_layer_names: *const *const c_char,
    enabled_extension_count: u32,
    pp_enabled_extension_names: *const *const c_char,
}

// std already links the system libc, so the three symbols are declared here rather than taking a crate.
extern "C" {
    fn dlopen(file: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *mut c_char;
}

// vkCreateInstance, vkEnumeratePhysicalDevices, vkGetPhysicalDeviceProperties and vkDestroyInstance.
type CreateInstance =
    unsafe extern "C" fn(*const InstanceCreateInfo, *const c_void, *mut *mut c_void) -> i32;
type EnumeratePhysicalDevices = unsafe extern "C" fn(*mut c_void, *mut u32, *mut *mut c_void) -> i32;
type GetPhysicalDeviceProperties = unsafe extern "C" fn(*mut c_void, *mut u8);
type DestroyInstance = unsafe extern "C" fn(*mut c_void, *const c_void);

// vendorID and deviceID sit at these offsets in VkPhysicalDeviceProperties; the rest of the struct
// is written into a buffer large enough that the write cannot run off the end.
const VENDOR_ID_OFFSET: usize = 8;
const DEVICE_ID_OFFSET: usize = 12;
const PROPERTIES_BYTES: usize = 2048;

// VkPhysicalDeviceLimits contains VkDeviceSize fields, so the driver write needs 8-byte alignment.
#[repr(C, align(8))]
struct PropertiesBuf([u8; PROPERTIES_BYTES]);

// VK_KHR_surface plus one platform surface extension, the pair Qt's Vulkan RHI presents through.
const SURFACE: &CStr = c"VK_KHR_surface";
// libQt6XcbQpa.so.6 and libQt6WaylandClient.so.6 here carry these two and no other surface name.
const WAYLAND_SURFACE: &CStr = c"VK_KHR_wayland_surface";
const XCB_SURFACE: &CStr = c"VK_KHR_xcb_surface";

// Only WAYLAND_DISPLAY is read, so a Wayland session wins on a box that has both set.
fn platform_surface() -> &'static CStr {
    surface_for(std::env::var_os("WAYLAND_DISPLAY").as_deref())
}

// Split from platform_surface() so a test can ask the rule without setting the variable for every thread.
// Sample input: Some("wayland-1"), and None or Some("") for a session that is not Wayland.
fn surface_for(wayland_display: Option<&OsStr>) -> &'static CStr {
    match wayland_display {
        Some(value) if !value.is_empty() => WAYLAND_SURFACE,
        _ => XCB_SURFACE,
    }
}

// dlerror() is the only thing that names why a load failed, and reading it clears it for the next call.
unsafe fn dl_reason() -> String {
    let text = dlerror();
    if text.is_null() {
        return String::from("the dynamic loader gave no reason");
    }
    CStr::from_ptr(text).to_string_lossy().into_owned()
}

// One dlsym that names the symbol it could not find, because "unusable" without the name is a dead end.
unsafe fn entry(library: *mut c_void, symbol: &CStr) -> Result<*mut c_void, String> {
    let found = dlsym(library, symbol.as_ptr());
    if found.is_null() {
        return Err(format!(
            "libvulkan.so.1 has no {}, {}",
            symbol.to_string_lossy(),
            dl_reason()
        ));
    }
    Ok(found)
}

// Ok only when this box can start Vulkan the way Qt starts it; the error is the sentence the operator reads.
// The Vec is PCI (vendor, device) in vkEnumeratePhysicalDevices order, which is also Qt's order.
pub fn usable() -> Result<Vec<(u32, u32)>, String> {
    usable_with(&[SURFACE, platform_surface()])
}

// Split from usable() so a test can ask the same question with an extension no loader can offer.
// corner: the handle is never dlclose()d, because this process execs qs a moment later.
fn usable_with(extensions: &[&CStr]) -> Result<Vec<(u32, u32)>, String> {
    let names: Vec<*const c_char> = extensions.iter().map(|e| e.as_ptr()).collect();
    let asked: Vec<String> = extensions
        .iter()
        .map(|e| e.to_string_lossy().into_owned())
        .collect();
    let asked = asked.join(" and ");
    unsafe {
        let library = dlopen(LIBVULKAN.as_ptr(), RTLD_NOW);
        if library.is_null() {
            return Err(format!("libvulkan.so.1 did not load, {}", dl_reason()));
        }
        let create: CreateInstance = std::mem::transmute(entry(library, c"vkCreateInstance")?);
        let enumerate: EnumeratePhysicalDevices =
            std::mem::transmute(entry(library, c"vkEnumeratePhysicalDevices")?);
        let destroy: DestroyInstance = std::mem::transmute(entry(library, c"vkDestroyInstance")?);
        // Missing this symbol still leaves Vulkan usable: Qt can start, it just cannot be pointed at a display GPU.
        let properties = entry(library, c"vkGetPhysicalDeviceProperties").ok().map(|found| {
            std::mem::transmute::<_, GetPhysicalDeviceProperties>(found)
        });

        let request = InstanceCreateInfo {
            s_type: INSTANCE_CREATE_INFO,
            p_next: null(),
            flags: 0,
            p_application_info: null(),
            enabled_layer_count: 0,
            pp_enabled_layer_names: null(),
            enabled_extension_count: names.len() as u32,
            pp_enabled_extension_names: names.as_ptr(),
        };
        let mut instance: *mut c_void = null_mut();
        let created = create(&request, null(), &mut instance);
        if created != VK_SUCCESS {
            return Err(format!("vkCreateInstance answered {created} for {asked}"));
        }
        if instance.is_null() {
            return Err(format!("vkCreateInstance took {asked} and returned no instance"));
        }
        let mut devices: u32 = 0;
        let listed = enumerate(instance, &mut devices, null_mut());
        if listed != VK_SUCCESS {
            destroy(instance, null());
            return Err(format!("vkEnumeratePhysicalDevices answered {listed}"));
        }
        if devices == 0 {
            destroy(instance, null());
            return Err(String::from("vkEnumeratePhysicalDevices succeeded and listed no device"));
        }
        let mut handles = vec![null_mut(); devices as usize];
        let mut filled = devices;
        let listed = enumerate(instance, &mut filled, handles.as_mut_ptr());
        // VK_INCOMPLETE still wrote `filled` handles; treating it as failure would drop to OpenGL.
        let complete = listed == VK_SUCCESS || listed == VK_INCOMPLETE;
        let ids = match (complete, properties) {
            (true, Some(get)) => handles
                .iter()
                .take(filled as usize)
                .map(|handle| pci_id(*handle, get))
                .collect(),
            _ => vec![(0, 0); devices as usize],
        };
        destroy(instance, null());
        if !complete {
            return Err(format!("vkEnumeratePhysicalDevices answered {listed}"));
        }
        Ok(ids)
    }
}

// Sample input: a VkPhysicalDevice and vkGetPhysicalDeviceProperties, which writes vendorID then deviceID.
fn pci_id(handle: *mut c_void, get: GetPhysicalDeviceProperties) -> (u32, u32) {
    let mut buf = PropertiesBuf([0u8; PROPERTIES_BYTES]);
    unsafe { get(handle, buf.0.as_mut_ptr()) }
    (
        u32::from_ne_bytes(buf.0[VENDOR_ID_OFFSET..DEVICE_ID_OFFSET].try_into().unwrap()),
        u32::from_ne_bytes(buf.0[DEVICE_ID_OFFSET..DEVICE_ID_OFFSET + 4].try_into().unwrap()),
    )
}

// A colon-separated ICD list for Qt, or None when device 0 is already a display GPU
// or the loader order is not our problem. Qt's enumerate order is not the loader's:
// this box lists NVIDIA then Intel to vkEnumeratePhysicalDevices and Intel then NVIDIA
// to QRhi, so an index pin would name the wrong device. Restricting the ICD list does not
// care about order: Qt can only open the GPU that owns a connected connector.
pub fn display_icd(devices: &[(u32, u32)]) -> Option<String> {
    icd_for_displays(
        devices,
        &display_pci_ids(Path::new("/sys/class/drm")),
        &icd_search_dirs(),
    )
}

fn icd_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // Empty is absent, the same rule userfile::env_dir uses: an empty XDG_CONFIG_HOME is not a root.
    if let Some(xdg) = crate::userfile::env_dir("XDG_CONFIG_HOME") {
        dirs.push(xdg.join("vulkan/icd.d"));
    } else if let Some(home) = crate::userfile::env_dir("HOME") {
        dirs.push(home.join(".config/vulkan/icd.d"));
    }
    dirs.push(PathBuf::from("/etc/vulkan/icd.d"));
    dirs.push(PathBuf::from("/usr/share/vulkan/icd.d"));
    dirs
}

// Sample input: devices [(0x8086, 0xa788), (0x10de, 0x27e0)] and displays [(0x10de, 0x27e0)].
fn needs_icd_pin(devices: &[(u32, u32)], displays: &[(u32, u32)]) -> bool {
    if displays.is_empty() || devices.is_empty() {
        return false;
    }
    let has_display = devices.iter().any(|id| displays.contains(id));
    let has_other = devices.iter().any(|id| !displays.contains(id));
    has_display && has_other
}

fn icd_for_displays(devices: &[(u32, u32)], displays: &[(u32, u32)], icd_dirs: &[PathBuf]) -> Option<String> {
    if !needs_icd_pin(devices, displays) {
        return None;
    }
    let mut files = Vec::new();
    for dir in icd_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(body) = std::fs::read_to_string(&path) else { continue };
            let Some(lib) = icd_library(&body) else { continue };
            let Some(vendor) = vendor_of_library(lib) else { continue };
            if !displays.iter().any(|(v, _)| *v == vendor) {
                continue;
            }
            let text = path.to_string_lossy().into_owned();
            if !files.contains(&text) {
                files.push(text);
            }
        }
    }
    if files.is_empty() {
        None
    } else {
        Some(files.join(":"))
    }
}

// Sample input: the packaged nvidia_icd.json, whose library_path is libGLX_nvidia.so.0.
fn icd_library(body: &str) -> Option<&str> {
    let key = body.find("\"library_path\"")?;
    let after = body[key + "\"library_path\"".len()..].trim_start();
    let after = after.strip_prefix(':')?.trim_start();
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(&after[..end])
}

// Sample input: "libGLX_nvidia.so.0", "libvulkan_intel.so", "libvulkan_intel_hasvk.so".
fn vendor_of_library(lib: &str) -> Option<u32> {
    let lib = lib.to_ascii_lowercase();
    // hasvk is Haswell/Broadwell, not the display GPU beside a modern iGPU ICD.
    if lib.contains("hasvk") {
        return None;
    }
    if lib.contains("nvidia") {
        Some(0x10de)
    } else if lib.contains("intel") {
        Some(0x8086)
    } else if lib.contains("radeon") || lib.contains("amdvlk") || lib.contains("amd_") {
        Some(0x1002)
    } else if lib.contains("virtio") {
        Some(0x1af4)
    } else {
        None
    }
}

// Sample input: a sysfs drm class with card0 (Intel, disconnected) and card1 (NVIDIA, connected).
fn display_pci_ids(drm: &Path) -> Vec<(u32, u32)> {
    let Ok(entries) = std::fs::read_dir(drm) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(card) = connector_card(&name) else { continue };
        let status = std::fs::read_to_string(entry.path().join("status")).unwrap_or_default();
        if status.trim() != "connected" {
            continue;
        }
        let card_dir = drm.join(card);
        let vendor = parse_hex_id(&std::fs::read_to_string(card_dir.join("device").join("vendor")).unwrap_or_default());
        let device = parse_hex_id(&std::fs::read_to_string(card_dir.join("device").join("device")).unwrap_or_default());
        let Some(pair) = vendor.zip(device) else { continue };
        if !ids.contains(&pair) {
            ids.push(pair);
        }
    }
    ids
}

// Sample input: "card1-DP-1" is the NVIDIA DP, "card0" is the card itself, "renderD128" is a render node.
fn connector_card(name: &str) -> Option<&str> {
    let (card, rest) = name.split_once('-')?;
    if rest.is_empty() {
        return None;
    }
    let digits = card.strip_prefix("card")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(card)
}

// Sample input: "0x8086\n", "0XA788", "10de".
fn parse_hex_id(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u32::from_str_radix(hex, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The error names the call that refused, so this can no longer pass from the dlopen or dlsym branch.
    #[test]
    fn a_required_extension_no_loader_offers_reads_unusable() {
        let absent = c"VK_KHR_flea_probe_extension_that_cannot_exist";
        let alone = usable_with(&[absent]).unwrap_err();
        assert!(alone.starts_with("vkCreateInstance answered"), "{alone}");
        let beside = usable_with(&[SURFACE, absent]).unwrap_err();
        assert!(beside.starts_with("vkCreateInstance answered"), "{beside}");
    }

    // A refusal the operator cannot read is the defect: every arm names the call or library that refused, and the two that asked for extensions name them.
    #[test]
    fn the_refusal_names_the_call_and_the_extension_it_was_asked_for() {
        let absent = c"VK_KHR_flea_probe_extension_that_cannot_exist";
        // corner: a working loader lands on the vkCreateInstance arm, so the other five cannot be reached from here.
        let reason = usable_with(&[SURFACE, absent]).unwrap_err();
        assert!(reason.contains("VK_KHR_surface"), "{reason}");
        assert!(reason.contains("VK_KHR_flea_probe_extension_that_cannot_exist"), "{reason}");
    }

    // An exported-but-empty WAYLAND_DISPLAY is not a Wayland session, the rule paths::has_display() uses.
    #[test]
    fn the_surface_extension_follows_the_session() {
        assert_eq!(surface_for(Some(OsStr::new("wayland-1"))), WAYLAND_SURFACE);
        assert_eq!(surface_for(Some(OsStr::new(""))), XCB_SURFACE);
        assert_eq!(surface_for(None), XCB_SURFACE);
    }

    #[test]
    fn usable_lists_pci_ids_in_enumerate_order() {
        let displays = display_pci_ids(Path::new("/sys/class/drm"));
        if displays.is_empty() {
            return;
        }
        let ids = match usable() {
            Ok(ids) => ids,
            Err(reason)
                if reason.contains("libvulkan.so.1 did not load")
                    || reason.contains("listed no device") =>
            {
                return;
            }
            Err(reason) => panic!("{reason}"),
        };
        assert!(!ids.is_empty(), "{ids:?}");
        assert!(
            displays.iter().any(|id| ids.contains(id)),
            "devices {ids:?} displays {displays:?}"
        );
    }

    #[test]
    fn parse_hex_id_accepts_sysfs_and_bare_forms() {
        assert_eq!(parse_hex_id("0x8086\n"), Some(0x8086));
        assert_eq!(parse_hex_id("0XA788"), Some(0xa788));
        assert_eq!(parse_hex_id("10de"), Some(0x10de));
        assert_eq!(parse_hex_id("not-a-pci-id"), None);
    }

    #[test]
    fn connector_card_reads_only_drm_connector_names() {
        assert_eq!(connector_card("card1-DP-1"), Some("card1"));
        assert_eq!(connector_card("card0-eDP-1"), Some("card0"));
        assert_eq!(connector_card("card0"), None);
        assert_eq!(connector_card("renderD128"), None);
        assert_eq!(connector_card("card-DP-1"), None);
    }

    #[test]
    fn icd_library_reads_the_packaged_nvidia_shape() {
        let body = "{\n    \"file_format_version\" : \"1.0.1\",\n    \"ICD\": {\n        \"library_path\": \"libGLX_nvidia.so.0\",\n        \"api_version\" : \"1.4.341\"\n    }\n}\n";
        assert_eq!(icd_library(body), Some("libGLX_nvidia.so.0"));
    }

    #[test]
    fn vendor_of_library_skips_hasvk_and_names_the_rest() {
        assert_eq!(vendor_of_library("libGLX_nvidia.so.0"), Some(0x10de));
        assert_eq!(vendor_of_library("libvulkan_intel.so"), Some(0x8086));
        assert_eq!(vendor_of_library("libvulkan_intel_hasvk.so"), None);
        assert_eq!(vendor_of_library("libvulkan_radeon.so"), Some(0x1002));
    }

    // Hybrid: Vulkan sees Intel and NVIDIA, only NVIDIA has a panel, so the Intel ICD has to go.
    #[test]
    fn a_gpu_with_no_display_needs_the_display_icd() {
        let intel = (0x8086, 0xa788);
        let nvidia = (0x10de, 0x27e0);
        assert!(needs_icd_pin(&[intel, nvidia], &[nvidia]));
        assert!(needs_icd_pin(&[intel, nvidia], &[intel]));
        assert!(!needs_icd_pin(&[intel, nvidia], &[intel, nvidia]));
        assert!(!needs_icd_pin(&[nvidia], &[nvidia]));
        assert!(!needs_icd_pin(&[intel, nvidia], &[]));
    }

    #[test]
    fn icd_for_displays_picks_the_nvidia_json_and_not_hasvk() {
        let root = std::env::temp_dir().join(format!("flea-icd-{}-{}", std::process::id(), "hybrid"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("nvidia_icd.json"), "{\"ICD\":{\"library_path\":\"libGLX_nvidia.so.0\"}}\n").unwrap();
        std::fs::write(root.join("intel_icd.json"), "{\"ICD\":{\"library_path\":\"libvulkan_intel.so\"}}\n").unwrap();
        std::fs::write(root.join("intel_hasvk_icd.json"), "{\"ICD\":{\"library_path\":\"libvulkan_intel_hasvk.so\"}}\n").unwrap();
        let intel = (0x8086, 0xa788);
        let nvidia = (0x10de, 0x27e0);
        let picked = icd_for_displays(&[intel, nvidia], &[nvidia], &[root.clone()]).unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(picked.ends_with("nvidia_icd.json"), "{picked}");
        assert!(!picked.contains("intel"), "{picked}");
    }

    #[test]
    fn display_pci_ids_reads_connected_cards_from_a_fake_drm_tree() {
        let root = std::env::temp_dir().join(format!("flea-drm-{}-{}", std::process::id(), "hybrid"));
        let _ = std::fs::remove_dir_all(&root);
        write_card(&root, "card0", 0x8086, 0xa788);
        write_connector(&root, "card0-eDP-1", "disconnected");
        write_card(&root, "card1", 0x10de, 0x27e0);
        write_connector(&root, "card1-DP-1", "connected");
        write_connector(&root, "card1-eDP-1", "connected");
        let ids = display_pci_ids(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(ids, vec![(0x10de, 0x27e0)]);
    }

    fn write_card(root: &Path, card: &str, vendor: u32, device: u32) {
        let dir = root.join(card).join("device");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vendor"), format!("0x{vendor:04x}\n")).unwrap();
        std::fs::write(dir.join("device"), format!("0x{device:04x}\n")).unwrap();
    }

    fn write_connector(root: &Path, name: &str, status: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("status"), format!("{status}\n")).unwrap();
    }
}
