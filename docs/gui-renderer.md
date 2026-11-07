# GUI renderer fallback

The GPUI window tries hardware Vulkan adapters, software Vulkan (Mesa Lavapipe,
provided by `vulkan-swrast`), and then GL (provided by `mesa`). Each adapter must
create a device and configure the actual window surface before it is accepted.
The pinned GPUI fork implements this device and surface check and the GL retry.

The launcher clears Qt renderer settings and drops a legacy `FLEA_VK_PIN` and its
`VK_DRIVER_FILES` and `VK_ICD_FILENAMES` list inherited from an older window.
It applies the ICD filter for absent vendors and an idle NVIDIA GPU, but adds no display GPU pin, Lavapipe pin
or Vulkan probe cache; GPUI selects adapters using compositor feedback and the real window surface.
A nonempty operator ICD override remains authoritative.

On a hybrid laptop the filter also disables NVIDIA's Vulkan drivers while the NVIDIA GPU
drives no display: no NVIDIA connector is connected, another GPU's connector is, that GPU has
its own Vulkan driver installed, and the compositor's first `AQ_DRM_DEVICES` or `WLR_DRM_DEVICES` entry, when set, is that other GPU.
Loading those drivers would wake the dGPU from runtime suspend for a window that never
renders on it. `FLEA_VK_KEEP_NVIDIA=1`, or a PRIME offload request
(`__NV_PRIME_RENDER_OFFLOAD=1`, `__VK_LAYER_NV_optimus=NVIDIA_only`, or a `DRI_PRIME` other
than `0`), keeps them.
Programs Flea opens never inherit the filter.

`QSG_RHI_BACKEND`, `FLEA_RENDERER_AUTOMATIC` and `QT_VK_PHYSICAL_DEVICE_INDEX` are removed
from the GPUI child's environment.
Direct `flea-gui` launches enter the GPUI application without the launcher's environment cleanup.

When no renderer can open the window, `flea-gui` exits with status 1 and reports
this sentence followed by the underlying error to stderr and the system journal:

```text
flea: no graphics driver could open a window; install or repair vulkan-swrast and mesa, or run flea --tui
```

The launcher execs `flea-gui`, so it preserves status 1 on renderer failure. Journal delivery uses
the local syslog socket, with the native journal socket as a fallback. A system
without either socket still receives both messages on stderr and status 1.
The failure branch reports, flushes stdout and stderr, and exits directly from
the startup callback, so termination never depends on the event loop starting.

`tests/gui-render-fallback.sh` builds the real GUI and runs five isolated driver
cases through both direct `flea-gui` (the unchanged fork renderer) and the launcher
under Hyprland on a headless output, through the existing `start-hyprland`
launcher used by the CI visual lane: Lavapipe, no Vulkan ICD,
a real Mesa ICD whose device creation is forced to fail, GL only, and neither.
It selects each client's Vulkan ICD inventory and EGL vendor through the loader
environment, changes no installed packages, and captures journal messages on a private socket. It reports the
selected adapter, first submitted Wayland buffer time, and whether the compositor
received that buffer. Times include namespace setup and, for launcher runs, the
launcher. The direct path uses the same diagnostic and failure reporting wrapper
as the launcher; it is a renderer baseline, not a copy of the old GUI executable. Empty Vulkan
override variables alone do not hide ICDs; the no-ICD case points both loader
variables at a marked directory containing no ICD manifests. Missing Hyprland,
its existing starter, Mesa or build prerequisites fail the suite explicitly.
`FLEA_GUI_RENDER_DIAGNOSTICS=1` prints the selected device and driver to stderr.
