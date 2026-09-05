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

// VkInstanceCreateInfo. Every pointer is null here: the probe asks the loader for a bare instance,
// with no application info, no layers and no extensions, because that is all a driver check needs.
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

// True only when this box can really start Vulkan: the loader is present, it created an instance,
// and it reported at least one device. A box without a usable driver answers
// VK_ERROR_INCOMPATIBLE_DRIVER at creation, or creates an instance and then lists no device at all.
// corner: the handle is never dlclose()d, because this process execs qs a moment later and
// unloading a driver that was just probed is the fragile path this probe exists to avoid.
pub fn usable() -> bool {
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
            enabled_extension_count: 0,
            pp_enabled_extension_names: null(),
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
