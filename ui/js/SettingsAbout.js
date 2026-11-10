.pragma library
.import "Update.js" as Update
.import "MakeDefault.js" as MakeDefault

// The Settings panel's About section: what is installed, the one updater, and where help lives; it came out of ui/js/Settings.js whole when the Updates group grew.

// The two rows that leave Flea for a web page carry their destination, so ui/SettingsPanel.qml opens whatever a row names.
var ISSUES_URL = "https://github.com/thisisgm/flea/issues"
var SPONSORS_URL = "https://github.com/sponsors/thisisgm"

// state.about is ui/AboutFacts.qml's facts, the updater's status and the default's claim among them; state.data is ui.json.
function rows(state) {
    var facts = state.about || {}
    var out = [
        { kind: "hero", label: "Flea", value: "A file manager for Omarchy" },
        { kind: "fact", label: "Version", value: facts.version || "Not reported" },
        { kind: "fact", label: "Built", value: facts.built || "Not recorded in this build" },
        { kind: "fact", label: "Installed from", value: facts.source || "Not reported" },
        { kind: "fact", label: "Package", value: facts.package || "Not reported" },
        { kind: "fact", label: "License", value: "MIT, © 2026 GM" },
        { kind: "group", label: "Language" },
        { kind: "fact", label: "Language", glyph: "globe", value: "English" },
        { kind: "group", label: "Updates" }
    ]
    out = out.concat(updateRows(facts.update || Update.idle(), state.data))
    out.push({ kind: "group", label: "This computer" })
    // Rule 5: the identity is in the tail, so a handler too long for the row loses its head instead.
    out.push({ kind: "fact", label: "File manager", glyph: "folder", elide: "head", value: facts.handler || "Not reported" })
    out = out.concat(MakeDefault.rows(facts.handler, facts.claim))
    out.push({ kind: "action", id: "keyboardSheet", label: "Keyboard sheet", glyph: "keyboard", value: "?" })
    out.push({ kind: "action", id: "reportIssue", label: "Report an issue", glyph: "network", value: "Open", url: ISSUES_URL })
    // SettingsGrammar rule 6: a brand is never given a generic glyph, and the mark set holds no GitHub reproduction.
    out.push({ kind: "action", id: "support", label: "Support Flea", value: "GitHub Sponsors", url: SPONSORS_URL })
    return out
}

// Omarchy installs and Flea may check and launch: the row's value is the check, its chevron says Enter acts, and the note explains the states that need it.
function updateRows(update, data) {
    var out = [
        { kind: Update.isFact(update) ? "fact" : "action", id: "updateFlea", label: "Update Flea", glyph: "download",
          value: Update.value(update), role: Update.role(update), inert: !Update.acts(update) },
        { kind: "check", id: "updates.autoCheck", label: "Check automatically", caption: "when About opens, every 6 h",
          on: autoCheck(data) }
    ]
    var note = Update.note(update)
    if (note !== null)
        out.push({ kind: "hint", label: note[0], role: note[1] })
    return out
}

// On until switched off, the default src/uischema.rs ships for updates.autoCheck.
function autoCheck(data) {
    return ((data || {}).updates || {}).autoCheck !== false
}

// The package names src/update.rs keys its kinds on, OPR_PACKAGE and RELEASE_PACKAGE.
var PACKAGES = { opr: "flea", release: "flea-bin" }

// Installed from, one sentence per kind src/update.rs derives, so this row and the Update Flea row name one source.
var SOURCES = { opr: "Omarchy Package Repository",
                local: "Local package", release: "Local package", unowned: "Not from a package" }

// src/update.rs kind_for, over pacman -Qqo's owner ("" when no package owns Flea).
function installKind(owner, signed) {
    if (!owner)
        return "unowned"
    if (owner === PACKAGES.opr && signed)
        return "opr"
    if (owner === PACKAGES.release)
        return "release"
    return "local"
}

function installedFrom(owner, signed) {
    return SOURCES[installKind(owner, signed)]
}

// pacman -Qi under LC_ALL=C, read the way src/update.rs package_info reads it; "" for a query that failed.
// Sample input: "Name            : flea\nVersion         : 0.3.4-1\nBuild Date      : Wed Sep 23 12:40:50 2026\nValidated By    : None\n"
function packageFacts(text) {
    var facts = { package: "", built: "", signed: false }
    var lines = String(text || "").split("\n")
    for (var i = 0; i < lines.length; i++) {
        var cut = lines[i].indexOf(":")
        if (cut < 0)
            continue
        var key = lines[i].substring(0, cut).trim(), value = lines[i].substring(cut + 1).trim()
        if (key === "Name")
            facts.package = value
        else if (key === "Version")
            facts.package = facts.package.length > 0 ? facts.package + " " + value : value
        else if (key === "Build Date")
            facts.built = value
        // One or more methods separated by two spaces, such as "SHA-256 Sum  Signature".
        else if (key === "Validated By")
            facts.signed = value.split(/\s+/).indexOf("Signature") >= 0
    }
    return facts
}
