// Every diagram type the bundle draws, one at a time through the real figure path, so a dropped or broken type fails here by name.
import { FACES, decode, load } from "./mermaid-corpus.mjs";
import { absolute, argv, finish, readDir, readText, repoRoot, resolve } from "./js-runtime.mjs";

const render = await load(argv[0]);
let checks = 0;
let failures = 0;
function check(ok, why) {
    checks++;
    if (!ok) {
        failures++;
        console.log("FAIL " + why);
    }
}

// [type, the class only that type draws, a label the drawing keeps]; each source is the fence in tests/fixtures/markdown-mermaid/<type>.md.
const types = [
    ["flowchart", "pele-shape-diam", "Is it working?"],
    ["sequence", "pele-lifeline", "Hello John, how are you?"],
    ["class", "pele-member", "+swim()"],
    ["state", "pele-state-start", "Moving"],
    ["er", "pele-er-identifying", "LINE-ITEM"],
    ["pie", "pele-slice", "Dogs"],
    ["gantt", "pele-task", "Task in Another"],
    ["git", "pele-commit", "develop"],
    ["mindmap", "pele-shape-branch", "Long history"],
    ["timeline", "pele-event", "LinkedIn"],
    ["journey", "pele-face", "Make tea"],
    ["xychart", "pele-bars", "Sales Revenue"],
    ["quadrant", "pele-point", "Campaign A"],
    ["requirement", "pele-requirement", "test_entity"],
    ["c4", "pele-c4-boundary", "Internet Banking System"],
    ["sankey", "pele-sankey-link", "Bio-conversion"],
    ["block", "pele-shape-block-arrow", "A wide one in the middle"],
    ["packet", "pele-packet-bits", "Source Port"],
    ["kanban", "pele-card", "Task Description"],
    ["architecture", "pele-service", "Database"],
    ["radar", "pele-curve", "Geography"],
    ["treemap", "pele-leaf", "Item A1"],
    ["info", "pele-version", "Pele"],
    ["agentflow", "pele-flow", "run_linter"],
    ["usecase", "pele-association", "Place order"],
    ["railroad", "pele-track", "digit"],
    ["treeview", "pele-tree-file", "index.ts"],
    ["venn", "pele-venn-set", "Feasible"],
    ["cynefin", "pele-cynefin-domain", "Investigate root cause"],
    ["wardley", "pele-wardley-axis", "Cup of Tea"],
    ["swimlane", "pele-lane", "Pick items"],
    ["ishikawa", "pele-ishikawa-head", "Shaky hands"],
    ["eventmodeling", "pele-em-event", "ItemAdded"]
];
const fixtures = resolve(argv[0] ? absolute(argv[0]) : repoRoot(import.meta.url), "tests/fixtures/markdown-mermaid");

for (const [type, mark, label] of types) {
    // A missing fence file reads as no fence, so it fails here by name instead of ending the run.
    let fence = null;
    try {
        fence = readText(resolve(fixtures, type + ".md")).match(/```mermaid\n([\s\S]*?)\n```/);
    } catch (error) {
        fence = null;
    }
    check(fence !== null, `${type}: its fixture holds one mermaid fence`);
    const source = fence ? fence[1] : "";
    let svg = "";
    try {
        svg = render(source, FACES[0]);
    } catch (error) {
        check(false, `${type}: renders, got ${error.message}`);
        continue;
    }
    check(svg.startsWith("<svg"), `${type}: renders an svg`);
    check(new RegExp(`class="(?:[^"]* )?${mark}(?: [^"]*)?"`).test(svg), `${type}: draws ${mark}`);
    // The words every text draws, read as markup and not placed, since a chart turns some of its labels.
    const words = (svg.match(/<text\b[^>]*>[\s\S]*?<\/text>/g) || []).map((t) => decode(t.replace(/<[^>]*>/g, ""))).join(" ");
    check(words.includes(label), `${type}: keeps the label "${label}", drew "${words.slice(0, 120)}"`);
    const box = (svg.match(/^<svg\b[^>]*\sviewBox="([^"]*)"/) || [0, ""])[1].split(" ").map(Number);
    check(box.length === 4 && box.every(Number.isFinite) && box[2] > 0 && box[3] > 0, `${type}: has a canvas, viewBox ${box.join(" ")}`);
}
// The list and the fixtures name each other: a missing or extra fence goes red here, not as a short count.
const fenced = new Set(readDir(fixtures).filter((name) => name.endsWith(".md")).map((name) => name.slice(0, -".md".length)));
check(types.length === fenced.size && types.every(([type]) => fenced.has(type)), "every fixture is listed once: "
    + types.map(([type]) => type).filter((type) => !fenced.has(type)).join(",") + " without a fence; "
    + [...fenced].filter((type) => !types.some(([listed]) => listed === type)).join(",") + " fenced but unlisted");
console.log(`mermaid-types: ${checks} check(s), ${failures} failed`);
finish(failures);
