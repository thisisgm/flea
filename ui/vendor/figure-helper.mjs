// One sandboxed figure renderer: newline-delimited JSON in and out, started by flea --figure-helper under bwrap with no network or writable paths.
import * as std from "qjs:std";

// Sample input: {"id":3,"kind":"math","source":"\\frac{a}{b}","display":true,"theme":{"bg":"#101315","fg":"#c0caf5"}}.
var worker = await import("../js/FigureWorker.mjs");
var mathApi = null;
var mermaidApi = null;
// Sample argv: figure-helper.mjs --bytecode=/home/gm/.cache/flea/figures/<key> --warm=mermaid,math --report; each flag is optional.
function flagValue(name) {
    var prefix = "--" + name + "=";
    var found = scriptArgs.slice(1).find(function (arg) { return arg.startsWith(prefix); });
    return found ? found.slice(prefix.length) : "";
}
// The launcher names the verified bytecode directory; none means every bundle is parsed from source.
var bytecodeDir = flagValue("bytecode");
var bytecode = bytecodeDir ? await import("./figure-bytecode.mjs") : null;
var KINDS = ["math", "mermaid"];
// The kinds a warm start loads before the first request, so the first figure finds its bundle ready.
var warmKinds = flagValue("warm").split(",").filter(function (kind, at, all) { return KINDS.includes(kind) && all.indexOf(kind) === at; });
var ERROR_WORDS = 160;
// With --report each bundle load prints {"id":0,"bundle":"math","from":"bytecode"} before the answer that needed it, so a silent fallback to source is visible.
var reportLoads = scriptArgs.slice(1).includes("--report");

function failText(e) {
    var msg = String((e && e.message) || e).split("\n")[0].slice(0, ERROR_WORDS);
    return msg || "render failed";
}

function answer(out) {
    std.out.puts(JSON.stringify(out) + "\n");
    std.out.flush();
}

// A bundle from its bytecode, or from source when there is none or it will not load; one refusal sends every later bundle to source too.
async function bundleOf(kind) {
    var spec = bytecode ? bytecode.BUNDLES[kind] : null;
    var api = null;
    var from = "source";
    if (spec) {
        try {
            api = await bytecode.loadBundle(bytecode.readBytes(bytecodeDir + "/" + spec.blob), spec.global);
            from = "bytecode";
        } catch (e) {
            bytecode = null;
        }
    }
    if (!api)
        api = kind === "math" ? await import("./math.mjs") : await import("./mermaid.mjs");
    if (reportLoads)
        answer({ id: 0, bundle: kind, from: from });
    return api;
}

function themeOf(req) {
    return req.theme || {};
}

async function renderOne(req) {
    var kind = req.kind;
    var source = String(req.source || "");
    var theme = themeOf(req);
    if (kind === "math") {
        if (!mathApi)
            mathApi = await bundleOf("math");
        return worker.renderFigure(kind, source, !!req.display, theme, mathApi);
    }
    if (kind === "mermaid") {
        if (!mermaidApi)
            mermaidApi = await bundleOf("mermaid");
        var got = worker.renderFigure(kind, source, !!req.display, theme, mermaidApi);
        return (got && got.then) ? await got : got;
    }
    throw new Error("unknown figure kind");
}

// One throwaway render per warmed kind, so the engine's own first-use setup is paid before a figure is asked for; the answer is dropped.
var WARM_SOURCE = { math: "x", mermaid: "flowchart TD\n    A --> B" };
var WARM_THEME = { bg: "#101315", fg: "#c0caf5" };
async function warmUp() {
    for (var kind of warmKinds) {
        try {
            await renderOne({ kind: kind, source: WARM_SOURCE[kind], display: false, theme: WARM_THEME });
        } catch (e) {
            // A failed warm-up only means the first real request pays for it.
        }
    }
    answer({ id: 0, warm: warmKinds });
}
if (warmKinds.length > 0)
    await warmUp();

// A bad line answers error under its own id, or id 0 when it names none; the loop survives it and EOF ends the process.
var line;
while ((line = std.in.getline()) !== null) {
    if (line === "" || line === "\n")
        continue;
    var req = null;
    try {
        req = JSON.parse(line);
    } catch (e) {
        answer({ id: 0, error: failText(e) });
        continue;
    }
    var id = (req && typeof req.id === "number") ? req.id : 0;
    try {
        answer({ id: id, svg: await renderOne(req) });
    } catch (e) {
        answer({ id: id, error: failText(e) });
    }
}
