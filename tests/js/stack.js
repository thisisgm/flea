.import "../../ui/js/Stack.js" as Stack

function run(check) {
    check("the token is the place", Stack.isStack("flea:stack"), true)
    check("a directory is not the place", Stack.isStack("/home/gm"), false)
    check("an empty path is not the place", Stack.isStack(""), false)

    check("a listpaths name becomes its absolute path", Stack.rowPath("home/gm/a.txt"), "/home/gm/a.txt")
    check("an absolute name stays absolute", Stack.rowPath("/tmp/a.txt"), "/tmp/a.txt")
    check("an empty name stays empty", Stack.rowPath(""), "")

    check("the leaf is the file", Stack.leaf("home/gm/notes/a.txt"), "a.txt")
    check("the place is the parent, written with a tilde inside home",
          Stack.place("home/gm/notes/a.txt", "/home/gm"), "~/notes")
    check("a file in home itself names home", Stack.place("home/gm/a.txt", "/home/gm"), "~")
    check("a file outside home keeps the directory", Stack.place("tmp/a.txt", "/home/gm"), "/tmp")

    check("the same order matches", Stack.same(["/a", "/b"], ["/a", "/b"]), true)
    check("a different order does not", Stack.same(["/a", "/b"], ["/b", "/a"]), false)
    check("a different length does not", Stack.same(["/a"], ["/a", "/b"]), false)

    var copied = Stack.copyList(["/a", "/b"])
    check("a variant list becomes a real array", copied.join("|"), "/a|/b")
    copied.push("/c")
    check("the copy does not keep the caller's identity", Stack.copyList(["/a"]).length, 1)

    check("an open in flight lists the paths it was given", Stack.readyAction(true, false, false, true, false), "list")
    check("a changed quiet read reloads", Stack.readyAction(false, true, true, false, false), "reload")
    check("an unchanged quiet read keeps the rows", Stack.readyAction(false, true, true, false, true), "keep")
    check("a read that was not a probe keeps the rows", Stack.readyAction(false, false, true, false, false), "keep")
    check("a probe during a listing keeps the rows", Stack.readyAction(false, true, true, true, false), "keep")

    var cmd = Stack.command("/home/gm", ["--touch", "/tmp/a"])
    check("the helper is python", cmd[0], "python3")
    check("the packaged helper is tried first", cmd[3], "/usr/lib/flea/the-stack.py")
    check("the home copy is the fallback", cmd[4], "/home/gm/.local/share/flea/the-stack.py")
    check("a touch is the helper's own argument", cmd.slice(5).join(" "), "--touch /tmp/a")
    check("a successful open on the place may record a touch", Stack.shouldTouch({ path: "flea:stack" }, "/tmp/a"), true)
    check("a directory open is not a touch", Stack.shouldTouch({ path: "/tmp" }, "/tmp/a"), false)
    check("a relative path is not a touch", Stack.shouldTouch({ path: "flea:stack" }, "a"), false)
}
