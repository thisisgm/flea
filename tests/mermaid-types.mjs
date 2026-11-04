// Every diagram type the bundle draws, one at a time through the real figure path, so a dropped or broken type fails here by name.
import { FACES, actors, bounds, decode, edgeLabels, edges, find, frame, load, messages, nodes, parse, view } from "./mermaid-corpus.mjs";
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
// v0.3.12's figure geometry as the HIG figure boards draw it (PELE-GEOM), one check per gap.
const near = (a, b) => Math.abs(a - b) < 0.01;
const flowTree = parse(render("flowchart TD\n    A --> B", FACES[0]));
const [boxA, boxB] = ["A", "B"].map((id) => (nodes(flowTree).find((n) => n.id === id) || { box: [NaN, NaN, NaN, NaN] }).box);
check([boxA, boxB].every((b) => near(b[2] - b[0], 60) && near(b[3] - b[1], 36.9)), `flowchart: boxes 60 by 36.9, drew ${JSON.stringify([boxA, boxB])}`);
const boxRects = find(flowTree, "pele-node").map((n) => n.children.find((c) => c.name === "rect"));
check(boxRects.length === 2 && boxRects.every((r) => r && r.attrs.rx === undefined && !/r[xy]\s*:/.test(r.attrs.style ?? "") && r.attrs["stroke-width"] === "0.75"),
    `flowchart: square boxes under a 0.75 stroke, drew ${JSON.stringify(boxRects.map((r) => r && r.attrs))}`);
// Sample input: a marker d="M25,85.9L21.64,77.9L28.36,77.9Z" reads its tip [25, 85.9].
const tipsOf = (source) => find(parse(render(source, FACES[0])), "pele-marker").map((m) => (m.attrs.d.match(/^M([-\d.]+),([-\d.]+)/) || []).slice(1).map(Number));
const tip = tipsOf("flowchart TD\n    A --> B")[0][1];
check(near(boxB[1] - boxA[3], 48) && near(tip, boxB[1]), `flowchart: boxes 48 apart and the arrow ends on the lower box, gap ${boxB[1] - boxA[3]}, tip ${tip} for top ${boxB[1]}`);
const wideFlow = parse(render("flowchart TD\n    A[Some longer label here] --> B", FACES[0]));
const wideBox = (nodes(wideFlow).find((n) => n.id === "A") || { box: [0, 0, 0, 0] }).box;
check(near(wideBox[2] - wideBox[0], 22 * 0.6 * 13 + 41.95), `flowchart: a box is 41.95 wider than its label, drew ${wideBox[2] - wideBox[0]}`);
function rows(source, extra) {
    const tree = parse(render(source, FACES[0], extra));
    const top = actors(tree).find((a) => !a.footer);
    const foot = actors(tree).find((a) => a.footer);
    const lines = messages(tree).map((m) => m.points[0][1]);
    return { top: top ? top.box : [], foot: foot ? foot.box : [], lines, label: (messages(tree)[0] || {}).label };
}
const one = rows("sequenceDiagram\n    A->>B: hi");
check(near(one.lines[0] - one.top[3], 20) && near(one.foot[1] - one.lines[0], 40) && one.label && near(one.lines[0] - one.label.y, 6.15),
    `sequence: the message 20 under the participants, its label 6.15 above it, the closing row 40 under it, drew ${JSON.stringify(one)}`);
const three = rows("sequenceDiagram\n    A->>B: hi\n    B-->>A: back\n    A->>B: third");
check(three.lines.length === 3 && near(three.lines[1] - three.lines[0], 40) && near(three.lines[2] - three.lines[1], 40) && near(three.foot[1] - three.lines[2], 40),
    `sequence: messages 40 apart and the closing row 40 under the last, drew ${JSON.stringify(three.lines)} closing at ${three.foot[1]}`);
const ownMargin = rows('%%{init: {"sequence": {"messageMargin": 35}}}%%\nsequenceDiagram\n    A->>B: hi');
check(near(ownMargin.lines[0] - ownMargin.top[3], 39), `sequence: a source's own messageMargin keeps Pele's rows, drew ${ownMargin.lines[0] - ownMargin.top[3]}`);
const frontMargin = rows("---\nconfig:\n  sequence:\n    messageMargin: 35\n---\nsequenceDiagram\n    A->>B: hi");
check(near(frontMargin.lines[0] - frontMargin.top[3], 39), `sequence: front matter's own messageMargin keeps Pele's rows, drew ${frontMargin.lines[0] - frontMargin.top[3]}`);
const wordy = rows("sequenceDiagram\n    A->>B: set messageMargin");
check(near(wordy.lines[0] - wordy.top[3], 20), `sequence: a message reading messageMargin still rises 20 under the participants, drew ${wordy.lines[0] - wordy.top[3]}`);
// Pele's own gaps under the participants for each shape whose rows stay as Pele drew them.
for (const [name, source, gap] of [["a self message", "sequenceDiagram\n    A->>A: self\n    A->>B: hi", 27], ["a destroy", "sequenceDiagram\n    A->>B: hi\n    destroy B\n    A->>B: bye", 46],
    ["a created participant", "sequenceDiagram\n    A->>B: hi\n    create participant C\n    A->>C: new", 46], ["a rect", "sequenceDiagram\n    rect rgb(0,0,0)\n    A->>B: hi\n    end", 54],
    ["no closing row", '%%{init: {"sequence": {"mirrorActors": false}}}%%\nsequenceDiagram\n    A->>B: hi', 46]]) {
    const kept = rows(source);
    check(near(kept.lines[0] - kept.top[3], gap), `sequence: ${name} keeps Pele's rows, drew ${kept.lines[0] - kept.top[3]}`);
}
const counted = parse(render("sequenceDiagram\n    autonumber\n    A->>B: hi", FACES[0]));
const mark = bounds(find(counted, "pele-sequence-number")[0]);
const countedLine = messages(counted)[0].points[0][1];
check(near((mark[1] + mark[3]) / 2, countedLine) && near(countedLine - actors(counted).find((a) => !a.footer).box[3], 20), `sequence: an autonumber mark rises with its message, drew ${mark} for line ${countedLine}`);
const active = parse(render("sequenceDiagram\n    A->>+B: hi\n    B-->>-A: back", FACES[0]));
const bar = bounds(find(active, "pele-activations")[0]);
const activeLine = messages(active)[0].points[0][1];
check(near(activeLine - actors(active).find((a) => !a.footer).box[3], 20) && near(bar[1], activeLine), `sequence: an activation rises with its first message, drew bar ${bar} for line ${activeLine}`);
for (const [kind, source] of [["pele-note", "sequenceDiagram\n    Note over A: first\n    A->>B: hi"], ["pele-frame", "sequenceDiagram\n    loop every\n    A->>B: hi\n    end"]]) {
    const tree = parse(render(source, FACES[0]));
    const head = actors(tree).find((a) => !a.footer).box[3];
    const tops = find(tree, kind).map((g) => bounds(g)[1]);
    check(tops.length === 1 && tops[0] > head, `sequence: a ${kind} before the first message keeps Pele's rows under the participants, drew ${tops} under ${head}`);
}
const boxesOf = (source) => Object.fromEntries(nodes(parse(render(source, FACES[0]))).map((n) => [n.id, n.box]));
const lr = boxesOf("flowchart LR\n    A --> B");
check(near(lr.B[0] - lr.A[2], 48) && near(lr.A[3] - lr.A[1], 36.9) && near(tipsOf("flowchart LR\n    A --> B")[0][0], lr.B[0]), `flowchart LR: ranks close along x, drew ${JSON.stringify(lr)}`);
const graph = boxesOf("graph TD\n    A --> B");
check(near(graph.A[2] - graph.A[0], 60) && near(graph.B[1] - graph.A[3], 48), `graph TD: the same boxes as flowchart, drew ${JSON.stringify(graph)}`);
const twoLines = parse(render('flowchart TD\n    A["one<br>two"] --> B', FACES[0]));
const spans = find(twoLines, "pele-node").find((n) => n.attrs["data-id"] === "A").children.filter((c) => c.name === "text").map((t) => Number(t.attrs.y));
check(near(bounds(find(twoLines, "pele-node")[0])[3] - bounds(find(twoLines, "pele-node")[0])[1], 2 * 16.9 + 20) && near(spans[1] - spans[0], 16.9), `flowchart: a two-line box is 53.8 high with lines 16.9 apart, drew ${spans}`);
const labelled = parse(render("flowchart TD\n    A -->|yes| B", FACES[0]));
const [edge] = edges(labelled);
const pill = edgeLabels(labelled)[0].box;
check(near((pill[1] + pill[3]) / 2, (edge.points[0][1] + edge.points[edge.points.length - 1][1]) / 2), `flowchart: an edge label stays on its edge's middle, drew ${pill} on ${JSON.stringify(edge.points)}`);
const mixed = boxesOf("flowchart TD\n    A --> B\n    A --> C{x}");
const mixedTips = tipsOf("flowchart TD\n    A --> B\n    A --> C{x}").map((p) => p[1]).sort((a, b) => a - b);
check(near(mixedTips[0], Math.min(mixed.B[1], mixed.C[1])) && near(mixedTips[1], Math.max(mixed.B[1], mixed.C[1])), `flowchart: a box beside a taller diamond keeps its arrow on its top, drew ${mixedTips} for ${JSON.stringify(mixed)}`);
const framed = parse(render("flowchart TD\n    subgraph S [Sub]\n    A --> B\n    end", FACES[0]));
check(near(frame(framed, "S")[3] - nodes(framed).find((n) => n.id === "B").box[3], 20), `flowchart: a subgraph closes 20 under its last box, drew ${frame(framed, "S")}`);
const classed = parse(render("flowchart TD\n    subgraph S [Sub]\n    A --> B\n    end\n    classDef foo fill:#f00\n    class S foo", FACES[0]));
check(near(frame(classed, "S")[3] - nodes(classed).find((n) => n.id === "B").box[3], 20), `flowchart: a subgraph carrying a class closes 20 under its last box, drew ${frame(classed, "S")}`);
const sideBySide = "flowchart TD\n    subgraph X\n    subgraph Z\n    A --> B\n    end\n    end\n    subgraph Y\n    C --> D\n    end";
const banded = boxesOf(sideBySide);
// Pele sets Y's ranks 8 above X's, and one band keeps that offset and spacing, so both columns' gaps open alike.
const bandOffset = -8;
const bandTips = tipsOf(sideBySide).map((p) => p[1]).sort((a, b) => a - b);
check(banded.B[1] - banded.A[3] > 48 - 0.01 && near(banded.B[1] - banded.A[3], banded.D[1] - banded.C[3]) && near(banded.C[1] - banded.A[1], bandOffset) && near(bandTips[0], Math.min(banded.B[1], banded.D[1])) && near(bandTips[1], Math.max(banded.B[1], banded.D[1])),
    `flowchart: side by side subgraphs whose ranks overlap move as one band, each column's boxes never closer than 48, drew ${JSON.stringify(banded)} tips ${bandTips}`);
const rounder = find(parse(render("flowchart TD\n    A --> B\n    style B rx:10px", FACES[0])), "pele-node").find((n) => n.attrs["data-id"] === "B").children[0];
check(rounder.attrs.rx === "10" && rounder.attrs.width === "60" && rounder.attrs.height === "36.9", `flowchart: a source's own rx wins over the square box it redraws, drew ${JSON.stringify(rounder.attrs)}`);
for (const [name, source] of [["sequence", "sequenceDiagram\n    A->>B: hi"], ["flowchart", "flowchart TD\n    A --> B"], ["pie", "pie\n    \"a\": 1"]]) {
    const at13 = render(source, FACES[0], { bodyPx: 13 });
    const at14 = render(source, FACES[0], { bodyPx: 14 });
    // Sample input: '<svg viewBox="0 0 50 138" width="53.8" height="148.6">' reads [53.8, 148.6].
    const size = (svg) => ["width", "height"].map((key) => Number(svg.match(new RegExp(`^<svg\\b[^>]*\\s${key}="([^"]*)"`))[1]));
    check(view(at14).join(" ") === view(at13).join(" ") && size(at14).every((v, i) => Math.abs(v - size(at13)[i] * 14 / 13) < 0.002),
        `${name}: a body 14 figure is the body 13 drawing scaled 14/13, drew ${size(at13)} then ${size(at14)}`);
}
// The list and the fixtures name each other: a missing or extra fence goes red here, not as a short count.
const fenced = new Set(readDir(fixtures).filter((name) => name.endsWith(".md")).map((name) => name.slice(0, -".md".length)));
const listed = types.map(([type]) => type);
check(new Set(listed).size === listed.length && listed.length === fenced.size && listed.every((type) => fenced.has(type)), "every fixture is listed once: "
    + listed.filter((type, at) => listed.indexOf(type) !== at).join(",") + " listed twice; "
    + listed.filter((type) => !fenced.has(type)).join(",") + " without a fence; "
    + [...fenced].filter((type) => !listed.includes(type)).join(",") + " fenced but unlisted");
console.log(`mermaid-types: ${checks} check(s), ${failures} failed`);
finish(failures);
