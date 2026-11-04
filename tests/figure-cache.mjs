// Pin the QML cache-key functions to the reference the worker used to carry, including both layouts of one source.
import { argv, finish, readText, resolve, absolute } from "./js-runtime.mjs";

const tree = absolute(argv[0] || ".");
// The reference cache-key recipe the worker carried until 0.3.9 made it test-only: FNV-1a over the table text, then the theme key, then the kind key.
const HEX_RADIX = 16;
const FNV_OFFSET = 0x811c9dc5;
const FNV_PRIME = 0x01000193;
function tableDigest(table) {
    if (!table || table.length === 0)
        return "0";
    var text = table.join(",");
    var hash = FNV_OFFSET;
    for (var i = 0; i < text.length; i++)
        hash = Math.imul(hash ^ text.charCodeAt(i), FNV_PRIME);
    return (hash >>> 0).toString(HEX_RADIX);
}
function themeKey(t) {
    return [t.bg, t.fg, t.accent || "", t.muted || "", t.line || "", t.surface || "",
        t.border || "", t.font || "", t.bodyPx || 0, t.exPx || 0, tableDigest(t.advances), tableDigest(t.boldAdvances)].join("|")
        + (t.today ? "|" + t.today : "");
}
function cacheKey(kind, source, t, display) {
    return kind + "\n" + themeKey(t) + "\n" + !!display + "\n" + source;
}
const service = readText(resolve(tree, "ui/FigureService.qml"));
// Sample input: function name(a, b) { body with { nested } braces } answers the text between the outer braces.
function functionBody(source, name) {
    const at = source.indexOf("function " + name + "(");
    if (at < 0)
        throw new Error("FigureService function " + name + " is missing");
    const open = source.indexOf("{", at);
    let depth = 0;
    for (let i = open; i < source.length; i++) {
        depth += source[i] === "{" ? 1 : source[i] === "}" ? -1 : 0;
        if (depth === 0)
            return source.slice(open + 1, i);
    }
    throw new Error("FigureService function " + name + " does not close");
}
const digestOf = new Function("table", functionBody(service, "tableDigestOf"));
const qmlKeyOf = new Function("root", "kind", "source", "t", "display", functionBody(service, "cacheKeyOf"));
const qmlKey = (kind, source, t, display) => qmlKeyOf({ tableDigestOf: digestOf }, kind, source, t, display);
// Sample input: var MIDDAY_HOUR = 12 in the lazy dating library the figures date through.
const dateLib = readText(resolve(tree, "ui/js/FigureDate.js"));
const MIDDAY_HOUR = Number((dateLib.match(/var MIDDAY_HOUR = (\d+);/) || [0, NaN])[1]);
const dateOpen = dateLib.indexOf("{", dateLib.indexOf("function datedTheme(kind, source, theme)"));
let dateDepth = 0;
let dateClose = -1;
for (let i = dateOpen; i < dateLib.length; i++) {
    dateDepth += dateLib[i] === "{" ? 1 : dateLib[i] === "}" ? -1 : 0;
    if (dateDepth === 0) {
        dateClose = i;
        break;
    }
}
const qmlDated = new Function("MIDDAY_HOUR", "return function datedTheme(kind, source, theme) {" + dateLib.slice(dateOpen + 1, dateClose) + "}")(MIDDAY_HOUR);
const dated = (kind, source, theme) => qmlDated(kind, source, theme);
// A printable ASCII advance table in thousandths of an em, 95 entries, and the same table with one entry changed.
const TABLE_LENGTH = 95;
const table = Array.from({ length: TABLE_LENGTH }, (unused, i) => 500 + (i % 7) * 20);
const nudged = table.map((v, i) => (i === TABLE_LENGTH - 1 ? v + 1 : v));
const themes = [{ bg: "#101315", fg: "#c0caf5", accent: "#7aa2f7", font: "monospace", bodyPx: 14, exPx: 7.7, advances: table, boldAdvances: table },
    { bg: "#fff", fg: "#000" }];
// The key stands a table in by a short digest, so a request's key stays far shorter than the table's text.
const DIGEST_LENGTH_MAX = 8;
let checks = 0;
let failures = 0;
function check(ok, why) {
    checks++;
    if (!ok) {
        failures++;
        console.log("FAIL " + why);
    }
}
for (const theme of themes) {
    for (const display of [false, true])
        check(qmlKey("math", "x^2", theme, display) === cacheKey("math", "x^2", theme, display), "service and reference agree on a formula key");
    check(qmlKey("math", "x^2", theme, false) !== qmlKey("math", "x^2", theme, true), "one source has separate inline and display cache keys");
    for (const role of ["bg", "fg", "accent", "muted", "line", "surface", "border", "font", "bodyPx", "exPx", "advances", "boldAdvances"]) {
        const value = role === "advances" || role === "boldAdvances" ? nudged : ["bodyPx", "exPx"].includes(role) ? 20 : "changed " + role;
        const changed = { ...theme, [role]: value };
        check(qmlKey("mermaid", "A --> B", theme, true) !== qmlKey("mermaid", "A --> B", changed, true), "service cache key includes theme role " + role);
        check(qmlKey("mermaid", "A --> B", changed, true) === cacheKey("mermaid", "A --> B", changed, true), "service and reference agree on theme role " + role);
    }
}
// A Gantt chart draws today's date, so it is keyed on the day it is asked for; every other figure keeps the key it had.
const gantt = "gantt\n    dateFormat YYYY-MM-DD\n    section Work\n    Build :a1, 2026-10-01, 30d";
const now = new Date();
const noon = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate(), MIDDAY_HOUR);
const DAY_MS = 86400000;
for (const source of [gantt, "---\ntitle: Plan\n---\ngantt\n    Build :a1, 2026-10-01, 30d", "%%{init: {}}%%\n  gantt\n    Build :a1, 2026-10-01, 30d"])
    check(dated("mermaid", source, themes[0]).today === noon && themes[0].today === undefined, "a Gantt source is asked for today's noon UTC on a copy of its theme: " + JSON.stringify(source.slice(0, 20)));
// The dated value names the asked day in UTC, so the helper draws it in every GUI zone.
const named = new Date(dated("mermaid", gantt, themes[0]).today);
check(named.getUTCFullYear() === now.getFullYear() && named.getUTCMonth() === now.getMonth() && named.getUTCDate() === now.getDate(),
    "the dated value's UTC day is the asked local day: " + named.toISOString());
check(dated("mermaid", "flowchart TD\n    A --> B", themes[0]) === themes[0] && dated("math", "gantt", themes[0]) === themes[0],
    "a figure that is not a Gantt chart keeps its theme, and so its key");
const today = dated("mermaid", gantt, themes[0]);
const tomorrow = { ...today, today: today.today + DAY_MS };
check(qmlKey("mermaid", gantt, today, true) !== qmlKey("mermaid", gantt, tomorrow, true) && qmlKey("mermaid", gantt, today, true) !== qmlKey("mermaid", gantt, themes[0], true),
    "a Gantt chart has one key per day");
check(qmlKey("mermaid", gantt, today, true) === cacheKey("mermaid", gantt, today, true), "service and reference agree on a dated key");
// MarkdownFigure dates before ask and before its cached lookup, so each side here dates its own theme.
// Sample input: function ask() dates, then function takeRemembered() dates, in MarkdownFigure.
const figureSource = readText(resolve(tree, "ui/MarkdownFigure.qml"));
const askAt = figureSource.indexOf("function ask()");
const rememberedAt = figureSource.indexOf("function takeRemembered()");
const rememberedEnd = figureSource.indexOf("\n    }\n", rememberedAt);
check(askAt >= 0 && rememberedAt > askAt && figureSource.slice(askAt, rememberedAt).includes("FigureDate.datedTheme")
    && rememberedEnd > rememberedAt && figureSource.slice(rememberedAt, rememberedEnd).includes("FigureDate.datedTheme"), "MarkdownFigure dates before ask and before its cached lookup");
const held = { answerCache: {}, answerOrder: [], cacheMax: themes.length, tableDigestOf: digestOf };
held.cacheKeyOf = (kind, source, t, display) => qmlKeyOf(held, kind, source, t, display);
const qmlStore = new Function("root", "kind", "source", "theme", "display", "svg", functionBody(service, "store"));
const qmlCached = new Function("root", "kind", "source", "theme", "display", functionBody(service, "cached"));
for (const source of [gantt, "flowchart TD\n    A --> B"]) {
    const svg = "<svg>" + source.length + "</svg>";
    const stored = dated("mermaid", source, themes[0]);
    qmlStore(held, "mermaid", source, stored, true, svg);
    const lookedUp = dated("mermaid", source, themes[0]);
    check(qmlCached(held, "mermaid", source, lookedUp, true) === svg, "cached finds the stored answer from the dated theme: " + JSON.stringify(source.slice(0, 20)));
}
check(tableDigest(table).length <= DIGEST_LENGTH_MAX && digestOf(table) === tableDigest(table), "the service digest is the reference, " + tableDigest(table));
check(digestOf(table) !== digestOf(nudged) && digestOf([]) === digestOf(undefined), "one changed entry changes the service digest, and no table digests as none");
console.log(`figure-cache: ${checks} check(s), ${failures} failed`);
finish(failures);
