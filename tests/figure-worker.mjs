// Exercise SVG post-processing directly, with the cyclic fixture confined to a child process.
import { argv, finish, forceUtcZone, readText, repoRoot, resolve, runSelf } from "./js-runtime.mjs";
import { checkSafe, postMermaid, postMath, renderFigure } from "../ui/js/FigureWorker.mjs";
import { texToSvg } from "../ui/vendor/math.mjs";
import { mermaidToSvg } from "../ui/vendor/mermaid.mjs";

// A hung resolver must fail this test without holding the suite open.
const CYCLE_BOUND_SECONDS = 5;
const theme = { bg: "#000000", fg: "#ffffff", font: "monospace", bodyPx: 14 };
// Sample input: two custom properties refer back to each other inside one style block, which the post-pass never reads as a table and the vocabulary never holds.
const cyclic = '<svg><style>svg { --a: var(--b); --b: var(--a); }</style><rect fill="var(--a)"/></svg>';
const roles = { bg: "#000000", fg: "#ffffff", accent: "#ff0000", muted: "#808080", surface: "#202020", font: "monospace", bodyPx: 14 };
// A Gantt chart draws the day its request names, whatever the clock says, so a figure cached under one day's key is that day's drawing.
const gantt = "gantt\n    dateFormat YYYY-MM-DD\n    section Work\n    Build :a1, 2001-10-01, 30d\n    Ship  :after a1, 10d";
const DAY_MS = 86400000;
const NOON_19 = Date.UTC(2001, 9, 19, 12);
const realDate = Date;
function GuardedDate(...args) {
    if (args.length === 0)
        throw new Error("the bundle read the clock through a bare new Date()");
    return new realDate(...args);
}
GuardedDate.prototype = realDate.prototype;
GuardedDate.UTC = realDate.UTC;
GuardedDate.parse = realDate.parse;
// A dated render takes the day from the request alone: a bare new Date() on this path throws, so growing one goes red here.
function guardedRender(today, clock) {
    GuardedDate.now = () => clock;
    globalThis.Date = GuardedDate;
    try {
        return renderFigure("mermaid", gantt, true, { ...roles, today }, { mermaidToSvg });
    } finally {
        globalThis.Date = realDate;
    }
}
// Sample input: Oct 2001 ticks a week apart with the marker on the 19th read "2001-10-19".
// The marker's calendar day reads off the figure's own weekly grid, so it holds in any render zone.
function markerDay(svg) {
    const grid = (svg.match(/<g class="pele-grid"[^>]*>([\s\S]*?)<\/g>/) || [null, ""])[1];
    const xs = [...grid.matchAll(/M([\d.]+),/g)].map((m) => Number(m[1]));
    const days = [...svg.matchAll(/<text[^>]*>(2001-[\d-]+)<\/text>/g)].map((m) => m[1].split("-").map(Number));
    const marker = Number((svg.match(/<g class="pele-today">[\s\S]*?M([\d.]+),/) || [null, NaN])[1]);
    const first = Date.UTC(days[0][0], days[0][1] - 1, days[0][2]);
    const scale = (xs[1] - xs[0]) / ((Date.UTC(days[1][0], days[1][1] - 1, days[1][2]) - first) / DAY_MS);
    return new Date(first + (marker - xs[0]) / scale * DAY_MS).toISOString().slice(0, 10);
}
// Sample input: var MIDDAY_HOUR = 12 in the lazy dating library the figures date through.
// The zone checks run the library's own dating, so a GUI-side regression goes red here, not in date arithmetic.
const dateLib = readText(resolve(repoRoot(import.meta.url), "ui/js/FigureDate.js"));
const dateMidday = Number((dateLib.match(/var MIDDAY_HOUR = (\d+);/) || [0, NaN])[1]);
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
const datedTheme = new Function("MIDDAY_HOUR", "return function datedTheme(kind, source, theme) {"
    + dateLib.slice(dateOpen + 1, dateClose) + "}")(dateMidday);
// One fixed instant in two GUI zones: 2001-10-20 00:30 at +13 and 2001-10-19 00:30 at -11, both 11:30 UTC.
const FIXED_NOW = Date.UTC(2001, 9, 19, 11, 30);
function guiDate(fixed) {
    function F(...args) {
        return args.length === 0 ? new realDate(fixed) : new realDate(...args);
    }
    F.UTC = realDate.UTC;
    F.parse = realDate.parse;
    F.now = () => fixed;
    F.prototype = realDate.prototype;
    return F;
}
// The child dates in its own zone and the UTC parent renders, the split the helper's jail makes.
if (argv[0] === "zone") {
    const zone = argv[1];
    const want = zone === "Pacific/Tongatapu" ? [20, 0] : [19, 0];
    const probe = new Date(FIXED_NOW);
    const zoneOk = probe.getDate() === want[0] && probe.getHours() === want[1];
    globalThis.Date = guiDate(FIXED_NOW);
    let dated;
    try {
        dated = datedTheme("mermaid", gantt, { ...roles });
    } finally {
        globalThis.Date = realDate;
    }
    const oldNoon = new Date(2001, 9, 20, 12).getTime();
    console.log(JSON.stringify({ zoneOk, today: dated.today, oldNoon }));
    finish(zoneOk ? 0 : 1);
} else if (argv[0] === "late") {
    const svg = guardedRender(Date.UTC(2001, 9, 20, 12), FIXED_NOW);
    console.log(svg.startsWith("<svg") ? markerDay(svg) : "no figure");
    finish(0);
} else if (argv[0] === "cycle") {
    try {
        postMermaid(cyclic, theme);
        console.log("FAIL two-variable cycle did not refuse the SVG");
        finish(1);
    } catch (error) {
        const passed = /unlisted element style/i.test(error.message);
        console.log((passed ? "PASS " : "FAIL ") + "two-variable cycle refuses the SVG at its style element: " + error.message);
        finish(passed ? 0 : 1);
    }
} else {
    let checks = 0;
    let failures = 0;
    function check(passed, label) {
        checks++;
        failures += passed ? 0 : 1;
        console.log((passed ? "PASS " : "FAIL ") + label);
    }
    // The helper formats in UTC, so this run renders its Gantt days in a UTC zone; without a zone seam the relative checks below still hold.
    const utcRender = forceUtcZone() && new Date(Date.UTC(2001, 9, 20, 12)).getHours() === 12;
    console.log("figure-worker: the render zone is " + (utcRender ? "UTC" : "the suite lane's own"));
    const child = runSelf(import.meta.url, ["cycle"], CYCLE_BOUND_SECONDS);
    check(child.status === 0, "two-variable cycle refuses the SVG: " + child.output);
    // The bundle resolves every theme role it draws to the request's own colours, so no var() reaches the post-pass, in a fill or a stroke.
    const flow = renderFigure("mermaid", "flowchart TD\nA --> B", false, roles, { mermaidToSvg });
    check(!flow.includes("var(") && /<svg\b[^>]*\sfill="#ffffff"/.test(flow) && flow.includes('<rect x="-30" y="-18.45" width="60" height="36.9" fill="#202020" stroke="#808080"'),
        "theme roles resolve to the request's colours: text in the foreground, a node on the surface, its border in the muted role");
    // A link crossing a nested subgraph's title gets a halo that mixes two roles, which the bundle hands over as one concrete colour.
    const nested = mermaidToSvg("flowchart TD\nsubgraph outer [Outer]\nsubgraph inner [Inner]\nA --> B\nend\nC --> A\nend", roles);
    check(!nested.includes("color-mix(") && /<g class="pele-cluster-title"[^<>]*><text fill="none" stroke="#[0-9a-f]{6}" stroke-width="5"/.test(nested),
        "a title halo's colour mix reaches the post-pass as one concrete colour");
    // The entry names its var() nesting cap after the old worker's, and a 12-deep alias chain resolves while a 20-deep one refuses.
    const entrySource = readText(resolve(repoRoot(import.meta.url), "tools/vendor-js/mermaid-entry.mjs"));
    const cap = Number((entrySource.match(/const RESOLVER_DEPTH_MAX = (\d+);/) || [null, NaN])[1]);
    check(cap === 12, "the entry names its nesting cap RESOLVER_DEPTH_MAX at 12, got " + cap);
    check(/if \(depth > RESOLVER_DEPTH_MAX\)/.test(entrySource), "resolve bounds its nesting by the named cap");
    const open = entrySource.indexOf("{", entrySource.indexOf("function resolve(text, table, depth)"));
    let braceDepth = 0;
    let close = -1;
    for (let i = open; i < entrySource.length; i++) {
        braceDepth += entrySource[i] === "{" ? 1 : entrySource[i] === "}" ? -1 : 0;
        if (braceDepth === 0) {
            close = i;
            break;
        }
    }
    const resolveVar = new Function("const RESOLVER_DEPTH_MAX = " + cap + "; return function resolve(text, table, depth) {"
        + entrySource.slice(open + 1, close) + "}")();
    const chain = (n) => {
        const table = {};
        for (let i = 1; i <= n; i++)
            table["a" + i] = i < n ? "var(--a" + (i + 1) + ")" : "#123456";
        return table;
    };
    check(resolveVar("var(--a1)", chain(12), 0) === "#123456", "a 12-deep alias chain resolves");
    let deep = "";
    try {
        resolveVar("var(--a1)", chain(20), 0);
    } catch (error) {
        deep = error.message;
    }
    check(/too deep/.test(deep), "a 20-deep alias chain refuses: " + deep);
    const mixed = (() => { try { postMermaid('<svg><rect fill="color-mix(in srgb, #ffffff 25%, #000000)"/></svg>', theme); return ""; } catch (error) { return error.message; } })();
    check(/unresolved style/.test(mixed), "a colour mix the bundle left is refused: " + mixed);
    const unresolved = (() => { try { postMermaid('<svg><rect fill="var(--fg)"/></svg>', theme); return ""; } catch (error) { return error.message; } })();
    check(/unresolved style/.test(unresolved), "a variable the bundle left is refused: " + unresolved);
    // Each names a file a renderer could load, in a form an element-name, double-quote, lower-case or literal-text check misses.
    const hostile = [
        ["a prefixed image", "<svg><s:image xmlns:s=\"http://www.w3.org/2000/svg\" width=\"9\" height=\"9\" href='#a'/></svg>"],
        ["a prefixed script", '<svg><s:script xmlns:s="http://www.w3.org/2000/svg">1</s:script></svg>'],
        ["a prefixed foreign object", '<svg><s:foreignObject xmlns:s="http://www.w3.org/2000/svg" width="9" height="9">x</s:foreignObject></svg>'],
        ["a single-quoted use link", "<svg><use href='file:///tmp/x.svg#a'/></svg>"],
        ["a single-quoted filter image link", "<svg><filter id=\"f\"><feImage href='file:///tmp/x.png'/></filter></svg>"],
        ["an upper-case URL()", '<svg><rect fill="URL(file:///tmp/x.svg#p)"/></svg>'],
        ["a decimal-encoded scheme", '<svg><use href="&#102;ile:x.svg#a"/></svg>'],
        ["a hex-encoded scheme", '<svg><use xlink:href="&#x66;&#x69;le:x.svg#a"/></svg>'],
        ["a tab inside a scheme", '<svg><use href="fi&#9;le:x.svg#a"/></svg>'],
        ["an encoded url()", '<svg><rect fill="&#117;rl(file:///tmp/x.svg#p)"/></svg>'],
        ["an entity the document defines", '<!DOCTYPE svg [<!ENTITY e "file:x.svg#a">]><svg><use href="&e;"/></svg>'],
        ["a style element's upper-case URL()", "<svg><style>rect { fill: URL(file:///tmp/x.svg#p) }</style><rect/></svg>"],
        ["an upper-case URL() in a style's character data", "<svg><style><![CDATA[rect { fill: URL(file:///tmp/x.svg#p) }]]></style><rect/></svg>"],
        // Each passes a denylist: a relative link, markup no bundle writes, or a style the drawing never carries; the vocabulary refuses it.
        ["a relative link", '<svg><a href="x.svg#a"><rect/></a></svg>'],
        ["a style sheet instruction", '<?xml-stylesheet type="text/css" href="x.css"?><svg><rect/></svg>'],
        ["an animation that sets a link", '<svg><a href="#a"><set attributeName="href" to="x.svg#a"/></a></svg>'],
        ["an event attribute", '<svg><rect onclick="x()"/></svg>'],
        ["a style property outside the list", '<svg><rect style="fill: red"/></svg>'],
        ["a style value that is not a length", '<svg style="vertical-align: expression(x)"><rect/></svg>'],
        ["a style element", "<svg><style>rect { fill: red }</style><rect/></svg>"],
        ["a namespace other than SVG's", '<svg xmlns="http://www.w3.org/1999/xhtml"><rect/></svg>'],
        ["a local use", "<svg><use href='#a'/></svg>"],
        ["an xlink namespace and link", '<svg xmlns:xlink="http://www.w3.org/1999/xlink"><use xlink:href="#a"/></svg>'],
        ["a filter with a local image", '<svg><filter id="f"><feImage href="#a"/></filter><rect filter="url(#f)"/></svg>'],
        ["a comment", "<svg><!-- x --><rect/></svg>"],
        ["an unquoted attribute", "<svg><rect fill=red/></svg>"]
    ];
    for (const [name, svg] of hostile) {
        const reason = checkSafe(svg);
        check(reason !== null, "checkSafe refuses " + name + ": " + reason);
    }
    // What the bundles draw stays drawn: local references in either quote style, the SVG namespace, a length style, and words in a label that only read like markup.
    const local = ["<svg><a href='#a'><path d='M0 0'/></a></svg>", '<svg><rect fill="url(#p)"/></svg>',
        '<svg xmlns="http://www.w3.org/2000/svg" style="vertical-align: -2.5px; min-width: 40px;"><g data-mml-node="math"/></svg>',
        '<svg><text>&lt;s:image href=\'file:x\'&gt; URL(x) &#102;ile:x var(--a) color-mix(</text></svg>'];
    check(local.every((svg) => checkSafe(svg) === null), "checkSafe passes local references and label words: " + local.map(checkSafe).join(", "));
    // Every attribute the post-pass writes from a style is an allowed attribute, read from the worker's own lists so the check holds for any edit.
    const workerSource = readText(resolve(repoRoot(import.meta.url), "ui/js/FigureWorker.mjs"));
    const listed = (name) => [...workerSource.match(new RegExp("var " + name + " = \\[([\\s\\S]*?)\\];"))[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
    const styleNames = listed("STYLE_ATTRIBUTES");
    const allowed = listed("ATTRIBUTES");
    check(styleNames.length > 0 && styleNames.every((name) => allowed.includes(name)),
        "every style attribute is an allowed attribute, missing: " + styleNames.filter((name) => !allowed.includes(name)).join(" "));
    for (const name of styleNames) {
        const reason = checkSafe(`<svg><rect ${name}="1"/></svg>`);
        check(reason === null, `checkSafe passes the style attribute ${name}: ${reason}`);
    }
    // The four the post-pass writes that the corpus never drew keep their value checks: a non-local url(), a remote scheme and a leftover var() refuse by reason.
    const valueCases = [["non-local url", "url(file:///tmp/x.svg#p)"], ["remote reference", "https://x.org/a"], ["unresolved style", "var(--fg)"]];
    for (const name of ["stroke-opacity", "stroke-dashoffset", "letter-spacing", "word-spacing"])
        for (const [reason, value] of valueCases) {
            const got = checkSafe(`<svg><rect ${name}="${value}"/></svg>`);
            check(got !== null && got.includes(reason), `${name} refuses ${value} as ${reason}: ${got}`);
        }
    // Each of the four draws through a style, a class definition and a link style: a node's attribute, and a labelled edge's, since a text property needs the label.
    const declarations = [["stroke-opacity:0.5", 'stroke-opacity="0.5"'], ["stroke-dashoffset:3", 'stroke-dashoffset="3"'],
        ["letter-spacing:2px", 'letter-spacing="2px"'], ["word-spacing:4px", 'word-spacing="4px"']];
    const forms = [["style", (decl) => "flowchart TD\n    A --> B\n    style A " + decl],
        ["classDef", (decl) => "flowchart TD\n    A --> B\n    classDef hot " + decl + "\n    class A hot"],
        ["linkStyle", (decl) => "flowchart TD\n    A -->|hi| B\n    linkStyle 0 " + decl]];
    for (const [form, make] of forms)
        for (const [decl, attribute] of declarations) {
            let drawn;
            try { drawn = renderFigure("mermaid", make(decl), false, roles, { mermaidToSvg }); } catch (error) { drawn = error.message; }
            check(drawn.includes(attribute), `${form} ${decl} draws as ${attribute}: ${drawn.slice(0, 90)}`);
        }
    // Mermaid's edge animation idiom draws its dash offset and drops the animation, which no renderer runs and the post-pass never writes.
    const animation = "flowchart LR\n    A e1@--> B\n    classDef animate stroke-dasharray: 9,5,stroke-dashoffset: 900,animation: dash 25s linear infinite;\n    class e1 animate";
    let animated;
    try { animated = renderFigure("mermaid", animation, false, roles, { mermaidToSvg }); } catch (error) { animated = error.message; }
    check(animated.includes('stroke-dashoffset="900"') && !animated.includes("animation"), "the edge animation idiom draws its dash offset: " + animated.slice(0, 90));
    // A label's own words are never a colour to resolve: a variable, a colour mix, currentColor and a theme hex reach the drawing as written.
    for (const words of ["color var(--pele-fg) here", "currentColor", "mix color-mix(in srgb,#ffffff 25%,#000000) end", "hex #808080 and #ff0000"]) {
        let drawn;
        try { drawn = renderFigure("mermaid", 'flowchart TD\n    A["' + words + '"] --> B', false, roles, { mermaidToSvg }); } catch (error) { drawn = error.message; }
        // A long label wraps at its spaces into one text per line, so the lines read back joined by one.
        const lines = (drawn.match(/<text\b[^<>]*>[^<]*<\/text>/g) || [drawn]).map((text) => text.replace(/<[^<>]*>/g, ""));
        check(lines.join(" ").includes(words), "a label reading " + words + " draws its own words: " + lines.join(" | "));
    }
    let datedSvg = "";
    try {
        datedSvg = guardedRender(NOON_19, NOON_19);
    } catch (error) {
        datedSvg = error.message;
    }
    check(datedSvg.startsWith("<svg"), "a dated Gantt render reads no bare new Date(): " + datedSvg.slice(0, 80));
    const realNow = Date.now;
    function ganttAt(today, clock) {
        Date.now = () => clock;
        try {
            return renderFigure("mermaid", gantt, true, { ...roles, today }, { mermaidToSvg });
        } finally {
            Date.now = realNow;
        }
    }
    const day19 = ganttAt(NOON_19, NOON_19);
    check(day19 === ganttAt(NOON_19, NOON_19 + 7 * DAY_MS) && day19 !== ganttAt(NOON_19 + 7 * DAY_MS, NOON_19),
        "a Gantt chart draws the day its request names and not the clock's");
    check(utcRender, "the lane renders its Gantt days in UTC");
    if (utcRender) {
        const drawnDay = datedSvg.startsWith("<svg") ? markerDay(datedSvg) : "no figure: " + datedSvg.slice(0, 60);
        check(drawnDay === "2001-10-19", "noon UTC of 2001-10-19 renders the 19th in UTC: " + drawnDay);
        let oldNoon = 0;
        for (const [zone, day] of [["Pacific/Tongatapu", "2001-10-20"], ["Pacific/Pago_Pago", "2001-10-19"]]) {
            const child = runSelf(import.meta.url, ["zone", zone], CYCLE_BOUND_SECONDS, zone);
            let info = { zoneOk: false, today: 0, oldNoon: 0 };
            try {
                info = JSON.parse(child.output);
            } catch (error) {
                info.error = error.message;
            }
            check(child.status === 0 && info.zoneOk === true, "a " + zone + " GUI process reads its own day: " + child.output);
            const svg = guardedRender(info.today, FIXED_NOW);
            const drawn = svg.startsWith("<svg") ? markerDay(svg) : "no figure: " + svg.slice(0, 60);
            check(drawn === day, "a " + zone + " GUI asking " + day + " is drawn " + day + ": " + drawn);
            if (zone === "Pacific/Tongatapu")
                oldNoon = info.oldNoon;
        }
        // Round 1 dated by local noon: at +13 that names the day before, which a UTC helper draws.
        const oldDrawn = guardedRender(oldNoon, FIXED_NOW);
        check(oldDrawn.startsWith("<svg") && markerDay(oldDrawn) === "2001-10-19",
            "local noon of the asked day draws the day before in a +13 GUI zone");
        // A helper left in the GUI zone: noon UTC rendered at +13 lands a day late, so the pin matters.
        const late = runSelf(import.meta.url, ["late", "Pacific/Tongatapu"], CYCLE_BOUND_SECONDS, "Pacific/Tongatapu");
        check(late.status === 0 && late.output === "2001-10-21",
            "noon UTC of 2001-10-20 rendered in a +13 zone draws the 21st: " + late.output);
    }
    const styled = postMermaid('<svg><rect style="fill:#ffffff;stroke-width:2;rx:4px" stroke="#000000"/><text style="font-size:12px">a</text></svg>', theme);
    check(styled.includes('fill="#ffffff"') && styled.includes('stroke-width="2"') && styled.includes('rx="4"') && styled.includes('font-size="12px"') && !styled.includes("style="),
        "style attributes become SVG presentation attributes");
    check(postMermaid('<svg><rect style="fill:#ffffff" fill="#000000"/></svg>', theme).includes('fill="#000000"') && !postMermaid('<svg><rect style="fill:#ffffff" fill="#000000"/></svg>', theme).includes('fill="#ffffff"'),
        "an attribute already set keeps its value over the style's");
    check(postMath('<svg width="1.234ex" height="-2ex" fill="currentColor"/>', theme).includes('width="7.64px" height="-12.38px"'), "ex conversion uses MathJax's 0.442 em and rounds to hundredths");
    // Sample input: MathJax's x^2 SVG, 2.025ex tall over a 894.9 unit viewBox at 442 units per ex.
    const formula = '<svg width="2.282ex" height="2.025ex" viewBox="0 -883.9 1008.6 894.9"><path fill="currentColor"/></svg>';
    const MATH_UNITS_PER_EX = 442;
    const EX_PER_EM = 0.442;
    // The exPx path is exact up to the hundredth-pixel rounding of the drawn height, so one percent leaves the 1.2 em rule (3.6% off at 12 and 14) red.
    const EX_PATH_TOLERANCE = 0.01;
    // The ex the formula takes through the helper's own entry point, read back from its height, or NaN when no pixel height was written.
    function drawnExPx(theme, display) {
        let asked = null;
        const svg = renderFigure("math", "x^2", display, theme, { texToSvg: (source, wantDisplay) => { asked = wantDisplay; return formula; } });
        check(asked === display, `a ${display ? "display" : "inline"} formula reaches MathJax as ${display ? "display" : "inline"}, asked ${asked}`);
        const match = svg.match(/height="([\d.]+)px"/);
        return match ? Number(match[1]) / (894.9 / MATH_UNITS_PER_EX) : NaN;
    }
    function exMatches(drawn, expected) {
        return Math.abs(drawn - expected) <= EX_PATH_TOLERANCE * expected;
    }
    // An inline and a display formula both take the body font's measured x-height at any body size, so each stands level with the prose.
    for (const display of [false, true]) {
        const kind = display ? "display" : "inline";
        for (const [bodyPx, xHeight] of [[12, 6.6], [14, 7.7], [14, 9]]) {
            const exPx = drawnExPx({ fg: "#ffffff", bodyPx, exPx: xHeight }, display);
            check(exMatches(exPx, xHeight), `${kind} maths ex ${exPx.toFixed(2)}px follows the body x-height ${xHeight}px at body ${bodyPx}px`);
        }
        // Without a measured x-height the em rule stays.
        const EM_BODY_PX = 14;
        const EM_RULE_EX_PX = EM_BODY_PX * EX_PER_EM;
        const fallbackEx = drawnExPx({ fg: "#ffffff", bodyPx: EM_BODY_PX }, display);
        check(exMatches(fallbackEx, EM_RULE_EX_PX), `${kind} maths ex ${fallbackEx.toFixed(2)}px falls back to the em rule ${EM_RULE_EX_PX.toFixed(2)}px without a measured x-height`);
    }
    // An inline formula draws whole through the real bundle: operators never cut the later terms, and a two-term formula draws wider than its first term alone.
    const inlineTheme = { bg: "#000000", fg: "#ffffff", font: "monospace", bodyPx: 14, exPx: 7.7, advances: [], boldAdvances: [] };
    function inlineSvg(source) {
        return renderFigure("math", source, false, inlineTheme, { texToSvg });
    }
    function inlineWidthPx(source) {
        const match = inlineSvg(source).match(/width="([\d.]+)px"/);
        return match ? Number(match[1]) : NaN;
    }
    for (const [source, marker] of [["x^2 + y^2", 'data-latex="y"'], ["a+b", 'data-latex="b"'],
        ["\\frac{a}{b} + c", 'data-latex="c"'], ["\\sqrt{x + 1}", "msqrt"], ["a_{1} + b^{2}", 'data-latex="b"']]) {
        check(inlineSvg(source).includes(marker), `inline ${source} draws its later terms`);
    }
    check(inlineWidthPx("x^2 + y^2") > inlineWidthPx("x^2"), "an inline two-term formula draws wider than its first term alone");
    console.log(`figure-worker: ${checks} check(s), ${failures} failed`);
    finish(failures);
}
