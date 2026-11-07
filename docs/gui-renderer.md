# GUI renderer fallback

The GPUI window tries hardware Vulkan adapters, software Vulkan (Mesa Lavapipe,
provided by `vulkan-swrast`), and then GL (provided by `mesa`). Each adapter must
create a device and configure the actual window surface before it is accepted.
The pinned GPUI fork implements this device and surface check and the GL retry.

The launcher clears Qt renderer settings and drops a legacy `FLEA_VK_PIN` and its
`VK_DRIVER_FILES` and `VK_ICD_FILENAMES` list inherited from an older window.
It applies the absent-vendor ICD filter, but adds no display GPU pin, Lavapipe pin
or Vulkan probe cache; GPUI selects adapters using compositor feedback and the real window surface.
A nonempty operator ICD override remains authoritative.

`QSG_RHI_BACKEND`, `FLEA_RENDERER_AUTOMATIC` and `QT_VK_PHYSICAL_DEVICE_INDEX` are removed
from the GPUI child's environment.
Direct `flea-gui` launches enter the GPUI application without the launcher's environment cleanup.

When no renderer can open the window, `flea-gui` exits with status 1 and reports
this sentence followed by the underlying error to stderr and the system journal:

```text
flea: no graphics driver could open a window; install or repair vulkan-swrast and mesa, or run flea --tui
```

One line follows for each adapter GPUI tried and refused, with wgpu's reason, for example
`llvmpipe (LLVM 23.1.3, 256 bits) (Gl) was refused: Failed to create wgpu device: Limit
'max_storage_buffer_binding_size' value 134217728 is better than allowed 0`.

When stderr is not a terminal, as in a launch from the app launcher, a key binding or the
file chooser portal, `flea-gui` also shows the reason as a critical desktop notification through
`notify-send`, and waits at most two seconds for it to be delivered. Its summary and body come
from the `window.no_driver` and `window.no_driver_hint` catalog keys in the desktop's language
(English summary `No graphics driver could open Flea`), and stay English when no catalog loads.
The stderr and journal lines stay English.

A GPU with no Vulkan driver cannot use the GL fallback unless its OpenGL driver has storage
buffers (OpenGL 4.3, OpenGL ES 3.1 or `GL_ARB_shader_storage_buffer_object`), because GPUI
requests a device with them. Intel HD Graphics 2000 and 3000 (Sandy Bridge) offer OpenGL 3.3
and OpenGL ES 3.0 without that extension. `vulkan-swrast` is an optional dependency for these
GPUs; with it installed the window draws on the CPU through Lavapipe.

The launcher execs `flea-gui`, so it preserves status 1 on renderer failure. Journal delivery uses
the local syslog socket, with the native journal socket as a fallback. A system
without either socket still receives both messages on stderr and status 1.
The failure branch reports, flushes stdout and stderr, and exits directly from
the startup callback, so termination never depends on the event loop starting.

`tests/gui-render-fallback.sh` builds the real GUI and runs seven isolated driver
cases through both direct `flea-gui` (the unchanged fork renderer) and the launcher
under Hyprland on a headless output, through the existing `start-hyprland`
launcher used by the CI visual lane: Lavapipe, no Vulkan ICD,
a real Mesa ICD whose device creation is forced to fail, GL only, and neither,
plus two with Mesa's GL held to a Sandy Bridge GPU's (OpenGL 3.3, OpenGL ES 3.0 and no
`GL_ARB_shader_storage_buffer_object`, through Mesa's version and extension overrides):
Lavapipe present, and no Vulkan ICD. A stand-in `notify-send` on `PATH` records the
desktop notification, which each failing case must send once and each opened window never.
Each failing case must also report GPUI's reason and its refused adapters, the Gen6 one
refused for storage buffers.
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
