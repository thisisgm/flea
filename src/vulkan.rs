// Quickshell hands QRhi::create a QVulkanInstance it never created, and an unusable loader turns
// that into a SIGSEGV, so the launcher has to find one before the shell is started at all.
use std::ffi::{c_void, CStr};
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

// VkInstanceCreateInfo. No application info and no layers; the extension list is the one thing the
// probe does fill in, because a bare instance is not what Qt asks the loader for.
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

// std already links the system libc, so the two symbols are declared here rather than taking a crate.
extern "C" {
    fn dlopen(file: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

// vkCreateInstance(pCreateInfo, pAllocator, pInstance), vkEnumeratePhysicalDevices(instance,
// pCount, pDevices) and vkDestroyInstance(instance, pAllocator), each reached through dlsym.
type CreateInstance =
    unsafe extern "C" fn(*const InstanceCreateInfo, *const c_void, *mut *mut c_void) -> i32;
type EnumeratePhysicalDevices = unsafe extern "C" fn(*mut c_void, *mut u32, *mut *mut c_void) -> i32;
type DestroyInstance = unsafe extern "C" fn(*mut c_void, *const c_void);

// The two WSI extensions Qt's Vulkan RHI presents through. QRhi is handed a QVulkanInstance built
// with the platform surface extension, so a loader that can only make a bare instance still crashes.
const SURFACE: &CStr = c"VK_KHR_surface";
const WAYLAND_SURFACE: &CStr = c"VK_KHR_wayland_surface";
const XCB_SURFACE: &CStr = c"VK_KHR_xcb_surface";

// Read from the same variable paths::has_display() prefers, because that is the session Qt will
// pick its own platform plugin from; naming only the Wayland one would refuse an X11 box Qt serves.
fn platform_surface() -> &'static CStr {
    match std::env::var_os("WAYLAND_DISPLAY") {
        Some(value) if !value.is_empty() => WAYLAND_SURFACE,
        _ => XCB_SURFACE,
    }
}

// True only when this box can really start Vulkan the way Qt starts it: the loader is present, it
// created an instance carrying the surface extensions QRhi needs, and it reported at least one
// device. A box without a usable driver answers VK_ERROR_INCOMPATIBLE_DRIVER or
// VK_ERROR_EXTENSION_NOT_PRESENT at creation, or creates an instance and then lists no device.
pub fn usable() -> bool {
    usable_with(&[SURFACE, platform_surface()])
}

// Split from usable() so a test can ask the same question with an extension no loader can offer,
// which is the only deterministic way here to prove a missing extension really answers false.
// corner: the handle is never dlclose()d, because this process execs qs a moment later and
// unloading a driver that was just probed is the fragile path this probe exists to avoid.
fn usable_with(extensions: &[&CStr]) -> bool {
    let names: Vec<*const c_char> = extensions.iter().map(|e| e.as_ptr()).collect();
    unsafe {
        let library = dlopen(LIBVULKAN.as_ptr(), RTLD_NOW);
        if library.is_null() {
            return false;
        }
        let create = dlsym(library, c"vkCreateInstance".as_ptr());
        let enumerate = dlsym(library, c"vkEnumeratePhysicalDevices".as_ptr());
        let destroy = dlsym(library, c"vkDestroyInstance".as_ptr());
        if create.is_null() || enumerate.is_null() || destroy.is_null() {
            return false;
        }
        let create: CreateInstance = std::mem::transmute(create);
        let enumerate: EnumeratePhysicalDevices = std::mem::transmute(enumerate);
        let destroy: DestroyInstance = std::mem::transmute(destroy);

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
        if create(&request, null(), &mut instance) != VK_SUCCESS || instance.is_null() {
            return false;
        }
        let mut devices: u32 = 0;
        let listed = enumerate(instance, &mut devices, null_mut());
        destroy(instance, null());
        listed == VK_SUCCESS && devices > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The control the device count alone could not give: an instance the loader cannot build must
    // read unusable. On a box with no libvulkan at all this passes at dlopen, which is also false.
    #[test]
    fn a_required_extension_no_loader_offers_reads_unusable() {
        assert!(!usable_with(&[c"VK_KHR_flea_probe_extension_that_cannot_exist"]));
        assert!(!usable_with(&[SURFACE, c"VK_KHR_flea_probe_extension_that_cannot_exist"]));
    }

    // The session decides which surface extension Qt will want, so the probe must ask for that one.
    #[test]
    fn the_platform_surface_follows_the_session() {
        assert_eq!(platform_surface(), if std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty()) { WAYLAND_SURFACE } else { XCB_SURFACE });
    }
}
