// PCI vendor ids, ICD manifest reads and DRM node names the Vulkan ICD filter shares, plus the pin marker older launchers set.
use std::ffi::OsStr;
use std::process::Command;

// PCI vendor ids, the values /sys/class/drm/card*/device/vendor carries.
pub(crate) const PCI_VENDOR_NVIDIA: u32 = 0x10de;
pub(crate) const PCI_VENDOR_INTEL: u32 = 0x8086;
pub(crate) const PCI_VENDOR_AMD: u32 = 0x1002;

// An older launcher marked its own pin, so a program Flea starts can tell it from the operator's own list.
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

// A packaged manifest is a few hundred bytes, and jsondoc's object parse is quadratic in its keys, so a larger file names nothing.
pub(crate) const MANIFEST_BYTES: usize = 4096;

// The loader reads ICD.library_path through cJSON: any case, first repeat; a repeat or a member elsewhere names nothing.
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

// Mesa's hasvk drives Gen7 and Gen8 Intel (Ivy Bridge to Broadwell, Bay Trail), whose ids all sit below this one.
const HASVK_ID_CEILING: u32 = 0x1800;
// Cherryview and Braswell are Gen8 on hasvk but carry ids above the ceiling.
const CHERRYVIEW_IDS: [u32; 4] = [0x22b0, 0x22b1, 0x22b2, 0x22b3];
// Broxton's first id is Gen9, which ANV drives, although it sits below the ceiling.
const BROXTON_BELOW_CEILING: u32 = 0x0a84;

// Sample input: (0x8086, 0x0412) Haswell GT2 and (0x8086, 0x22b0) Cherryview true, (0x8086, 0xa788) false.
pub(crate) fn is_hasvk_intel(id: (u32, u32)) -> bool {
    if id.0 != PCI_VENDOR_INTEL {
        return false;
    }
    if CHERRYVIEW_IDS.contains(&id.1) {
        return true;
    }
    id.1 < HASVK_ID_CEILING && id.1 != BROXTON_BELOW_CEILING
}

// Sample input: "card1" is a card, "card1-DP-1" is one of its connectors, "renderD128" is a render node.
pub(crate) fn is_card(name: &str) -> bool {
    match name.strip_prefix("card") {
        Some(digits) => !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

pub(crate) fn is_render_node(name: &str) -> bool {
    name.strip_prefix("renderD")
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
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

    #[test]
    fn parse_hex_id_accepts_sysfs_and_bare_forms() {
        assert_eq!(parse_hex_id("0x8086\n"), Some(PCI_VENDOR_INTEL));
        assert_eq!(parse_hex_id("0XA788"), Some(0xa788));
        assert_eq!(parse_hex_id("10de"), Some(PCI_VENDOR_NVIDIA));
        assert_eq!(parse_hex_id("not-a-pci-id"), None);
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

    #[test]
    fn an_exported_but_empty_pin_marker_is_absent_not_a_pin() {
        assert!(!pin_is_marked(None));
        assert!(!pin_is_marked(Some(OsStr::new(""))));
        assert!(pin_is_marked(Some(OsStr::new("1"))));
    }
}
