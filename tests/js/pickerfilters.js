.import "../../ui/js/PickerFilters.js" as Filters

// A filters.toml as a user would write one: two tables, one named, comments and a stray table.
var CONFIG = "# Pills for apps that send no filter of their own.\n"
    + "\n"
    + "[[filter]]\n"
    + "globs = [\"*.jpg\", \"*.jpeg\"]\n"
    + "mimes = [\"image/*\"]\n"
    + "\n"
    + "[[filter]]\n"
    + "name = \"Documents\"\n"
    + "globs = ['*.doc', '*.docx', '*.odt']\n"
    + "\n"
    + "[[filter]]\n"
    + "name = \"No globs, so skipped\"\n"
    + "mimes = [\"text/*\"]\n"
    + "\n"
    + "[other]\n"
    + "globs = [\"*.pdf\"]\n"
    + "\n"
    + "[[filter]]\n"
    + "globs = [\"*.pdf\"]   # one glob\n"

function run(check) {
    var parsed = Filters.parse(CONFIG)
    check("every table with globs is a filter", parsed.length, 3)
    check("globs are read in order", parsed[0].globs.join(","), "*.jpg,*.jpeg")
    check("mimes are read", parsed[0].mimes.join(","), "image/*")
    check("a table without mimes has none", parsed[1].mimes.length, 0)
    check("single quotes are strings too", parsed[1].globs.join(","), "*.doc,*.docx,*.odt")
    check("a name is read", parsed[1].name, "Documents")
    check("a table without a name has an empty one", parsed[0].name, "")
    check("a [[filter]] without globs is skipped", parsed[2].globs.join(","), "*.pdf")
    check("a trailing comment does not reach the array", parsed[2].globs.length, 1)
    check("a key under another table is not a filter", parsed.some(function (f) { return f.name === "" && f.globs[0] === "*.pdf" && f.mimes.length > 0 }), false)

    check("an empty body yields nothing", Filters.parse("").length, 0)
    check("an absent file yields nothing", Filters.parse(undefined).length, 0)
    check("comments alone yield nothing", Filters.parse("# only\n\n").length, 0)
    check("a malformed globs line is skipped with its table", Filters.parse("[[filter]]\nglobs = \"*.png\"\n").length, 0)
    check("an empty globs array is skipped", Filters.parse("[[filter]]\nglobs = []\n").length, 0)
    check("keys before any table are ignored", Filters.parse("globs = [\"*.png\"]\n").length, 0)

    // Labels the Windows way: extension first, the whole set in brackets.
    check("two globs label by extension", Filters.labelFor(parsed[0]), ".jpg (.jpg, .jpeg)")
    check("an explicit name overrides", Filters.labelFor(parsed[1]), "Documents")
    check("three globs without a name", Filters.labelFor({ name: "", globs: ["*.doc", "*.docx", "*.odt"] }), ".doc (.doc, .docx, .odt)")
    check("one glob collapses to its extension", Filters.labelFor(parsed[2]), ".pdf")
    check("the extension is lowercased", Filters.labelFor({ globs: ["*.JPG"] }), ".jpg")
    check("duplicates by case are one extension", Filters.labelFor({ globs: ["*.jpg", "*.JPG"] }), ".jpg")
    check("a two part suffix is one extension", Filters.labelFor({ globs: ["*.tar.gz"] }), ".tar.gz")
    check("a stem before the star does not count", Filters.labelFor({ globs: ["IMG_*.png"] }), ".png")
    check("a directory in the glob does not count", Filters.labelFor({ globs: ["v1.0/*.md"] }), ".md")
    check("a glob with no extension falls back to its text", Filters.labelFor({ globs: ["Makefile"] }), "Makefile")
    check("a star after the dot is no extension", Filters.labelFor({ globs: ["photo.*", "*.png"] }), "photo.* (photo.*, .png)")

    // The glob leg is the one ui/js/Picker.js matches app filters by.
    check("a glob matches", Filters.matchesGlobs("shot.png", ["*.png"]), true)
    check("a glob matches whatever the case is", Filters.matchesGlobs("SHOT.PNG", ["*.png"]), true)
    check("a name outside the globs does not match", Filters.matchesGlobs("notes.md", ["*.png"]), false)
    check("no globs is no glob leg", Filters.matchesGlobs("notes.md", []), true)
    check("a bracket in a glob is literal", Filters.matchesGlobs("a[b].png", ["a[b].png"]), true)
    check("a bracket glob does not become a character class", Filters.matchesGlobs("ab.png", ["a[b].png"]), false)
    check("a question mark is one character", Filters.matchesGlobs("ab.png", ["a?.png"]), true)
    check("a two part suffix matches", Filters.matchesGlobs("a.tar.gz", ["*.tar.gz"]), true)
    check("and nothing past it", Filters.matchesGlobs("a.tar.gzx", ["*.tar.gz"]), false)
    check("a star-heavy glob misses a long name without blowing up",
          Filters.matchesGlobs(new Array(61).join("a"), ["*a*a*a*a*a*a*a*a*b"]), false)

    // The mime leg: a row knows an icon name, so only the class of a rule can be confirmed.
    var image = { n: "shot.png", d: false, i: "image-x-generic" }
    var text = { n: "notes.md", d: false, i: "text-x-generic" }
    var pdf = { n: "paper.pdf", d: false, i: "application-x-generic" }
    var office = { n: "deck.odp", d: false, i: "x-office-presentation" }
    var dir = { n: "sub", d: true, i: "folder" }
    check("image/* matches an image row", Filters.matchesRow(image, { globs: [], mimes: ["image/*"] }), true)
    check("image/* does not match a text row", Filters.matchesRow(text, { globs: [], mimes: ["image/*"] }), false)
    check("application/* matches the generic application icon", Filters.matchesRow(pdf, { globs: [], mimes: ["application/*"] }), true)
    check("an office icon confirms no class", Filters.matchesRow(office, { globs: [], mimes: ["application/*"] }), false)
    check("image/* matches a named image icon", Filters.matchesMimes("image-jpeg", ["image/*"]), true)
    check("an exact subtype no row can confirm does not narrow", Filters.matchesRow(image, { globs: [], mimes: ["image/jpeg"] }), true)
    check("exact subtypes alone hide nothing", Filters.matchesRow(text, { globs: [], mimes: ["text/plain"] }), true)
    check("a class rule beside an exact subtype still narrows", Filters.matchesRow(text, { globs: [], mimes: ["text/plain", "image/*"] }), false)
    check("*/* matches every file", Filters.matchesRow(office, { globs: [], mimes: ["*/*"] }), true)
    check("any one rule of several is enough", Filters.matchesRow(text, { globs: [], mimes: ["image/*", "text/*"] }), true)

    // Both legs AND: the filter narrows to the intersection.
    var both = { globs: ["*.png", "*.md"], mimes: ["image/*"] }
    check("globs alone are today's behaviour", Filters.matchesRow(text, { globs: ["*.md"], mimes: [] }), true)
    check("a glob hit with a mime miss is out", Filters.matchesRow(text, both), false)
    check("a glob hit with a mime hit stands", Filters.matchesRow(image, both), true)
    check("a mime hit with a glob miss is out", Filters.matchesRow({ n: "shot.jpg", d: false, i: "image-x-generic" }, both), false)
    check("a directory stands under every filter", Filters.matchesRow(dir, both), true)
    check("a directory stands under an exact subtype rule", Filters.matchesRow(dir, { globs: [], mimes: ["image/jpeg"] }), true)
    check("no filter hides nothing", Filters.matchesRow(office, null), true)
    check("a filter with neither leg hides nothing", Filters.matchesRow(office, { globs: [], mimes: [] }), true)
}
