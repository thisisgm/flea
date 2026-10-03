// Quickshell hands QRhi::create a QVulkanInstance it never created, which kills the shell rather than raising.
use std::ffi::{c_void, CStr, OsStr};
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};
use std::process::Command;
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

// vendorID and deviceID sit at these offsets in VkPhysicalDeviceProperties, which PROPERTIES_BYTES is sized to hold.
const VENDOR_ID_OFFSET: usize = 8;
const DEVICE_ID_OFFSET: usize = 12;
// deviceType follows deviceID at offset 16 in VkPhysicalDeviceProperties.
const DEVICE_TYPE_OFFSET: usize = 16;
const PROPERTIES_BYTES: usize = 2048;

// VkPhysicalDeviceType VK_PHYSICAL_DEVICE_TYPE_CPU, the kind lavapipe reports.
const DEVICE_TYPE_CPU: u32 = 4;

// PCI vendor ids, the values /sys/class/drm/card*/device/vendor carries.
pub(crate) const PCI_VENDOR_NVIDIA: u32 = 0x10de;
pub(crate) const PCI_VENDOR_INTEL: u32 = 0x8086;
pub(crate) const PCI_VENDOR_AMD: u32 = 0x1002;
const PCI_VENDOR_VIRTIO: u32 = 0x1af4;

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

// Ok lists each device's PCI (vendor, device) plus its VkPhysicalDeviceType in enumerate order; the error is the sentence the operator reads.
pub fn usable() -> Result<Vec<(u32, u32, u32)>, String> {
    usable_with(&[SURFACE, platform_surface()])
}

// Split from usable() so a test can ask the same question with an extension no loader can offer.
// corner: the handle is never dlclose()d, because this process execs qs a moment later.
fn usable_with(extensions: &[&CStr]) -> Result<Vec<(u32, u32, u32)>, String> {
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
        let properties: GetPhysicalDeviceProperties =
            std::mem::transmute(entry(library, c"vkGetPhysicalDeviceProperties")?);

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
        let ids: Vec<(u32, u32, u32)> = if complete {
            handles
                .iter()
                .take(filled as usize)
                .map(|handle| pci_id(*handle, properties))
                .collect()
        } else {
            Vec::new()
        };
        destroy(instance, null());
        if !complete {
            return Err(format!("vkEnumeratePhysicalDevices answered {listed}"));
        }
        Ok(ids)
    }
}

// Sample input: a VkPhysicalDevice and vkGetPhysicalDeviceProperties, which writes vendorID then deviceID then deviceType.
fn pci_id(handle: *mut c_void, get: GetPhysicalDeviceProperties) -> (u32, u32, u32) {
    let mut buf = PropertiesBuf([0u8; PROPERTIES_BYTES]);
    unsafe { get(handle, buf.0.as_mut_ptr()) }
    (
        u32::from_ne_bytes(buf.0[VENDOR_ID_OFFSET..DEVICE_ID_OFFSET].try_into().unwrap()),
        u32::from_ne_bytes(buf.0[DEVICE_ID_OFFSET..DEVICE_ID_OFFSET + 4].try_into().unwrap()),
        u32::from_ne_bytes(buf.0[DEVICE_TYPE_OFFSET..DEVICE_TYPE_OFFSET + 4].try_into().unwrap()),
    )
}

// The launcher marks its own pin, so a program Flea starts can tell it from the operator's own list.
pub const PIN_MARKER: &str = "FLEA_VK_PIN";

// Empty is absent, the same rule QSG_RHI_BACKEND and the loader variables already follow.
fn pin_is_marked(marker: Option<&OsStr>) -> bool {
    marker.is_some_and(|value| !value.is_empty())
}

// Undo this launcher's pin for a child, leaving an operator's own VK_DRIVER_FILES untouched.
pub fn drop_display_pin(command: &mut Command) {
    if !pin_is_marked(std::env::var_os(PIN_MARKER).as_deref()) {
        return;
    }
    command.env_remove("VK_DRIVER_FILES");
    command.env_remove("VK_ICD_FILENAMES");
    command.env_remove(PIN_MARKER);
}

// The display GPU's ICD list; AGENTS.md rule 5 says why an index pin cannot do this job.
pub fn display_pin(devices: &[(u32, u32, u32)]) -> DisplayPin {
    display_pin_in(devices, Path::new("/sys/class/drm"), &icd_search_dirs())
}

// Split from display_pin so a test can drive the whole chain against a fake sysfs and icd dir.
fn display_pin_in(devices: &[(u32, u32, u32)], drm: &Path, icd_dirs: &[PathBuf]) -> DisplayPin {
    icd_for_displays(devices, &display_pci_ids(drm), &card_pci_ids(drm), icd_dirs)
}

// What the launcher should do about this box's GPUs, so src/gui.rs can say it in one sentence.
pub enum DisplayPin {
    NotNeeded,
    Unmatched { vendor: u32 },
    Pin { icd: String, gpu: (u32, u32) },
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

// Sample input: devices [(0x8086, 0xa788, 1), (0x10de, 0x27e0, 2)] and displays [(0x10de, 0x27e0)].
fn needs_icd_pin(devices: &[(u32, u32, u32)], displays: &[(u32, u32)], cards: &[(u32, u32)]) -> bool {
    if displays.is_empty() || devices.is_empty() {
        return false;
    }
    let has_display = devices.iter().any(|id| displays.contains(&(id.0, id.1)));
    // A rival must be a real card and a different vendor: a software ICD owns no card, and an ICD list is vendor granular.
    let excludable = devices.iter().any(|id| {
        let pci = (id.0, id.1);
        !displays.contains(&pci)
            && cards.contains(&pci)
            && !displays.iter().any(|(vendor, _)| *vendor == id.0)
    });
    has_display && excludable
}

fn icd_for_displays(devices: &[(u32, u32, u32)], displays: &[(u32, u32)], cards: &[(u32, u32)], icd_dirs: &[PathBuf]) -> DisplayPin {
    if !needs_icd_pin(devices, displays, cards) {
        return DisplayPin::NotNeeded;
    }
    let Some(found) = devices.iter().copied().find(|id| displays.contains(&(id.0, id.1))) else {
        return DisplayPin::NotNeeded;
    };
    let gpu = (found.0, found.1);
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
            let Some(vendor) = vendor_of_library(&lib) else { continue };
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
        DisplayPin::Unmatched { vendor: gpu.0 }
    } else {
        DisplayPin::Pin { icd: files.join(":"), gpu }
    }
}

// A packaged manifest is a few hundred bytes, and jsondoc's object parse is quadratic in its keys, so a larger file names nothing.
pub(crate) const MANIFEST_BYTES: usize = 4096;

// The loader reads ICD.library_path through cJSON: any case, first repeat. A repeat of either key, which no packaged
// manifest has, names no library, and a member anywhere else, or a string holding the words, changes nothing.
// Sample input: the packaged nvidia_icd.json, whose ICD.library_path is libGLX_nvidia.so.0.
pub(crate) fn icd_library(body: &str) -> Option<String> {
    // The loader refuses a manifest that does not parse, so a library named in one drives nothing.
    if body.len() > MANIFEST_BYTES || crate::jsondoc::parse(body).is_err() {
        return None;
    }
    let library = crate::jsonmembers::only(crate::jsonmembers::only(body, "ICD")?, "library_path")?;
    // The loader opens the path up to its first NUL, so a path holding one names a library this cannot tell.
    crate::jsondoc::parse(library).ok()?.as_str().filter(|path| !path.contains('\0')).map(str::to_owned)
}

// One name test for the hasvk ICD, which the display pin skips.
// Sample input: "libvulkan_intel_hasvk.so", and "libvulkan_intel.so" for the ANV ICD beside it.
fn is_hasvk_library(lib: &str) -> bool {
    lib.to_ascii_lowercase().contains("hasvk")
}

// Mesa's hasvk drives Gen7 and Gen8 Intel (Ivy Bridge to Broadwell, Bay Trail), whose ids all sit below this one.
const HASVK_ID_CEILING: u32 = 0x1800;
// Cherryview and Braswell are Gen8 on hasvk but carry ids above the ceiling.
const CHERRYVIEW_IDS: [u32; 4] = [0x22b0, 0x22b1, 0x22b2, 0x22b3];
// Broxton's first id is Gen9, which ANV drives, although it sits below the ceiling.
const BROXTON_BELOW_CEILING: u32 = 0x0a84;

// Sample input: (0x8086, 0x0412) Haswell GT2 and (0x8086, 0x22b0) Cherryview true, (0x8086, 0xa788) false.
fn is_hasvk_intel(id: (u32, u32)) -> bool {
    if id.0 != PCI_VENDOR_INTEL {
        return false;
    }
    if CHERRYVIEW_IDS.contains(&id.1) {
        return true;
    }
    id.1 < HASVK_ID_CEILING && id.1 != BROXTON_BELOW_CEILING
}

// Issue #160: hasvk draws garbled text, so a box whose every real GPU is a hasvk Intel GPU starts on OpenGL.
// Sample input: [(0x8086, 0x0412, 1)] for a Haswell-only box, plus (0x10de, 0x27e0, 2) beside it.
pub fn hasvk_only(devices: &[(u32, u32, u32)]) -> bool {
    let mut real = false;
    for id in devices {
        if id.2 == DEVICE_TYPE_CPU {
            continue;
        }
        if !is_hasvk_intel((id.0, id.1)) {
            return false;
        }
        real = true;
    }
    real
}

// Sample input: "libGLX_nvidia.so.0", "libvulkan_intel.so", "libvulkan_intel_hasvk.so".
fn vendor_of_library(lib: &str) -> Option<u32> {
    let lib = lib.to_ascii_lowercase();
    // hasvk is Haswell/Broadwell, not the display GPU beside a modern iGPU ICD.
    if is_hasvk_library(&lib) {
        return None;
    }
    if lib.contains("nvidia") || lib.contains("nouveau") {
        Some(PCI_VENDOR_NVIDIA)
    } else if lib.contains("intel") {
        Some(PCI_VENDOR_INTEL)
    } else if lib.contains("radeon") || lib.contains("amdvlk") || lib.contains("amd_") {
        Some(PCI_VENDOR_AMD)
    } else if lib.contains("virtio") {
        Some(PCI_VENDOR_VIRTIO)
    } else {
        None
    }
}

// Sample input: a sysfs drm class where card0 and card1 are cards and card1-DP-1 is a connector.
fn card_pci_ids(drm: &Path) -> Vec<(u32, u32)> {
    let Ok(entries) = std::fs::read_dir(drm) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !is_card(&name) {
            continue;
        }
        let device = entry.path().join("device");
        let vendor = parse_hex_id(&std::fs::read_to_string(device.join("vendor")).unwrap_or_default());
        let id = parse_hex_id(&std::fs::read_to_string(device.join("device")).unwrap_or_default());
        let Some(pair) = vendor.zip(id) else { continue };
        if !ids.contains(&pair) {
            ids.push(pair);
        }
    }
    ids
}

// Sample input: "card1" is a card, "card1-DP-1" is one of its connectors, "renderD128" is a render node.
pub(crate) fn is_card(name: &str) -> bool {
    match name.strip_prefix("card") {
        Some(digits) => !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

// corner: a connected connector is the proxy for the compositor's render device, so a lid-closed box wired to the other GPU needs the operator's own VK_DRIVER_FILES.
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
pub(crate) fn parse_hex_id(raw: &str) -> Option<u32> {
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
    // VkPhysicalDeviceType for a real GPU, the kinds a hasvk decision keeps.
    const DEVICE_TYPE_INTEGRATED: u32 = 1;
    const DEVICE_TYPE_DISCRETE: u32 = 2;
    use std::io::Write;

    // A build box with no Vulkan loader answers from the dlopen arm, which the two tests below cannot read, so they say so and skip.
    fn skipped_without_loader(test: &str) -> bool {
        let loaded = unsafe { !dlopen(LIBVULKAN.as_ptr(), RTLD_NOW).is_null() };
        if !loaded {
            std::io::stderr().write_all(format!("SKIP {}: this box has no libvulkan.so.1\n", test).as_bytes()).ok();
        }
        !loaded
    }

    // The error names the call that refused, so this can no longer pass from the dlopen or dlsym branch.
    #[test]
    fn a_required_extension_no_loader_offers_reads_unusable() {
        if skipped_without_loader("vulkan::tests::a_required_extension_no_loader_offers_reads_unusable") {
            return;
        }
        let absent = c"VK_KHR_flea_probe_extension_that_cannot_exist";
        let alone = usable_with(&[absent]).unwrap_err();
        assert!(alone.starts_with("vkCreateInstance answered"), "{alone}");
        let beside = usable_with(&[SURFACE, absent]).unwrap_err();
        assert!(beside.starts_with("vkCreateInstance answered"), "{beside}");
    }

    // A refusal the operator cannot read is the defect: every arm names the call or library that refused, and the two that asked for extensions name them.
    #[test]
    fn the_refusal_names_the_call_and_the_extension_it_was_asked_for() {
        if skipped_without_loader("vulkan::tests::the_refusal_names_the_call_and_the_extension_it_was_asked_for") {
            return;
        }
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
    fn usable_reports_the_connected_card_pci_id() {
        let displays = display_pci_ids(Path::new("/sys/class/drm"));
        // corner: one connected card is the only shape where a rival GPU cannot explain a mismatch.
        if displays.len() != 1 {
            return;
        }
        let Ok(ids) = usable() else {
            return;
        };
        assert!(ids.iter().any(|id| (id.0, id.1) == displays[0]), "devices {ids:?} displays {displays:?}");
    }

    #[test]
    fn parse_hex_id_accepts_sysfs_and_bare_forms() {
        assert_eq!(parse_hex_id("0x8086\n"), Some(PCI_VENDOR_INTEL));
        assert_eq!(parse_hex_id("0XA788"), Some(0xa788));
        assert_eq!(parse_hex_id("10de"), Some(PCI_VENDOR_NVIDIA));
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
        assert_eq!(icd_library(body).as_deref(), Some("libGLX_nvidia.so.0"));
        let lower = "{\"icd\":{\"Library_Path\":\"libvulkan_radeon.so\"}}";
        assert_eq!(icd_library(lower).as_deref(), Some("libvulkan_radeon.so"), "cJSON matches either key in any case");
        let metadata = "{\"comment\":\"library_path caf\\u00e9\",\"ICD\":{\"library_path\":\"libvulkan_radeon.so\",\"note\":\"library_path\"}}";
        assert_eq!(icd_library(metadata).as_deref(), Some("libvulkan_radeon.so"), "only the two keys can be ambiguous");
        for ambiguous in [
            "{\"ICD\":{\"Library_Path\":\"libvulkan_radeon.so\",\"library_path\":\"libGLX_nvidia.so.0\"}}",
            "{\"ICD\":{\"library_path\":\"libvulkan_radeon.so\",\"library_path\":\"libGLX_nvidia.so.0\"}}",
            "{\"ICD\":{\"library_path\":\"libvulkan_radeon.so\",\"library\\u005fpath\":\"libGLX_nvidia.so.0\"}}",
            "{\"ICD\":{\"library_path\":\"libvulkan_radeon.so\"},\"ICD\":{\"library_path\":\"libGLX_nvidia.so.0\"}}",
            "{\"ICD\":{\"library_path\\u0000x\":\"libvulkan_radeon.so\",\"library_path\":\"libGLX_nvidia.so.0\"}}",
            "{\"ICD\":{\"library_path\":\"libvulkan_radeon.so\\u0000/../libGLX_nvidia.so.0\"}}",
        ] {
            assert_eq!(icd_library(ambiguous), None, "{ambiguous}");
        }
    }

    #[test]
    fn vendor_of_library_skips_hasvk_and_names_the_rest() {
        assert_eq!(vendor_of_library("libGLX_nvidia.so.0"), Some(PCI_VENDOR_NVIDIA));
        assert_eq!(vendor_of_library("libvulkan_intel.so"), Some(PCI_VENDOR_INTEL));
        assert_eq!(vendor_of_library("libvulkan_nouveau.so"), Some(PCI_VENDOR_NVIDIA));
        assert_eq!(vendor_of_library("libvulkan_intel_hasvk.so"), None);
        assert_eq!(vendor_of_library("libvulkan_radeon.so"), Some(PCI_VENDOR_AMD));
    }

    // Issue #160: hasvk is Mesa's driver for Gen7 and Gen8 Intel; Gen9 and later are ANV's.
    #[test]
    fn hasvk_covers_gen7_and_gen8_intel_and_nothing_newer() {
        assert!(is_hasvk_intel((PCI_VENDOR_INTEL, 0x0412)), "Haswell GT2");
        assert!(is_hasvk_intel((PCI_VENDOR_INTEL, 0x0166)), "Ivy Bridge GT2");
        assert!(is_hasvk_intel((PCI_VENDOR_INTEL, 0x0f31)), "Bay Trail");
        assert!(is_hasvk_intel((PCI_VENDOR_INTEL, 0x1616)), "Broadwell GT2, Gen8");
        assert!(is_hasvk_intel((PCI_VENDOR_INTEL, 0x22b0)), "Cherryview, Gen8 above the ceiling");
        assert!(!is_hasvk_intel((PCI_VENDOR_INTEL, 0x0a84)), "Broxton, Gen9 below the ceiling");
        assert!(!is_hasvk_intel((PCI_VENDOR_INTEL, 0x1916)), "Skylake GT2, Gen9");
        assert!(!is_hasvk_intel((PCI_VENDOR_INTEL, 0xa788)), "a modern iGPU");
        assert!(!is_hasvk_intel((PCI_VENDOR_INTEL, 0x1800)));
        assert!(!is_hasvk_intel((PCI_VENDOR_NVIDIA, 0x0412)));
    }

    // Issue #160: a Haswell-only box draws garbled text on Vulkan, so the launcher falls back.
    #[test]
    fn a_haswell_only_box_is_hasvk_only() {
        assert!(hasvk_only(&[(PCI_VENDOR_INTEL, 0x0412, DEVICE_TYPE_INTEGRATED)]));
    }

    // Issue #160: hasvk beside another usable device keeps Vulkan, the pin still decides the ICD.
    #[test]
    fn hasvk_beside_another_device_is_not_hasvk_only() {
        assert!(!hasvk_only(&[(PCI_VENDOR_INTEL, 0x0412, DEVICE_TYPE_INTEGRATED), (PCI_VENDOR_NVIDIA, 0x27e0, DEVICE_TYPE_DISCRETE)]));
        assert!(!hasvk_only(&[(PCI_VENDOR_NVIDIA, 0x27e0, DEVICE_TYPE_DISCRETE)]));
        assert!(!hasvk_only(&[(PCI_VENDOR_INTEL, 0xa788, DEVICE_TYPE_INTEGRATED)]));
    }

    // A CPU device beside hasvk is still hasvk-only: lavapipe reports VK_PHYSICAL_DEVICE_TYPE_CPU.
    // Sample input: [(0x8086, 0x0412, 1), (0x10005, 0, 4)] answers true.
    #[test]
    fn a_cpu_device_beside_hasvk_is_still_hasvk_only() {
        assert!(hasvk_only(&[(PCI_VENDOR_INTEL, 0x0412, DEVICE_TYPE_INTEGRATED), (0x10005, 0, DEVICE_TYPE_CPU)]));
        assert!(!hasvk_only(&[(PCI_VENDOR_INTEL, 0x0412, DEVICE_TYPE_INTEGRATED), (PCI_VENDOR_NVIDIA, 0x27e0, DEVICE_TYPE_DISCRETE)]));
        assert!(!hasvk_only(&[(0x10005, 0, DEVICE_TYPE_CPU)]));
    }

    // usable() errors on zero devices, so an empty list never reaches the fallback; it reads as not.
    #[test]
    fn no_device_is_never_hasvk_only() {
        assert!(!hasvk_only(&[]));
    }

    // The minipc ICD pair, read the way the pin reads them: hasvk is recognised and skipped.
    #[test]
    fn the_minipc_icd_pair_names_hasvk_and_skips_it() {
        let hasvk = "{\"ICD\":{\"library_path\":\"libvulkan_intel_hasvk.so\"}}\n";
        let intel = "{\"ICD\":{\"library_path\":\"libvulkan_intel.so\"}}\n";
        assert!(is_hasvk_library(&icd_library(hasvk).unwrap()));
        assert!(!is_hasvk_library(&icd_library(intel).unwrap()));
        assert_eq!(vendor_of_library(&icd_library(hasvk).unwrap()), None);
        assert_eq!(vendor_of_library(&icd_library(intel).unwrap()), Some(PCI_VENDOR_INTEL));
    }

    // Hybrid: Vulkan sees Intel and NVIDIA, only NVIDIA has a panel, so the Intel ICD has to go.
    #[test]
    fn a_gpu_with_no_display_needs_the_display_icd() {
        let intel = (PCI_VENDOR_INTEL, 0xa788, DEVICE_TYPE_INTEGRATED);
        let nvidia = (PCI_VENDOR_NVIDIA, 0x27e0, DEVICE_TYPE_DISCRETE);
        let intel_pci = (PCI_VENDOR_INTEL, 0xa788);
        let nvidia_pci = (PCI_VENDOR_NVIDIA, 0x27e0);
        let cards = [intel_pci, nvidia_pci];
        assert!(needs_icd_pin(&[intel, nvidia], &[nvidia_pci], &cards));
        assert!(needs_icd_pin(&[intel, nvidia], &[intel_pci], &cards));
        assert!(!needs_icd_pin(&[intel, nvidia], &[intel_pci, nvidia_pci], &cards));
        assert!(!needs_icd_pin(&[nvidia], &[nvidia_pci], &cards));
        assert!(!needs_icd_pin(&[intel, nvidia], &[], &cards));
    }

    #[test]
    fn icd_for_displays_picks_the_nvidia_json_and_not_hasvk() {
        let root = fixture_root("icd-pick");
        std::fs::write(root.join("nvidia_icd.json"), "{\"comment\":\"library_path caf\\u00e9\",\"ICD\":{\"library_path\":\"libGLX_nvidia.so.0\"}}\n").unwrap();
        std::fs::write(root.join("intel_icd.json"), "{\"ICD\":{\"library_path\":\"libvulkan_intel.so\"}}\n").unwrap();
        std::fs::write(root.join("intel_hasvk_icd.json"), "{\"ICD\":{\"library_path\":\"libvulkan_intel_hasvk.so\"}}\n").unwrap();
        let intel = (PCI_VENDOR_INTEL, 0xa788, DEVICE_TYPE_INTEGRATED);
        let nvidia = (PCI_VENDOR_NVIDIA, 0x27e0, DEVICE_TYPE_DISCRETE);
        let picked = icd_for_displays(&[intel, nvidia], &[(PCI_VENDOR_NVIDIA, 0x27e0)], &[(PCI_VENDOR_INTEL, 0xa788), (PCI_VENDOR_NVIDIA, 0x27e0)], &[root.clone()]);
        let _ = std::fs::remove_dir_all(&root);
        match picked {
            DisplayPin::Pin { icd, .. } => {
                assert!(icd.ends_with("nvidia_icd.json"), "{icd}");
                assert!(!icd.contains("intel"), "{icd}");
            }
            _ => panic!("a hybrid tree must pin the display GPU"),
        }
    }

    #[test]
    fn display_pci_ids_reads_connected_cards_from_a_fake_drm_tree() {
        let root = fixture_root("drm-read");
        write_card(&root, "card0", PCI_VENDOR_INTEL, 0xa788);
        write_connector(&root, "card0-eDP-1", "disconnected");
        write_card(&root, "card1", PCI_VENDOR_NVIDIA, 0x27e0);
        write_connector(&root, "card1-DP-1", "connected");
        write_connector(&root, "card1-eDP-1", "connected");
        let ids = display_pci_ids(&root);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(ids, vec![(PCI_VENDOR_NVIDIA, 0x27e0)]);
    }

    #[test]
    fn a_fake_hybrid_tree_pins_the_display_gpu_icd() {
        let drm = fixture_root("drm-hybrid");
        write_card(&drm, "card0", PCI_VENDOR_INTEL, 0xa788);
        write_connector(&drm, "card0-eDP-1", "disconnected");
        write_card(&drm, "card1", PCI_VENDOR_NVIDIA, 0x27e0);
        write_connector(&drm, "card1-DP-1", "connected");
        let icd = fixture_root("icd-hybrid");
        std::fs::write(icd.join("nvidia_icd.json"), "{\"ICD\":{\"library_path\":\"libGLX_nvidia.so.0\"}}\n").unwrap();
        std::fs::write(icd.join("intel_icd.json"), "{\"ICD\":{\"library_path\":\"libvulkan_intel.so\"}}\n").unwrap();
        let pinned = display_pin_in(&[(PCI_VENDOR_INTEL, 0xa788, DEVICE_TYPE_INTEGRATED), (PCI_VENDOR_NVIDIA, 0x27e0, DEVICE_TYPE_DISCRETE)], &drm, &[icd.clone()]);
        let alone = display_pin_in(&[(PCI_VENDOR_NVIDIA, 0x27e0, DEVICE_TYPE_DISCRETE)], &drm, &[icd.clone()]);
        let _ = std::fs::remove_dir_all(&drm);
        let _ = std::fs::remove_dir_all(&icd);
        match pinned {
            DisplayPin::Pin { icd, gpu } => {
                assert!(icd.ends_with("nvidia_icd.json"), "{icd}");
                assert!(!icd.contains("intel"), "{icd}");
                assert_eq!(gpu, (PCI_VENDOR_NVIDIA, 0x27e0));
            }
            _ => panic!("a hybrid tree must pin the display GPU"),
        }
        assert!(matches!(alone, DisplayPin::NotNeeded));
    }

    #[test]
    fn an_exported_but_empty_pin_marker_is_absent_not_a_pin() {
        assert!(!pin_is_marked(None));
        assert!(!pin_is_marked(Some(OsStr::new(""))));
        assert!(pin_is_marked(Some(OsStr::new("1"))));
    }

    #[test]
    fn a_vulkan_device_that_owns_no_drm_card_is_not_a_rival_gpu() {
        let intel = (PCI_VENDOR_INTEL, 0x3e92, DEVICE_TYPE_INTEGRATED);
        let lavapipe = (0x10005, 0x0, DEVICE_TYPE_CPU);
        assert!(!needs_icd_pin(&[intel, lavapipe], &[(PCI_VENDOR_INTEL, 0x3e92)], &[(PCI_VENDOR_INTEL, 0x3e92)]));
    }

    #[test]
    fn a_same_vendor_rival_cannot_be_excluded_by_an_icd_list() {
        let igpu = (PCI_VENDOR_AMD, 0x164e, DEVICE_TYPE_INTEGRATED);
        let dgpu = (PCI_VENDOR_AMD, 0x744c, DEVICE_TYPE_DISCRETE);
        assert!(!needs_icd_pin(&[igpu, dgpu], &[(PCI_VENDOR_AMD, 0x164e)], &[(PCI_VENDOR_AMD, 0x164e), (PCI_VENDOR_AMD, 0x744c)]));
    }

    #[test]
    fn a_display_gpu_with_no_known_icd_is_reported_rather_than_pinned() {
        let icd = fixture_root("icd-unknown");
        std::fs::write(icd.join("mystery_icd.json"), "{\"ICD\":{\"library_path\":\"libvulkan_mystery.so\"}}\n").unwrap();
        let answer = icd_for_displays(
            &[(PCI_VENDOR_INTEL, 0xa788, DEVICE_TYPE_INTEGRATED), (PCI_VENDOR_NVIDIA, 0x27e0, DEVICE_TYPE_DISCRETE)],
            &[(PCI_VENDOR_NVIDIA, 0x27e0)],
            &[(PCI_VENDOR_INTEL, 0xa788), (PCI_VENDOR_NVIDIA, 0x27e0)],
            &[icd.clone()],
        );
        let _ = std::fs::remove_dir_all(&icd);
        assert!(matches!(answer, DisplayPin::Unmatched { vendor: PCI_VENDOR_NVIDIA }));
    }

    // A fixture root this test created itself, so a pre-planted symlink cannot divert its writes.
    fn fixture_root(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("flea-{name}-{}-{unique}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        root
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
