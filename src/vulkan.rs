// Quickshell hands QRhi::create a QVulkanInstance it never created, which kills the shell rather than raising.
use std::ffi::{c_void, CStr, OsStr};
use std::os::raw::{c_char, c_int};
use std::ptr::{null, null_mut};

// dlopen(3) RTLD_NOW, so a loader missing an entry point fails here and never at the first call.
const RTLD_NOW: c_int = 2;
// The soname is the only Vulkan file name the loader ABI guarantees.
const LIBVULKAN: &CStr = c"libvulkan.so.1";
// VkResult VK_SUCCESS, the one result that means the call did what was asked.
const VK_SUCCESS: i32 = 0;
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

// vkCreateInstance, vkEnumeratePhysicalDevices and vkDestroyInstance, each reached through dlsym.
type CreateInstance =
    unsafe extern "C" fn(*const InstanceCreateInfo, *const c_void, *mut *mut c_void) -> i32;
type EnumeratePhysicalDevices = unsafe extern "C" fn(*mut c_void, *mut u32, *mut *mut c_void) -> i32;
type DestroyInstance = unsafe extern "C" fn(*mut c_void, *const c_void);

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
pub fn usable() -> Result<(), String> {
    usable_with(&[SURFACE, platform_surface()])
}

// Split from usable() so a test can ask the same question with an extension no loader can offer.
// corner: the handle is never dlclose()d, because this process execs qs a moment later.
fn usable_with(extensions: &[&CStr]) -> Result<(), String> {
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
        destroy(instance, null());
        if listed != VK_SUCCESS {
            return Err(format!("vkEnumeratePhysicalDevices answered {listed}"));
        }
        if devices == 0 {
            return Err(String::from("vkEnumeratePhysicalDevices succeeded and listed no device"));
        }
        Ok(())
    }
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
}
