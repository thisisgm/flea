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
// A sequence draws v0.3.12's 80 by 40 participants 60 apart over dashed lifelines, as the HIG board does, and each key a source sets wins (SW-19).
function participants(svg) {
    return (svg.match(/<rect\b[^>]*\sstroke="(?!none")[^"]*"[^>]*>/g) || []).map((r) => ["x", "width", "height"].map((name) => Number((r.match(new RegExp(`\\s${name}="([^"]*)"`)) || [])[1])));
}
function drawsParticipants(svg, width, height, apart) {
    const boxes = participants(svg);
    return boxes.length === 4 && boxes.every(([, w, h]) => w === width && h === height) && boxes[1][0] - boxes[0][0] === apart;
}
const plain = render("sequenceDiagram\n    A->>B: hi", FACES[0]);
check(drawsParticipants(plain, 80, 40, 140), `sequence: 80 by 40 participants 140 apart, drew ${JSON.stringify(participants(plain))}`);
// The whole group tag, so a Pele that starts writing its own stroke-width or dash fails here instead of drawing a duplicate attribute.
const lifelines = (plain.match(/<g class="pele-lifelines"[^>]*>/) || [""])[0];
check(/^<g class="pele-lifelines" stroke-width="0\.75" stroke-dasharray="6 4" fill="none" stroke="#[0-9a-f]{6}">$/.test(lifelines), `sequence: dashed lifelines, drew ${lifelines}`);
const numbered = render('%%{init: {"sequence": {"showSequenceNumbers": true}}}%%\nsequenceDiagram\n    A->>B: hi', FACES[0]);
check(drawsParticipants(numbered, 80, 40, 140), `sequence: a directive setting another key keeps the defaults, drew ${JSON.stringify(participants(numbered))}`);
const wide = render("---\nconfig:\n  sequence:\n    width: 120\n---\nsequenceDiagram\n    A->>B: hi", FACES[0]);
check(drawsParticipants(wide, 120, 40, 180), `sequence: front matter's width wins and the gap stays, drew ${JSON.stringify(participants(wide))}`);
const quoted = render('---\nconfig: {"sequence": {"width": 120}}\n---\nsequenceDiagram\n    A->>B: hi', FACES[0]);
check(drawsParticipants(quoted, 120, 40, 180), `sequence: a quoted front matter width wins, drew ${JSON.stringify(participants(quoted))}`);
const elsewhere = render("---\nconfig:\n  flowchart:\n    width: 200\n---\nsequenceDiagram\n    A->>B: hi", FACES[0]);
check(drawsParticipants(elsewhere, 80, 40, 140), `sequence: a width in another section keeps the default, drew ${JSON.stringify(participants(elsewhere))}`);
const sibling = render("---\nconfig:\n  sequence:\n    showSequenceNumbers: true\n  flowchart:\n    width: 200\n---\nsequenceDiagram\n    A->>B: hi", FACES[0]);
check(drawsParticipants(sibling, 80, 40, 140), `sequence: a width in a later sibling section keeps the default, drew ${JSON.stringify(participants(sibling))}`);
const flow = render('---\nconfig: {"sequence": {"showSequenceNumbers": true}, "flowchart": {"width": 200}}\n---\nsequenceDiagram\n    A->>B: hi', FACES[0]);
check(drawsParticipants(flow, 80, 40, 140), `sequence: a width beside a flow-style sequence section keeps the default, drew ${JSON.stringify(participants(flow))}`);
const titled = render("---\ntitle: Login sequence: happy path\nconfig:\n  sequence:\n    width: 120\n---\nsequenceDiagram\n    A->>B: hi", FACES[0]);
check(drawsParticipants(titled, 120, 40, 180), `sequence: a title naming a sequence leaves the front matter's width in charge, drew ${JSON.stringify(participants(titled))}`);
const nested = render('---\nconfig: {"sequence": {"x": {"y": 1}, "width": 120}}\n---\nsequenceDiagram\n    A->>B: hi', FACES[0]);
check(drawsParticipants(nested, 120, 40, 180), `sequence: a width after a nested flow object wins, drew ${JSON.stringify(participants(nested))}`);
const spread = render('---\nconfig: {\n  "sequence": {"width": 120}\n}\n---\nsequenceDiagram\n    A->>B: hi', FACES[0]);
check(drawsParticipants(spread, 120, 40, 180), `sequence: a flow config over several lines keeps its width, drew ${JSON.stringify(participants(spread))}`);
const commented = render("---\nconfig:\n  # sequence: {actorMargin: 1}\n  sequence:\n    width: 120\n---\nsequenceDiagram\n    A->>B: hi", FACES[0]);
check(drawsParticipants(commented, 120, 40, 180), `sequence: a commented line names nothing, drew ${JSON.stringify(participants(commented))}`);
// A defaults line written above front matter Pele still reads as front matter fails detection, so the error is the drawing here.
let spaced = "";
try {
    spaced = render("--- \r\ntitle: Login\r\n---  \r\n\r\nsequenceDiagram\r\n    A->>B: hi", FACES[0]);
} catch (error) {
    spaced = error.message;
}
check(drawsParticipants(spaced, 80, 40, 140), `sequence: front matter with trailing blanks and CRLF still takes the defaults, drew ${JSON.stringify(participants(spaced))}`);
const narrow = render('%%{init: {"sequence": {"width": 70}}}%%\nsequenceDiagram\n    A->>B: hi', FACES[0]);
check(drawsParticipants(narrow, 70, 40, 130), `sequence: a directive's width wins, drew ${JSON.stringify(participants(narrow))}`);
// The list and the fixtures name each other: a missing or extra fence goes red here, not as a short count.
const fenced = new Set(readDir(fixtures).filter((name) => name.endsWith(".md")).map((name) => name.slice(0, -".md".length)));
const listed = types.map(([type]) => type);
check(new Set(listed).size === listed.length && listed.length === fenced.size && listed.every((type) => fenced.has(type)), "every fixture is listed once: "
    + listed.filter((type, at) => listed.indexOf(type) !== at).join(",") + " listed twice; "
    + listed.filter((type) => !fenced.has(type)).join(",") + " without a fence; "
    + [...fenced].filter((type) => !listed.includes(type)).join(",") + " fenced but unlisted");
console.log(`mermaid-types: ${checks} check(s), ${failures} failed`);
finish(failures);
