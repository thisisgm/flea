// The readers the Mermaid layout and fit checks share: an SVG as a tree, and its nodes, edges, lifelines and frames as numbers.

// Sample input: &lt;b&gt; reads <b>.
export function decode(text) {
    return text.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, "&");
}
// The weight from which the library measures with the bold table.
const BOLD_WEIGHT = 600;
const NAMED_WEIGHTS = { normal: 400, bold: 700 };
// Sample input: translate(10,4.5) reads [10, 4.5] and translate(3) reads [3, 0]; any other transform throws, so a reader never misplaces a shape.
function translation(transform) {
    if (transform === undefined)
        return [0, 0];
    const found = transform.trim().match(/^translate\(\s*(-?[\d.]+)(?:[\s,]+(-?[\d.]+))?\s*\)$/);
    if (!found)
        throw new Error("the readers place only a translate, got " + transform);
    return [Number(found[1]), Number(found[2] ?? 0)];
}
// The figure as a tree: each element has its name, its decoded attributes, its children in document order with their text, and its offset, font size and weight.
// Sample input: <svg font-size="14"><g transform="translate(5,6)"><text x="1" y="2">a &amp; b</text></g></svg> reads a text at offset 5,6, size 14, content "a & b".
export function parse(svg) {
    const root = { name: "#root", attrs: {}, children: [], seq: [], parent: null, dx: 0, dy: 0, size: NaN, weight: NAMED_WEIGHTS.normal };
    let at = root;
    for (const m of svg.matchAll(/<(\/?)([a-zA-Z][\w:-]*)((?:\s+[\w:-]+="[^"]*")*)\s*(\/?)>|([^<]+)/g)) {
        if (m[5] !== undefined) {
            at.seq.push(decode(m[5]));
            continue;
        }
        if (m[1]) {
            at = at.parent;
            continue;
        }
        const attrs = {};
        for (const a of m[3].matchAll(/([\w:-]+)="([^"]*)"/g))
            attrs[a[1]] = decode(a[2]);
        const [x, y] = translation(attrs.transform);
        const weight = attrs["font-weight"] === undefined ? at.weight : NAMED_WEIGHTS[attrs["font-weight"]] ?? Number(attrs["font-weight"]);
        const el = { name: m[2], attrs, children: [], seq: [], parent: at, dx: at.dx + x, dy: at.dy + y,
            size: attrs["font-size"] === undefined ? at.size : Number(attrs["font-size"]), weight };
        at.children.push(el);
        at.seq.push(el);
        if (!m[4])
            at = el;
    }
    return root;
}
// Every element under a node, depth first, in drawing order.
export function all(el) {
    return el.children.flatMap((c) => [c, ...all(c)]);
}
// Sample input: ("pele-node pele-shape-rect", "pele-node") answers true; a class is matched whole.
export function has(el, cls) {
    return (el.attrs.class ?? "").split(/\s+/).includes(cls);
}
// Every element under a node that carries a class.
export function find(el, cls) {
    return all(el).filter((c) => has(c, cls));
}
export function num(el, name) {
    return el.attrs[name] === undefined ? NaN : Number(el.attrs[name]);
}
// A position attribute SVG reads as 0 when it is absent.
// Sample input: <circle r="4"/> reads 0 for "cx".
function at0(el, name) {
    return el.attrs[name] === undefined ? 0 : Number(el.attrs[name]);
}
// The plain text of an element and its tspans in document order.
// Sample input: <text>a <tspan>b</tspan> c</text> reads "a b c".
function content(el) {
    return el.seq.map((part) => typeof part === "string" ? part : content(part)).join("");
}
// The texts under a node with their absolute anchor point, inherited size and weight, and plain content.
// Sample input: <g font-size="12"><text x="10" y="20" text-anchor="middle" font-weight="bold">Animal</text></g> reads anchor middle, size 12, bold.
export function texts(el) {
    return all(el).filter((c) => c.name === "text").map((t) => ({ x: t.dx + at0(t, "x"), y: t.dy + at0(t, "y"), size: t.size, bold: t.weight >= BOLD_WEIGHT,
        anchor: t.attrs["text-anchor"] ?? "start", string: content(t), family: inherited(t, "font-family") }));
}
// Sample input: a text under <svg font-family="monospace"> reads monospace for "font-family".
function inherited(el, name) {
    for (let at = el; at; at = at.parent) {
        if (at.attrs[name] !== undefined)
            return at.attrs[name];
    }
    return "";
}
// The vertices of a path, absolute: each command's end point, so a curve or an arc counts where it lands.
// Sample input: M0,59V347 at offset 10,0 reads [[10, 59], [10, 347]]; M1,2H5A2,2 0 0 1 7,4 reads [[1, 2], [5, 2], [7, 4]].
export function vertices(el) {
    const out = [];
    let x = 0;
    let y = 0;
    const COUNTS = { M: 2, L: 2, H: 1, V: 1, C: 6, S: 4, Q: 4, T: 2, A: 7, Z: 0 };
    for (const m of (el.attrs.d ?? "").matchAll(/([MLHVCSQTAZmlhvcsqtaz])([^MLHVCSQTAZmlhvcsqtaz]*)/g)) {
        const cmd = m[1].toUpperCase();
        const relative = m[1] !== cmd;
        const n = m[2].trim() === "" ? [] : m[2].trim().split(/[\s,]+|(?=-)/).filter((v) => v !== "").map(Number);
        const step = COUNTS[cmd];
        for (let i = 0; step > 0 && i + step <= n.length; i += step) {
            const args = n.slice(i, i + step);
            if (cmd === "H")
                x = relative ? x + args[0] : args[0];
            else if (cmd === "V")
                y = relative ? y + args[0] : args[0];
            else {
                x = relative ? x + args[step - 2] : args[step - 2];
                y = relative ? y + args[step - 1] : args[step - 1];
            }
            out.push([x, y]);
        }
    }
    return out.map(([px, py]) => [el.dx + px, el.dy + py]);
}
// The absolute box [left, top, right, bottom] of one shape, or null for a shape the readers do not size.
// Sample input: <rect x="3" y="4" width="40" height="20"/> reads [3, 4, 43, 24]; a polygon reads its points' extremes, a circle its radius, a path its vertices.
export function shapeBox(el) {
    if (el.name === "rect")
        return [el.dx + at0(el, "x"), el.dy + at0(el, "y"), el.dx + at0(el, "x") + num(el, "width"), el.dy + at0(el, "y") + num(el, "height")];
    if (el.name === "circle" || el.name === "ellipse") {
        const rx = el.name === "circle" ? num(el, "r") : num(el, "rx");
        const ry = el.name === "circle" ? num(el, "r") : num(el, "ry");
        return [el.dx + at0(el, "cx") - rx, el.dy + at0(el, "cy") - ry, el.dx + at0(el, "cx") + rx, el.dy + at0(el, "cy") + ry];
    }
    const p = el.name === "polygon" ? el.attrs.points.trim().split(/[\s,]+/).map(Number).reduce((pairs, v, i, a) => i % 2 ? pairs : [...pairs, [el.dx + v, el.dy + a[i + 1]]], [])
        : el.name === "path" ? vertices(el) : [];
    if (p.length === 0)
        return null;
    return [Math.min(...p.map((q) => q[0])), Math.min(...p.map((q) => q[1])), Math.max(...p.map((q) => q[0])), Math.max(...p.map((q) => q[1]))];
}
// The box of a group: its first sized shape, the outline every label of the group is laid out in.
export function bounds(el) {
    for (const c of all(el)) {
        if (c.name !== "text" && !has(c, "pele-divider")) {
            const box = shapeBox(c);
            if (box)
                return box;
        }
    }
    return null;
}
// The graph nodes of a flowchart, state, class or ER diagram: id, kind classes, box and texts.
// Sample input: a diam node B at translate(91,120) with a 40 by 30 polygon reads shape diam at box [51, 90, 131, 150].
export function nodes(tree) {
    return find(tree, "pele-node").filter((n) => !has(n, "pele-actor")).map((n) => {
        const shape = (n.attrs.class.match(/pele-shape-([\w-]+)/) ?? [null, ""])[1];
        const label = all(n).find((c) => has(c, "pele-label"));
        return { id: n.attrs["data-id"], el: n, shape, box: bounds(n), label: label ? content(label) : "", texts: texts(n) };
    });
}
// Sample input: a point 2 px right of the box [0, 0, 10, 10] stands 2 px from it; a point inside stands 0 from it.
function distance(point, box) {
    const dx = Math.max(box[0] - point[0], 0, point[0] - box[2]);
    const dy = Math.max(box[1] - point[1], 0, point[1] - box[3]);
    return Math.hypot(dx, dy);
}
// The node whose box is nearest a point.
function nearest(point, graphNodes) {
    let best = null;
    for (const n of graphNodes) {
        if (n.box && (best === null || distance(point, n.box) < distance(point, best.box)))
            best = n;
    }
    return best;
}
// A marker's kind from its drawing: a filled closed path is an arrow head, a circle a circle, two crossing strokes a cross, an open stroke an open arrow.
// Sample input: <path class="pele-marker" d="M25,94L21.64,86L28.36,86Z" fill="#565f89"/> reads "arrow"; <path class="pele-marker" d="M1,1L5,5M5,1L1,5"/> reads "cross".
function markerKind(el) {
    if (el.name === "circle")
        return "circle";
    const d = el.attrs.d ?? "";
    if (/Z\s*$/i.test(d))
        return "arrow";
    return (d.match(/M/gi) ?? []).length === 2 ? "cross" : "open";
}
// The drawn edges of a graph, each with its ends read from where its line meets a node, its marks, stroke and label.
// Sample input: edge L_A_B_0 with a dotted line through [[25, 46], [25, 87]] reads A to B with an arrow at the end.
export function edges(tree, graphNodes = nodes(tree)) {
    const labels = new Map(find(tree, "pele-edge-label").map((l) => [l.attrs["data-id"], texts(l).map((t) => t.string).join(" ")]));
    return find(tree, "pele-edge").filter((e) => !has(e, "pele-message")).map((e) => {
        const line = e.children.find((c) => c.name === "path" && !has(c, "pele-marker"));
        const points = line ? vertices(line) : [];
        const marks = e.children.filter((c) => has(c, "pele-marker")).map((m) => ({ kind: markerKind(m), box: shapeBox(m) }));
        const first = points[0];
        const last = points[points.length - 1];
        const near = (box, p) => distance(p, box);
        const atStart = marks.filter((m) => first && last && near(m.box, first) < near(m.box, last));
        const atEnd = marks.filter((m) => !atStart.includes(m));
        const from = first ? nearest(first, graphNodes) : null;
        const to = last ? nearest(last, graphNodes) : null;
        return { id: e.attrs["data-id"], from: from ? from.id : "", to: to ? to.id : "", points,
            start: atStart.length ? atStart[0].kind : "none", end: atEnd.length ? atEnd[0].kind : "none",
            width: line && line.attrs["stroke-width"] !== undefined ? Number(line.attrs["stroke-width"]) : 1,
            dashed: line !== undefined && line.attrs["stroke-dasharray"] !== undefined, label: labels.get(e.attrs["data-id"]) ?? "" };
    });
}
// The edge labels of a graph with the box of their pill, if they draw one, and their texts.
export function edgeLabels(tree) {
    return find(tree, "pele-edge-label").map((l) => ({ id: l.attrs["data-id"], box: bounds(l), texts: texts(l) }));
}
// The lifelines of a sequence diagram.
// Sample input: <path class="pele-lifeline" data-id="A" d="M0,59V347"/> reads { id: "A", x: 0, top: 59, end: 347 }.
export function lifelines(tree) {
    return find(tree, "pele-lifeline").map((l) => {
        const p = vertices(l);
        return { id: l.attrs["data-id"], x: p[0][0], top: p[0][1], end: p[p.length - 1][1] };
    });
}
// The participant boxes of a sequence diagram, top ones and footers alike.
export function actors(tree) {
    return find(tree, "pele-actor").map((a) => ({ id: a.attrs["data-id"], footer: has(a, "pele-actor-footer"), figure: has(a, "pele-actor-actor"), box: bounds(a), texts: texts(a) }));
}
// The messages of a sequence diagram with their line's vertices and label.
export function messages(tree) {
    const labels = new Map(find(tree, "pele-edge-label").map((l) => [l.attrs["data-id"], texts(l)[0]]));
    return find(tree, "pele-message").map((m) => {
        const line = m.children.find((c) => c.name === "path" && !has(c, "pele-marker"));
        return { id: m.attrs["data-id"], points: line ? vertices(line) : [], label: labels.get(m.attrs["data-id"]) ?? null };
    });
}
// The notes of a sequence diagram with their box and texts.
export function notesOf(tree) {
    return find(tree, "pele-note").map((n) => ({ id: n.attrs["data-id"], box: bounds(n), texts: texts(n) }));
}
// The viewBox [x, y, width, height] of a figure.
// Sample input: <svg width="300" height="200" viewBox="0 0 300 200"> reads [0, 0, 300, 200].
export function view(svg) {
    return svg.match(/<svg\b[^>]*>/)[0].match(/viewBox="([^"]*)"/)[1].split(/\s+/).map(Number);
}
// The subgraph frames of a flowchart by id, each with its outline and title texts.
// Sample input: <g class="pele-cluster" data-id="one"><rect x="1" y="1" width="92" height="97"/><text class="pele-cluster-label">one</text></g> reads one at [1, 1, 93, 98].
export function frames(tree) {
    return new Map(find(tree, "pele-cluster").map((c) => [c.attrs["data-id"], { box: bounds(c), texts: texts(c) }]));
}
export function frame(tree, id) {
    const found = frames(tree).get(id);
    return found ? found.box : null;
}
