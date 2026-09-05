.pragma library

// Only src/gui.rs's own implicit Vulkan choice may retry, and the argv is what it relaunches Flea with.
// Sample input: ("vulkan", "1", "/usr/bin/flea"), and ("opengl", "1", "/usr/bin/flea") for a launch that may not.
function fallbackCommand(backend, automatic, bin) {
    if (backend !== "vulkan" || automatic !== "1")
        return null
    return ["/usr/bin/env", "QSG_RHI_BACKEND=opengl", bin || "flea", "--gui"]
}
