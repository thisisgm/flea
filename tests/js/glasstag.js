.import "../../ui/js/GlassTag.js" as GlassTag

function run(check) {
    var clients = '[{"address":"0xabc1","pid":42,"class":"com.thisisgm.flea"},{"address":"0xdef2","pid":7,"class":"org.gnome.Nautilus"},{"address":"abc3","pid":42}]'
    check("only this process's windows with a well-formed address are asked about", GlassTag.ownAddresses(clients, 42).join(","), "0xabc1")
    check("a reply that is not a list asks about nothing",
          GlassTag.ownAddresses('{"bool":false}', 42).length + "|" + GlassTag.ownAddresses("not json", 42).length, "0|0")
    check("bare Transparent with Hyprland at 1 takes the hotkey's tag", GlassTag.decide("transparent", true, 1, 1, false), "add")
    check("a window Hyprland already draws below 1, as stable Omarchy draws every window, keeps its rule",
          GlassTag.decide("transparent", true, 0.985, 0.96, false), "keep")
    check("a window Flea tagged keeps it while Hyprland draws it at the rule's opacity",
          GlassTag.decide("transparent", true, 0.985, 0.96, true), "keep")
    check("a window recorded as tagged that Hyprland draws at 1, such as a new window on a reused address, is tagged again",
          GlassTag.decide("transparent", true, 1, 1, true), "add")
    check("Glassy and Opaque take Flea's own tag off",
          GlassTag.decide("glassy", true, 0.985, 0.96, true) + "|" + GlassTag.decide("opaque", true, 0.985, 0.96, true), "remove|remove")
    check("a [flea] value outranks the mode, so the tag comes off and none is added",
          GlassTag.decide("transparent", false, 0.985, 0.96, true) + "|" + GlassTag.decide("transparent", false, 1, 1, false), "remove|keep")
    check("a tag Flea did not set is never taken away", GlassTag.decide("glassy", true, 0.985, 0.96, false), "keep")
    check("an unreadable property adds nothing", GlassTag.decide("transparent", true, NaN, NaN, false), "keep")
    var p = GlassTag.plan("0xabc1 1 1\n0xdef2 0.985 0.96\nprop not found\n", "transparent", true, {})
    check("PROBE's lines plan one add and skip the rest", p.pairs.join(",") + "|" + Object.keys(p.tagged).join(","), "0xabc1,+transparent|0xabc1")
    var q = GlassTag.plan("0xabc1 0.985 0.96\n", "glassy", true, p.tagged)
    check("a switch to Glassy plans the removal and forgets the window", q.pairs.join(",") + "|" + Object.keys(q.tagged).length, "0xabc1,-transparent|0")
    var r = GlassTag.plan("0xabc1 0.985 0.96\n", "transparent", true, p.tagged)
    check("a window Flea already tagged plans nothing more", r.pairs.length + "|" + Object.keys(r.tagged).join(","), "0|0xabc1")
    var t = GlassTag.plan("0xabc6 1 1\n", "transparent", true, p.tagged)
    check("a closed window's record drops out and a new window is tagged", t.pairs.join(",") + "|" + Object.keys(t.tagged).join(","), "0xabc6,+transparent|0xabc6")
}
