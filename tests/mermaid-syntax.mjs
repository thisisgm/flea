// Mermaid's flowchart syntax reads as mermaid.js reads it: every documented link form, and which subgraph a node belongs to.
import { FACES, edges, frame, load, nodes, parse } from "./mermaid-corpus.mjs";
import { argv, finish } from "./js-runtime.mjs";

const render = await load(argv[0]);
const face = FACES[0];
let checks = 0;
let failures = 0;
function check(ok, why) {
    checks++;
    if (!ok) {
        failures++;
        console.log("FAIL " + why);
    }
}
// A tolerance for the float arithmetic of the layout's coordinates, in px.
const EPSILON = 0.01;
// A link one rank longer must sit at least this much further from its source, in px; the library's layer spacing is 48.
const RANK_STEP_MIN = 40;
// The stroke widths of a plain and a thick link, in px.
const PLAIN_WIDTH = 1;
const THICK_WIDTH = 2.5;

const edge = (from, to, style, start, end, length = 1, label = "") => ({ from, to, style, start, end, length, label });
// Every documented link form of mermaid's flowchart docs: [source, the edges it draws].
const forms = [
    ["A --> B", [edge("A", "B", "solid", "none", "arrow")]],
    ["A --- B", [edge("A", "B", "solid", "none", "none")]],
    ["A -->|t| B", [edge("A", "B", "solid", "none", "arrow", 1, "t")]],
    ["A ---|t| B", [edge("A", "B", "solid", "none", "none", 1, "t")]],
    ["A -- t --> B", [edge("A", "B", "solid", "none", "arrow", 1, "t")]],
    ["A -- t --- B", [edge("A", "B", "solid", "none", "none", 1, "t")]],
    ["A-- text with spaces -->B", [edge("A", "B", "solid", "none", "arrow", 1, "text with spaces")]],
    ["A-->B", [edge("A", "B", "solid", "none", "arrow")]],
    ["my-node --> other-node", [edge("my-node", "other-node", "solid", "none", "arrow")]],
    ["A -.-> B", [edge("A", "B", "dotted", "none", "arrow")]],
    ["A -.- B", [edge("A", "B", "dotted", "none", "none")]],
    ["A -. t .-> B", [edge("A", "B", "dotted", "none", "arrow", 1, "t")]],
    ["A -. t .- B", [edge("A", "B", "dotted", "none", "none", 1, "t")]],
    ["A ==> B", [edge("A", "B", "thick", "none", "arrow")]],
    ["A === B", [edge("A", "B", "thick", "none", "none")]],
    ["A == t ==> B", [edge("A", "B", "thick", "none", "arrow", 1, "t")]],
    ["A == t === B", [edge("A", "B", "thick", "none", "none", 1, "t")]],
    ["A ---> B", [edge("A", "B", "solid", "none", "arrow", 2)]],
    ["A ----> B", [edge("A", "B", "solid", "none", "arrow", 3)]],
    ["A ----- B", [edge("A", "B", "solid", "none", "none", 3)]],
    ["A -..-> B", [edge("A", "B", "dotted", "none", "arrow", 2)]],
    ["A -...-> B", [edge("A", "B", "dotted", "none", "arrow", 3)]],
    ["A ===> B", [edge("A", "B", "thick", "none", "arrow", 2)]],
    ["A ==== B", [edge("A", "B", "thick", "none", "none", 2)]],
    ["A --->|t| B", [edge("A", "B", "solid", "none", "arrow", 2, "t")]],
    ["A <--> B", [edge("A", "B", "solid", "arrow", "arrow")]],
    ["A <-.-> B", [edge("A", "B", "dotted", "arrow", "arrow")]],
    ["A <==> B", [edge("A", "B", "thick", "arrow", "arrow")]],
    ["A --o B", [edge("A", "B", "solid", "none", "circle")]],
    ["A --x B", [edge("A", "B", "solid", "none", "cross")]],
    ["A o--o B", [edge("A", "B", "solid", "circle", "circle")]],
    ["A x--x B", [edge("A", "B", "solid", "cross", "cross")]],
    ["A & B --> C", [edge("A", "C", "solid", "none", "arrow"), edge("B", "C", "solid", "none", "arrow")]],
    ["A --> B & C", [edge("A", "B", "solid", "none", "arrow"), edge("A", "C", "solid", "none", "arrow")]],
    ["A & B -->|t| C & D", ["A", "B"].flatMap((s) => ["C", "D"].map((d) => edge(s, d, "solid", "none", "arrow", 1, "t")))],
    ["A --> B == t ==> C", [edge("A", "B", "solid", "none", "arrow"), edge("B", "C", "thick", "none", "arrow", 1, "t")]],
    ["A ~~~ B", []]
];

// Sample input: <g class="pele-edge" data-id="L_A_B_0"><path d="M25,46L25,87" stroke-dasharray="3 4"/><path class="pele-marker" d="M25,94L21.64,86L28.36,86Z"/></g>
// reads the edge A to B, dotted, with an arrow at its end.
function drawnEdges(tree) {
    return edges(tree).map((e) => ({ from: e.from, to: e.to, style: e.dashed ? "dotted" : e.width === THICK_WIDTH ? "thick" : "solid",
        start: e.start, end: e.end, label: e.label, width: e.width }));
}
const key = (e) => [e.from, e.to, e.style, e.start, e.end, e.label].join(" ");
for (const [source, want] of forms) {
    const tree = parse(render("flowchart TD\n" + source, face));
    const drawn = drawnEdges(tree);
    check(JSON.stringify(drawn.map(key).sort()) === JSON.stringify(want.map(key).sort()),
        `${source}: draws [${drawn.map(key).join("; ")}], want [${want.map(key).join("; ")}]`);
    check(drawn.every((e) => e.width === (e.style === "thick" ? THICK_WIDTH : PLAIN_WIDTH)), `${source}: dotted and plain links are plain width and thick is heavier`);
    const labels = drawn.map((e) => e.label).filter((l) => l !== "");
    check(JSON.stringify(labels.sort()) === JSON.stringify(want.map((e) => e.label).filter((l) => l !== "").sort()), `${source}: labels [${labels}]`);
    const drawnNodes = nodes(tree).map((n) => n.id).sort();
    const wantNodes = want.length === 0 ? ["A", "B"] : [...new Set(want.flatMap((e) => [e.from, e.to]))].sort();
    check(JSON.stringify(drawnNodes) === JSON.stringify(wantNodes), `${source}: nodes [${drawnNodes}], want [${wantNodes}]`);
}

// How far node B sits below node A, in px, in a top-down graph.
function drop(link) {
    const box = new Map(nodes(parse(render("flowchart TD\nA " + link + " B", face))).map((n) => [n.id, n.box]));
    return box.has("A") && box.has("B") ? box.get("B")[1] - box.get("A")[3] : NaN;
}
// A longer link spans more ranks: each extra dash, dot or equals sign moves the target a rank further down.
for (const [family, links] of [["solid", ["-->", "--->", "---->", "----->"]], ["dotted", ["-.->", "-..->", "-...->"]], ["thick", ["==>", "===>", "====>"]]]) {
    const drops = links.map(drop);
    for (let i = 1; i < drops.length; i++)
        check(drops[i] >= drops[0] + i * RANK_STEP_MIN - EPSILON, `${family} link ${links[i]} drops ${drops[i].toFixed(1)}, want at least ${(drops[0] + i * RANK_STEP_MIN).toFixed(1)} beyond ${links[0]} at ${drops[0].toFixed(1)}`);
}
// The edge a longer link draws is one unbroken line from its source to its target, not one piece per rank.
const longEdge = edges(parse(render("flowchart TD\nA ------> B", face)));
check(longEdge.length === 1 && longEdge[0].from === "A" && longEdge[0].to === "B", `a long link draws one edge, got ${longEdge.length}`);
// An invisible link orders its ends and draws nothing.
const invisible = parse(render("flowchart TD\nA ~~~ B", face));
const invisibleBoxes = new Map(nodes(invisible).map((n) => [n.id, n.box]));
check(edges(invisible).length === 0, "an invisible link draws no edge");
check(invisibleBoxes.has("B") && invisibleBoxes.get("A")[3] <= invisibleBoxes.get("B")[1] + EPSILON, "an invisible link still ranks B below A");

// A node belongs to the first subgraph whose block closes with it named, as mermaid's flowDb keeps it; an earlier mention outside any block does not count.
const subgraphs = [
    ["docs example", "flowchart TB\nc1 --> a2\nsubgraph one\na1 --> a2\nend\nsubgraph two\nb1 --> b2\nend\nsubgraph three\nc1 --> c2\nend",
        { one: ["a1", "a2"], two: ["b1", "b2"], three: ["c1", "c2"] }],
    ["a node named in two sibling blocks", "flowchart TB\nsubgraph s1\nx --> y\nend\nsubgraph s2\ny --> z\nend",
        { s1: ["x", "y"], s2: ["z"] }],
    ["a node named outside, then in a block", "flowchart LR\nlone --> p\nsubgraph box\nq\np\nend\nlone --> q",
        { box: ["p", "q"] }],
    ["an inner block takes a node its outer block names", "flowchart TB\nsubgraph outer\nn --> m\nsubgraph inner\nm --> k\nend\nend",
        { outer: ["n", "m", "k"], inner: ["m", "k"] }]
];
for (const [name, source, members] of subgraphs) {
    const tree = parse(render(source, face));
    const box = new Map(nodes(tree).map((n) => [n.id, n.box]));
    for (const [id, held] of Object.entries(members)) {
        const outline = frame(tree, id);
        check(outline !== null, `${name}: subgraph ${id} is drawn`);
        if (outline === null)
            continue;
        for (const node of box.keys()) {
            const b = box.get(node);
            const inside = b[0] >= outline[0] - EPSILON && b[1] >= outline[1] - EPSILON && b[2] <= outline[2] + EPSILON && b[3] <= outline[3] + EPSILON;
            check(inside === held.includes(node), `${name}: ${node} ${inside ? "sits in" : "sits outside"} subgraph ${id}, want ${held.includes(node) ? "inside" : "outside"}`);
        }
    }
}
// A later bare mention of a node keeps the shape and label its first definition gave it, as mermaid.js keeps them.
const mentions = [
    ["a bare mention after a diamond", "flowchart TD\nA[Start] --> B{Is it?}\nB --> C", { B: ["diam", "Is it?"], C: ["rect", "C"] }],
    ["docs link length, D --> B after the diamond", "flowchart TD\nA[Start] --> B{Is it?}\nB -->|Yes| C[OK]\nC --> D[Rethink]\nD --> B\nB ----->|No| E[End]", { B: ["diam", "Is it?"], D: ["rect", "Rethink"] }]
];
for (const [name, source, expected] of mentions) {
    const drawn = nodes(parse(render(source, face)));
    for (const [id, [shape, label]] of Object.entries(expected)) {
        const node = drawn.find((n) => n.id === id);
        check(node !== undefined && node.shape === shape && node.label === label,
            `${name}: ${id} keeps ${shape} "${label}", got ${node === undefined ? "nothing" : node.shape + ' "' + node.label + '"'}`);
    }
}
console.log(`mermaid-syntax: ${checks} check(s), ${failures} failed`);
finish(failures);
