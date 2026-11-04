// The bytecode path answers the source path's bytes for the whole figure corpus, and refuses what is not bytecode; quickjs-ng only, node has no bjson.
import { finish, importFile, readText, repoRoot, resolve } from "./js-runtime.mjs";
import { FACES, layered, notes, others, sequences } from "./mermaid-corpus.mjs";

const root = repoRoot(import.meta.url);
const worker = await importFile(resolve(root, "ui/js/FigureWorker.mjs"));
const bytecode = await importFile(resolve(root, "ui/vendor/figure-bytecode.mjs"));
const sourceMath = await importFile(resolve(root, "ui/vendor/math.mjs"));
const sourceMermaid = await importFile(resolve(root, "ui/vendor/mermaid.mjs"));

let checks = 0;
let failures = 0;
function check(passed, label) {
    checks++;
    failures += passed ? 0 : 1;
    console.log((passed ? "PASS " : "FAIL ") + label);
}
async function fromBytecode(kind) {
    const spec = bytecode.BUNDLES[kind];
    const blob = bytecode.compileBundle(readText(resolve(root, "ui/vendor/" + spec.file)), spec.global);
    return await bytecode.loadBundle(new Uint8Array(blob), spec.global);
}
const compiledMath = await fromBytecode("math");
const compiledMermaid = await fromBytecode("mermaid");

// What a request answers: the svg, or the error text the helper would send.
function answer(kind, source, display, theme, api) {
    try {
        return "svg " + worker.renderFigure(kind, source, display, theme, api);
    } catch (e) {
        return "error " + String(e.message).split("\n")[0];
    }
}
const theme = { bg: "#101315", fg: "#c0caf5", accent: "#7aa2f7", font: "monospace", bodyPx: 14, exPx: 7 };

// The bundle's export clause becomes a global assignment, and anything else is refused.
// Sample input: "var a=1;export{wgt as mermaidToSvg};" and "export default 1;".
check(bytecode.exposeExports("var a=1;export{wgt as mermaidToSvg};", "g") === "var a=1;globalThis.g={mermaidToSvg:wgt};", "an aliased export becomes a member");
check(bytecode.exposeExports("x;export { a , b as c } ;\n", "g") === "x;globalThis.g={a:a,c:b};", "several members and loose spacing are read");
for (const bad of ["var a=1;", "export default 1;", "export{a};var b=2;", "export{a as b as c};", "export{a-b};"]) {
    let refused = false;
    try {
        bytecode.exposeExports(bad, "g");
    } catch (e) {
        refused = true;
    }
    check(refused, "refuses " + JSON.stringify(bad));
}

// The maths cases of the helper harness, display and inline, and a malformed one that must fail the same way.
const maths = ["\\frac{a}{b}", "\\int_0^1 x^2\\,dx", "\\sum_{n=1}^{\\infty}\\frac{1}{n^2}", "\\begin{matrix}a&b\\\\c&d\\end{matrix}",
    "\\begin{aligned}x&=1\\\\y&=2\\end{aligned}", "x^2", "\\sqrt{2}", "\\alpha+\\beta=\\gamma", "\\lim_{x\\to0}\\frac{\\sin x}{x}",
    "\\binom{n}{k}=\\frac{n!}{k!(n-k)!}", "\\frac{unclosed", "\\href{http://127.0.0.1:18037/x}{click}"];
let identicalMath = 0;
let drawnMath = 0;
for (const source of maths) {
    for (const display of [true, false]) {
        const want = answer("math", source, display, theme, sourceMath);
        drawnMath += want.startsWith("svg <svg") ? 1 : 0;
        identicalMath += want === answer("math", source, display, theme, compiledMath) ? 1 : 0;
    }
}
// Two of the twelve are refused on purpose, so a pass that compared two failures cannot hide behind equal errors.
check(drawnMath === (maths.length - 2) * 2, `the source path draws ${drawnMath} formulas and refuses the two bad ones`);
check(identicalMath === maths.length * 2, `every formula answers the same bytes from bytecode (${identicalMath} of ${maths.length * 2})`);

// The Mermaid corpus on two faces, so the table-driven label fit runs through both paths.
const diagrams = [...layered, ...sequences, ...others, notes];
const faces = [FACES[0], FACES[FACES.length - 1]];
let identicalMermaid = 0;
let drawnMermaid = 0;
let total = 0;
for (const [, source] of diagrams) {
    for (const face of faces) {
        const mine = { ...theme, advances: face.regular, boldAdvances: face.bold };
        const want = answer("mermaid", source, false, mine, sourceMermaid);
        total++;
        drawnMermaid += want.startsWith("svg <svg") ? 1 : 0;
        identicalMermaid += want === answer("mermaid", source, false, mine, compiledMermaid) ? 1 : 0;
    }
}
check(drawnMermaid === total, `the source path draws every corpus diagram (${drawnMermaid} of ${total})`);
check(identicalMermaid === total, `every diagram answers the same bytes from bytecode (${identicalMermaid} of ${total})`);
// Whatever the source path answers, bytecode answers the same bytes: a refusal for the bad diagram, and a figure with the link unwrapped for the hostile one.
const refused = [answer("mermaid", "not a diagram {{{", true, theme, sourceMermaid), answer("mermaid", "not a diagram {{{", true, theme, compiledMermaid)];
check(refused[0].startsWith("error") && refused[1].startsWith("error"), "both paths refuse a diagram that is not one");
check(refused[0] === refused[1], "and refuse it the same way");
const click = "flowchart TD\n    click A href \"http://127.0.0.1:18037/evil\"\n    A --> B";
const unwrapped = [answer("mermaid", click, true, theme, sourceMermaid), answer("mermaid", click, true, theme, compiledMermaid)];
check(unwrapped.every((got) => got.startsWith("svg <svg") && !got.includes("127.0.0.1")), "both paths draw the hostile click with no link");
check(unwrapped[0] === unwrapped[1], "and draw the same bytes");

// Anything that is not whole bytecode is refused before it runs.
const blob = new Uint8Array(bytecode.compileBundle(readText(resolve(root, "ui/vendor/math.mjs")), bytecode.BUNDLES.math.global));
for (const [label, bytes] of [["truncated", blob.slice(0, blob.length >> 1)], ["empty", new Uint8Array(0)], ["text", Uint8Array.from("var a = 1;", (c) => c.charCodeAt(0))]]) {
    let refused = false;
    try {
        await bytecode.loadBundle(bytes, bytecode.BUNDLES.math.global);
    } catch (e) {
        refused = true;
    }
    check(refused, label + " bytecode is refused");
}
console.log(`figure-bytecode: ${checks} check(s), ${failures} failed`);
finish(failures);
