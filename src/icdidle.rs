// An NVIDIA GPU that drives no display is idle, and loading its Vulkan driver wakes it from runtime suspend.
use crate::vulkan::{is_card, parse_hex_id, PCI_VENDOR_NVIDIA};
use std::ffi::{OsStr, OsString};
use std::path::Path;

// The operator keeps NVIDIA's drivers with this set to 1, for a GPUI that should run on the dGPU.
const KEEP_NVIDIA: &str = "FLEA_VK_KEEP_NVIDIA";
// The compositor's own GPU list, primary first: Hyprland reads the first, wlroots compositors the second.
const COMPOSITOR_DEVICES: [&str; 2] = ["AQ_DRM_DEVICES", "WLR_DRM_DEVICES"];

// True only when a non-NVIDIA card lights a display, no NVIDIA card does, and nothing asked for the dGPU.
pub(crate) fn nvidia_idle(var: impl Fn(&str) -> Option<OsString>, drm: &Path) -> bool {
    if var(KEEP_NVIDIA).as_deref() == Some(OsStr::new("1")) || offload_requested(&var) {
        return false;
    }
    for name in COMPOSITOR_DEVICES {
        if let Some(list) = var(name).filter(|value| !value.is_empty()) {
            if !primary_is_other(&list, drm) {
                return false;
            }
        }
    }
    matches!(connected_vendors(drm), Some((true, false)))
}

// A PRIME render offload request names the dGPU only with these values; DRI_PRIME=0 or non_NVIDIA_only ask for the default GPU.
fn offload_requested(var: impl Fn(&str) -> Option<OsString>) -> bool {
    let is = |name: &str, value: &str| var(name).as_deref() == Some(OsStr::new(value));
    let dri_prime = var("DRI_PRIME").is_some_and(|value| !value.is_empty() && value != "0");
    is("__NV_PRIME_RENDER_OFFLOAD", "1") || is("__VK_LAYER_NV_optimus", "NVIDIA_only") || dri_prime
}

// Sample input: "/dev/dri/card1:/dev/dri/card0", or a udev link such as /dev/dri/intel-igpu. Unknown is never "other".
fn primary_is_other(list: &OsString, drm: &Path) -> bool {
    let Some(first) = std::env::split_paths(list).next() else { return false };
    let resolved = std::fs::canonicalize(&first).unwrap_or(first);
    let Some(name) = resolved.file_name().and_then(|name| name.to_str()) else { return false };
    is_card(name) && card_vendor(drm, name).is_some_and(|vendor| vendor != PCI_VENDOR_NVIDIA)
}

// (a non-NVIDIA connector is connected, an NVIDIA connector is connected), or None when a connector's card is unreadable.
// Sample input: /sys/class/drm/card1-eDP-1/status "connected" with card1/device/vendor 0x8086.
fn connected_vendors(drm: &Path) -> Option<(bool, bool)> {
    let (mut other, mut nvidia) = (false, false);
    for entry in std::fs::read_dir(drm).ok()? {
        let name = entry.ok()?.file_name();
        let name = name.to_string_lossy();
        let Some((card, _)) = name.split_once('-') else { continue };
        if !is_card(card) {
            continue;
        }
        let status = std::fs::read_to_string(drm.join(&*name).join("status")).unwrap_or_default();
        if status.trim() != "connected" {
            continue;
        }
        if card_vendor(drm, card)? == PCI_VENDOR_NVIDIA {
            nvidia = true;
        } else {
            other = true;
        }
    }
    Some((other, nvidia))
}

fn card_vendor(drm: &Path, card: &str) -> Option<u32> {
    parse_hex_id(&std::fs::read_to_string(drm.join(card).join("device/vendor")).ok()?)
}

#[cfg(test)]
#[path = "icdidle_tests.rs"]
mod tests;
