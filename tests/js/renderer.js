.import "../../ui/js/Renderer.js" as Renderer

// ui/shell.qml's OpenGL retry, which only a real scene-graph failure raises: the guard and the argv are
// driven here so a wrong one reds without one, and tests/ui.sh case_renderer drives the signal itself.

// The argv as one string, so a wrong element and a wrong length both read as one difference.
function command(backend, automatic, bin) {
    var argv = Renderer.fallbackCommand(backend, automatic, bin)
    return argv === null ? "none" : argv.join(" ")
}

function run(check) {
    check("the launcher's own Vulkan choice retries once on OpenGL",
          command("vulkan", "1", "/usr/bin/flea"),
          "/usr/bin/env QSG_RHI_BACKEND=opengl /usr/bin/flea --gui")
    // src/gui.rs removes FLEA_RENDERER_AUTOMATIC on an explicit choice, so Quickshell.env answers null.
    check("an operator's own Vulkan choice is never retried",
          command("vulkan", null, "/usr/bin/flea"), "none")
    check("a marker beside another renderer retries nothing",
          command("opengl", "1", "/usr/bin/flea"), "none")
    check("and neither does the renderer a retry already moved to",
          command("opengl", null, "/usr/bin/flea"), "none")
    // The marker is that exact string and not a truthiness test, because only src/gui.rs writes it.
    check("no other marker value is the launcher's",
          command("vulkan", "0", "/usr/bin/flea"), "none")
    check("an empty FLEA_BIN falls back to the name on PATH",
          command("vulkan", "1", ""),
          "/usr/bin/env QSG_RHI_BACKEND=opengl flea --gui")
    check("and so does an unset one",
          command("vulkan", "1", null),
          "/usr/bin/env QSG_RHI_BACKEND=opengl flea --gui")
}
