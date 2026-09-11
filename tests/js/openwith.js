.import "../../ui/js/OpenWith.js" as OpenWith
.import "../../ui/js/Icons.js" as Icons

function app(id, label, isDefault) { return { id: id, label: label, icon: id, default: isDefault === true } }

var HANDLERS = [app("viewer.desktop", "Image Viewer", true), app("chromium.desktop", "Chromium"),
                app("pinta.desktop", "Pinta")]
var INSTALLED = [app("1password.desktop", "1Password"), app("chromium.desktop", "Chromium"),
                 app("pinta.desktop", "Pinta"), app("viewer.desktop", "Image Viewer")]

function labels(rows) {
    return rows.map(function (row) { return row.eyebrow !== undefined ? "[" + row.eyebrow + "]" : row.label }).join(",")
}

function run(check) {
    var full = OpenWith.rows(HANDLERS, INSTALLED, "PNG image", "")
    check("both groups draw under their own eyebrow", labels(full),
          "[Registered for PNG image],Image Viewer,Chromium,Pinta,[All applications],1Password,Chromium,Pinta,Image Viewer")
    check("only the second eyebrow carries the rule that separates the groups",
          full.filter(function (row) { return row.rule === true }).length, 1)
    check("the cursor counts applications and never an eyebrow", OpenWith.applications(full).length, 7)
    check("every application row knows the seat it holds",
          OpenWith.applications(full).map(function (row, at) { return row.at === at }).indexOf(false), -1)
    check("a registered application drawn twice takes two seats",
          full.filter(function (row) { return row.id === "chromium.desktop" }).map(function (row) { return row.at }).join(","), "1,4")
    check("the seat maps back to the row it was drawn in", OpenWith.rowOf(full, 3), 5)

    var narrowed = OpenWith.rows(HANDLERS, INSTALLED, "PNG image", "pin")
    check("the search filters both groups", labels(narrowed), "[Registered for PNG image],Pinta,[All applications],Pinta")
    var installedOnly = OpenWith.rows(HANDLERS, INSTALLED, "PNG image", "1pass")
    check("an empty registered group takes its eyebrow with it", labels(installedOnly), "[All applications],1Password")
    check("the surviving eyebrow drops the rule it no longer separates", installedOnly[0].rule, false)
    check("the search ignores case and surrounding space", labels(OpenWith.rows([], INSTALLED, "PNG image", "  CHROM ")),
          "[All applications],Chromium")
    check("nothing matched leaves no row at all", OpenWith.rows(HANDLERS, INSTALLED, "PNG image", "zzq").length, 0)
    check("the caption names the search and the way back", OpenWith.noMatch("zzq"),
          "No application matches “zzq”. Clear the search to see every installed application.")

    // Rule 5: seven applications, and the eyebrows standing over them, decide where the list is cut.
    check("seven applications and two eyebrows set the viewport", OpenWith.viewportHeight(full, 7, 37, 28), 7 * 37 + 2 * 28)
    check("the first seven stop before an eighth application", OpenWith.viewportHeight(full, 3, 37, 28), 3 * 37 + 28)
    check("a catalogue shorter than the viewport still fills it",
          OpenWith.viewportHeight(OpenWith.rows([], [INSTALLED[0]], "PNG image", ""), 7, 37, 28), 7 * 37 + 28)
    check("rule 6 keeps that height when nothing matches", OpenWith.viewportHeight([], 7, 37, 28), 7 * 37)

    check("the tail row and the entry with no themed icon draw the same window",
          Icons.pathFor("app-window"), "M3 4h18v16H3z M3 9h18 M6 6.5h.01 M9 6.5h.01")
}
