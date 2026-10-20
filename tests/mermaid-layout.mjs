// Pele lays out the figures: dagre-style cycle breaking in flowcharts, mirrored boxes and spaced messages in a sequence.
import { FACES, actors, edges, frames, layered, lifelines, load, messages, nodes, notes, notesOf, others, parse, sequences, texts } from "./mermaid-corpus.mjs";
import { argv, finish } from "./js-runtime.mjs";

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
const LAYOUT_FACE = FACES[0];
// A tolerance for the float arithmetic of the layout's coordinates, in px.
const EPSILON = 0.01;
// The gap between a message label's descent and its arrow stays at least this, so a label never sits on its arrow.
const MESSAGE_GAP_MIN = 2;
const DESCENT_RATIO = 0.3;

// Sample input: "A -->|Yes| B" reads the pair [A, B]; "A --> B --> C" reads two; "X ||--o{ Y : a" is an entity pair; the header line is skipped.
function readSource(source) {
    const order = [];
    const edges = [];
    const see = (id) => { if (!order.includes(id)) order.push(id); };
    const arrow = /\s*(?:-{2,}>|={2,}>|-{3,})(?:\|[^|]*\|)?\s*/;
    for (const line of source.split("\n").slice(1)) {
        if (/^(subgraph|end$|direction)/.test(line))
            continue;
        const relation = line.match(/^(\w+) \|\|--o\{ (\w+) : /);
        const ids = relation ? [relation[1], relation[2]] : line.split(arrow).map((piece) => piece.match(/^[A-Za-z0-9_]+/)[0]);
        ids.forEach(see);
        for (let i = 1; i < ids.length; i++)
            edges.push([ids[i - 1], ids[i]]);
    }
    return { order, edges };
}
// The direction each diagram kind flows: a flowchart names it, and the state, class and ER layouts run top down, as mermaid.js lays them.
function directionOf(source) {
    return (source.match(/^flowchart (\w+)/) ?? [0, "TD"])[1];
}
// dagre's acyclic pass (dfsFAS): nodes in declaration order, out edges in declaration order, an edge to a node on the DFS stack is reversed.
function dagreReversed(order, edges) {
    const visited = new Set();
    const stack = new Set();
    const reversed = new Set();
    function dfs(v) {
        if (visited.has(v))
            return;
        visited.add(v);
        stack.add(v);
        for (const [from, to] of edges) {
            if (from !== v || from === to)
                continue;
            if (stack.has(to))
                reversed.add(from + ">" + to);
            else
                dfs(to);
        }
        stack.delete(v);
    }
    order.forEach(dfs);
    return reversed;
}

// A label mixing words and tspans reads in document order, so a styled middle word never moves to the end.
// Sample input: <text>a <tspan>b</tspan> c</text> reads "a b c".
const mixedOrder = texts(parse('<svg><text x="0" y="0">a <tspan>b</tspan> c</text></svg>'))[0].string;
check(mixedOrder === "a b c", "mixed content keeps document order: " + mixedOrder);

for (const [name, source] of layered) {
    const tree = parse(render(source, LAYOUT_FACE));
    const { order, edges: sourceEdges } = readSource(source);
    const direction = directionOf(source);
    const graph = nodes(tree);
    const box = new Map(graph.map((n) => [n.id, n.box]));
    const drawnEdges = edges(tree, graph);
    const drawn = drawnEdges.map((e) => e.from + ">" + e.to).sort();
    check(JSON.stringify(drawn) === JSON.stringify(sourceEdges.map((e) => e.join(">")).sort()), `${name}: the reader and the figure agree on the edge set, ${drawn.join(" ")}`);
    // Flow position along the direction: a larger value is later in the flow.
    const horizontal = direction === "LR" || direction === "RL";
    const sign = direction === "BT" || direction === "RL" ? -1 : 1;
    const span = (id) => horizontal ? [box.get(id)[0], box.get(id)[2]] : [box.get(id)[1], box.get(id)[3]];
    const start = (id) => sign > 0 ? span(id)[0] : -span(id)[1];
    const end = (id) => sign > 0 ? span(id)[1] : -span(id)[0];
    if (!sourceEdges.every((e) => e.every((id) => box.has(id) && box.get(id) !== null))) {
        check(false, `${name}: every edge end is a drawn node`);
        continue;
    }
    const against = new Set(sourceEdges.filter(([from, to]) => from !== to && start(to) < start(from)).map((e) => e.join(">")));
    const expected = dagreReversed(order, sourceEdges);
    check(JSON.stringify([...against].sort()) === JSON.stringify([...expected].sort()),
        `${name}: edges drawn against the flow [${[...against].sort()}] equal dagre's reversed set [${[...expected].sort()}]`);
    for (const [from, to] of sourceEdges) {
        if (from === to || expected.has(from + ">" + to))
            continue;
        check(start(to) > end(from) - EPSILON, `${name}: ${to} sits in a layer after its predecessor ${from}`);
    }
    // A forward edge never doubles back: along the flow, each point of its line is no earlier than the one before.
    for (const drawnEdge of drawnEdges) {
        if (expected.has(drawnEdge.from + ">" + drawnEdge.to) || drawnEdge.from === drawnEdge.to)
            continue;
        const along = drawnEdge.points.map((p) => sign * (horizontal ? p[0] : p[1]));
        check(along.every((v, i) => i === 0 || v >= along[i - 1] - EPSILON), `${name}: the forward edge ${drawnEdge.from} to ${drawnEdge.to} runs one way along the flow, ${along.map((v) => v.toFixed(0))}`);
    }
}

// A note stands beside its lifeline within this, in px; mermaid's own margin is 25.
const NOTE_BESIDE_MAX = 40;
const noteTree = parse(render(notes[1], LAYOUT_FACE));
const lifelineX = new Map(lifelines(noteTree).map((l) => [l.id, l.x]));
// Sample input: "Note over A,C: spans all three" reads { position: "over", ids: ["A", "C"] } for the text "spans all three".
const noteLines = new Map(notes[1].split("\n").map((l) => l.match(/^Note (over|left of|right of) ([\w,]+): (.*)$/)).filter((m) => m)
    .map((m) => [m[3], { position: m[1].split(" ")[0], ids: m[2].split(",") }]));
const drawnNotes = notesOf(noteTree);
check(drawnNotes.length === noteLines.size, `the notes source draws ${drawnNotes.length} notes, want ${noteLines.size}`);
for (const note of drawnNotes) {
    const [left, , right] = note.box;
    const name = `note "${note.texts[0].string}"`;
    const line = noteLines.get(note.texts[0].string);
    check(line !== undefined, `${name} is a note of the source`);
    if (line === undefined)
        continue;
    const { position, ids } = line;
    const first = lifelineX.get(ids[0]);
    const last = lifelineX.get(ids[ids.length - 1]);
    if (position === "over" && ids.length > 1) {
        check(left < first && right > last, `${name} spans lifeline ${ids[0]} at ${first} to ${ids[ids.length - 1]} at ${last}, drawn ${left} to ${right}`);
        const outside = [...lifelineX.values()].filter((x) => x < first - EPSILON || x > last + EPSILON);
        check(outside.every((x) => x < left || x > right), `${name} covers no lifeline outside its own`);
    } else if (position === "over") {
        check(Math.abs((left + right) / 2 - first) < EPSILON, `${name} is centred on its lifeline ${first}, drawn ${left} to ${right}`);
    } else if (position === "left") {
        check(right < first && first - right <= NOTE_BESIDE_MAX, `${name} sits left of its lifeline ${first}, ending at ${right}`);
    } else {
        check(left > first && left - first <= NOTE_BESIDE_MAX, `${name} sits right of its lifeline ${first}, starting at ${left}`);
    }
}

// A frame an edge enters ranks after the frame the edge leaves, so one sits below three in the docs example.
const SIBLING_GAP_MIN = 1;
const source = "flowchart TB\nc1 --> a2\nsubgraph one\na1 --> a2\nend\nsubgraph two\nb1 --> b2\nend\nsubgraph three\nc1 --> c2\nend";
const rowTree = parse(render(source, LAYOUT_FACE));
const [one, two, three] = ["one", "two", "three"].map((id) => frames(rowTree).get(id)?.box ?? null);
check(one !== null && two !== null && three !== null, "docs example: the three frames are drawn");
if (one !== null && two !== null && three !== null) {
    const along = Math.min(two[3], three[3]) - Math.max(two[1], three[1]);
    check(two[2] + SIBLING_GAP_MIN <= three[0] || three[2] + SIBLING_GAP_MIN <= two[0], `docs example: two and three stand side by side, not on top of each other (${two} against ${three})`);
    check(along > 0, `docs example: two and three share a row, overlapping ${along.toFixed(1)} px`);
    check(one[1] >= three[3] + SIBLING_GAP_MIN, `docs example: one, which c1 --> a2 enters, sits below three, which it leaves (${one} against ${three})`);
    check(one[1] >= two[3] + SIBLING_GAP_MIN || one[2] + SIBLING_GAP_MIN <= two[0] || two[2] + SIBLING_GAP_MIN <= one[0], `docs example: one does not cross two (${one} against ${two})`);
}

// A frame holds only its own members, and frames that are not nested never cross: the clash sources fall back to the nested layout, so a stubbed clash check goes red here.
// Sample input: "flowchart TB\nsubgraph a [A]\nx --> y\nend\nsubgraph b\nsubgraph c\nz\nend\nend" reads members a: x y, b: z, c: z and parents a: null, b: null, c: b.
function readFrames(source) {
    const members = new Map();
    const parents = new Map();
    const open = [];
    for (const line of source.split("\n").slice(1)) {
        const header = line.match(/^subgraph (\w+)/);
        if (header) {
            members.set(header[1], new Set());
            parents.set(header[1], open.length ? open[open.length - 1] : null);
            open.push(header[1]);
        } else if (line === "end") {
            open.pop();
        } else {
            for (const piece of line.split(/\s*(?:-{2,}>|-{3,})(?:\|[^|]*\|)?\s*/)) {
                const id = (piece.match(/^[A-Za-z0-9_]+/) ?? [null])[0];
                for (const frameId of open)
                    if (id) members.get(frameId).add(id);
            }
        }
    }
    return { members, parents };
}
const overlap = (a, b) => Math.min(a[2], b[2]) - Math.max(a[0], b[0]) > EPSILON && Math.min(a[3], b[3]) - Math.max(a[1], b[1]) > EPSILON;
const clashSources = [
    ["flattened nested frames", "flowchart TB\nsubgraph outer\nsubgraph inner\nm --> k\nend\nn --> m\nend\nk --> z"],
    ["nested frames with an outside edge", layered.find(([name]) => name === "nested subgraphs")[1]],
    ["docs example left to right", "flowchart LR\nc1 --> a2\nsubgraph one\na1 --> a2\nend\nsubgraph two\nb1 --> b2\nend\nsubgraph three\nc1 --> c2\nend"],
    ["right to left subgraph title", others.find(([name]) => name === "right to left subgraph title")[1]]
];
for (const [name, clashSource] of clashSources) {
    const tree = parse(render(clashSource, LAYOUT_FACE));
    const { members, parents } = readFrames(clashSource);
    check(members.size > 0 && [...members.values()].every((m) => m.size > 0), `${name}: the reader finds frames that hold members, so the checks below can go red`);
    const drawnFrames = frames(tree);
    const outlines = new Map([...members.keys()].map((id) => [id, drawnFrames.get(id)?.box ?? null]));
    const boxes = new Map(nodes(tree).map((n) => [n.id, n.box]));
    const nested = (a, b) => { for (let at = a; at; at = parents.get(at)) if (at === b) return true; return false; };
    for (const [id, f] of outlines) {
        check(f !== null, `${name}: frame ${id} is drawn`);
        if (f === null)
            continue;
        for (const [nodeId, nodeBox] of boxes)
            check(members.get(id).has(nodeId) === overlap(f, nodeBox), `${name}: frame ${id} ${members.get(id).has(nodeId) ? "holds its member" : "keeps out the stranger"} ${nodeId}, frame ${f} against node ${nodeBox}`);
    }
    for (const a of outlines.keys()) {
        for (const b of outlines.keys()) {
            if (a < b && outlines.get(a) && outlines.get(b) && !nested(a, b) && !nested(b, a))
                check(!overlap(outlines.get(a), outlines.get(b)), `${name}: sibling frames ${a} and ${b} do not cross, ${outlines.get(a)} against ${outlines.get(b)}`);
        }
    }
}

for (const [name, source] of sequences) {
    const tree = parse(render(source, LAYOUT_FACE));
    // Sample input: "participant A as Alice" reads the id A.
    const ids = [...new Set(source.split("\n").filter((l) => /^participant/.test(l)).map((l) => l.split(" ")[1]))];
    const boxes = actors(tree);
    const lines = lifelines(tree);
    check(boxes.length === ids.length * 2, `${name}: ${ids.length} participants draw ${boxes.length} boxes, want one top and one bottom each`);
    for (const id of ids) {
        const own = boxes.filter((a) => a.id === id).map((a) => ({ r: a.box, t: a.texts[0], footer: a.footer })).sort((p, q) => p.r[1] - q.r[1]);
        const line = lines.find((l) => l.id === id);
        check(own.length === 2 && !own[0].footer && own[1].footer, `${name}: ${id} has a top and a bottom box`);
        if (own.length !== 2 || line === undefined)
            continue;
        const [top, bottom] = own;
        check(top.r[2] - top.r[0] === bottom.r[2] - bottom.r[0] && top.r[3] - top.r[1] === bottom.r[3] - bottom.r[1], `${name}: ${id}'s bottom box has the size of its top box`);
        check(top.r[0] === bottom.r[0] && top.t.string === bottom.t.string && top.t.x === bottom.t.x, `${name}: ${id}'s bottom box has the label and column of its top box`);
        check(Math.abs(top.r[3] - line.top) < EPSILON, `${name}: ${id}'s lifeline starts where its top box ends, ${line.top} against ${top.r[3]}`);
        check(Math.abs(bottom.r[1] - line.end) < EPSILON, `${name}: ${id}'s bottom box begins where its lifeline ends, ${bottom.r[1]} against ${line.end}`);
    }
    // A message between two lifelines is one level line with its label above it; a self message loops beside its lifeline instead.
    for (const m of messages(tree)) {
        const y = m.points.length ? m.points[0][1] : NaN;
        if (!m.label || !m.points.every((p) => Math.abs(p[1] - y) < EPSILON))
            continue;
        check(m.label.y + DESCENT_RATIO * m.label.size <= y - MESSAGE_GAP_MIN, `${name}: "${m.label.string}" sits above its arrow, baseline ${m.label.y} against ${y}`);
    }
}
console.log(`mermaid-layout: ${checks} check(s), ${failures} failed`);
finish(failures);
