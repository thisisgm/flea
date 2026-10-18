# GUI renderer fallback

The GPUI window tries hardware Vulkan adapters, software Vulkan (Mesa Lavapipe,
provided by `vulkan-swrast`), and then GL (provided by `mesa`). Each adapter must
create a device and configure the actual window surface before it is accepted.
The pinned GPUI fork implements this device and surface check and the GL retry.

The launcher keeps the display GPU's automatic ICD pin. It adds the Lavapipe
manifest to that pin so a failed hardware device can still reach software Vulkan
before GL. When the Vulkan probe finds no hardware device, it pins Lavapipe alone
if its manifest is installed. Both `VK_DRIVER_FILES` and `VK_ICD_FILENAMES` name
the chosen manifest list. `FLEA_VK_PIN` marks that list for removal from programs
opened by Flea; a marked list inherited from another Flea launch is dropped and
chosen again. A nonempty operator ICD override remains authoritative, including
an override that deliberately hides Lavapipe. Without a pin, the loader's normal
driver discovery includes the installed Lavapipe driver.

`QSG_RHI_BACKEND` and the old Qt downgrade and retry logic do not choose a renderer
on the GPUI launch path. Direct `flea-gui` launches use the fork's Vulkan then GL
selection with the inherited Vulkan loader environment.

When no renderer can open the window, `flea-gui` exits with status 78 and writes
this single sentence to stderr and the system journal:

```text
flea: no graphics driver could open a window; install or repair vulkan-swrast and mesa, or run flea --tui
```

The launcher execs `flea-gui`, so it preserves status 78. Journal delivery uses
the local syslog socket, with the native journal socket as a fallback. A system
without either socket still receives the sentence on stderr and status 78.
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
