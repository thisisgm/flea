// No Mermaid label overflows: each label, measured as characters times the font's advance times its size, fits its shape and its lifelines.
import { ARROW_LABEL_PAD, FACES, actors, all, edgeLabels, extent, frames, labelWidth, layered, lifelines, load, messages, nodes, notes, notesOf, others, parse, sequences, shapeBox, texts, theme, view } from "./mermaid-corpus.mjs";
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
// The library's padding beside a label: inside a box node, a state, an entity header, an actor box and a note, round an edge label's pill,
// and between the texts of a class or entity row; in px.
const BOX_PADS = { "pele-shape-rect": 18, "pele-state": 16, "pele-entity": 18 };
const ACTOR_PAD = 16;
const NOTE_PAD = 10;
const PILL_PAD = 4;
const ROW_PAD = 4;
// A tolerance for the float arithmetic of the layout's coordinates, in px.
const EPSILON = 0.01;
// The half stroke a drawn edge may reach past the canvas, in px.
const STROKE_PAD = 1;
const SELF_LABEL_GAP = 8;

function width(rect) { return rect[2] - rect[0]; }
// Sample input: a 40 by 10 label in a diamond 100 wide and 60 high reaches 20 / 50 + 5 / 30 of its half diagonals, so it fits.
function insideDiamond(rect, w, h) {
    return (w / 2) / (width(rect) / 2) + (h / 2) / ((rect[3] - rect[1]) / 2) <= 1 + EPSILON;
}
// Sample input: a 30 by 10 label in a circle of radius 20 reaches a corner 15.8 from the centre, so it fits.
function insideCircle(rect, w, h) {
    return Math.hypot(w / 2, h / 2) <= width(rect) / 2 + EPSILON;
}

for (const face of FACES) {
    const tag = ` on ${face.name}`;
    for (const [name, source] of [...layered, ...sequences, notes, ...others]) {
        const svg = render(source, face);
        const tree = parse(svg);
        const canvas = view(svg);
        // Each diagram draws its labels and its shapes, so a figure that draws nothing cannot pass the checks below by having nothing to check.
        check(texts(tree).length > 0, `${name}${tag}: the figure draws its labels`);
        check(nodes(tree).length + actors(tree).length > 0, `${name}${tag}: the figure draws its nodes or participants`);
        for (const t of texts(tree)) {
            const [left, right] = extent(t, face);
            check(left >= canvas[0] - EPSILON && right <= canvas[0] + canvas[2] + EPSILON, `${name}${tag}: "${t.string}" stays inside the canvas, ${left.toFixed(1)} to ${right.toFixed(1)} of ${canvas[0]} to ${canvas[0] + canvas[2]}`);
        }
        // A box or a pill is drawn whole: the canvas holds every rect, less its stroke.
        for (const el of all(tree).filter((c) => c.name === "rect")) {
            const r = shapeBox(el);
            check(r[0] >= canvas[0] - STROKE_PAD && r[1] >= canvas[1] - STROKE_PAD && r[2] <= canvas[0] + canvas[2] + STROKE_PAD && r[3] <= canvas[1] + canvas[3] + STROKE_PAD,
                `${name}${tag}: the rect ${r.map((v) => v.toFixed(1))} stays inside the canvas ${canvas}`);
        }
        for (const node of nodes(tree)) {
            const kind = Object.keys(BOX_PADS).find((k) => node.el.attrs.class.split(" ").includes(k));
            const label = node.texts[0];
            if (kind === undefined || label === undefined)
                continue;
            check(labelWidth(label.string, label.size, face, label.bold) + 2 * BOX_PADS[kind] <= width(node.box) + EPSILON, `${name}${tag}: node "${label.string}" fits its shape ${width(node.box).toFixed(1)} wide`);
        }
        for (const node of nodes(tree).filter((n) => n.shape === "diam" || n.shape === "circle")) {
            for (const t of node.texts) {
                const w = labelWidth(t.string, t.size, face, t.bold);
                const fits = node.shape === "diam" ? insideDiamond(node.box, w, t.size) : insideCircle(node.box, w, t.size);
                check(fits, `${name}${tag}: ${node.shape} node "${t.string}" fits inside its outline ${node.box.map((v) => v.toFixed(1))}`);
            }
        }
        if (!/^sequenceDiagram/.test(source)) {
            for (const label of edgeLabels(tree)) {
                check(label.box !== null, `${name}${tag}: edge label "${label.texts.map((t) => t.string).join(" ")}" draws its pill`);
                for (const t of label.box ? label.texts : [])
                    check(labelWidth(t.string, t.size, face, t.bold) + 2 * PILL_PAD <= width(label.box) + EPSILON, `${name}${tag}: edge label "${t.string}" fits its pill ${width(label.box).toFixed(1)} wide`);
            }
        }
        for (const [id, group] of frames(tree)) {
            for (const t of group.texts)
                check(extent(t, face)[0] >= group.box[0] - EPSILON && extent(t, face)[1] <= group.box[2] + EPSILON, `${name}${tag}: subgraph ${id} title "${t.string}" fits its frame`);
        }
        for (const actor of actors(tree).filter((a) => !a.figure)) {
            for (const t of actor.texts)
                check(labelWidth(t.string, t.size, face, t.bold) + 2 * ACTOR_PAD <= width(actor.box) + EPSILON, `${name}${tag}: participant "${t.string}" fits its box ${width(actor.box)} wide`);
        }
        for (const note of notesOf(tree)) {
            for (const t of note.texts)
                check(labelWidth(t.string, t.size, face, t.bold) + 2 * NOTE_PAD <= width(note.box) + EPSILON, `${name}${tag}: note "${t.string}" fits its box ${width(note.box)} wide`);
        }
        const lifelineXs = lifelines(tree).map((l) => l.x).sort((a, b) => a - b);
        for (const m of messages(tree)) {
            const label = m.label;
            if (!label || m.points.length === 0)
                continue;
            const w = labelWidth(label.string, label.size, face, label.bold);
            const level = m.points.every((p) => Math.abs(p[1] - m.points[0][1]) < EPSILON);
            if (!level) {
                // A self message loops right of its lifeline, and its label starts a gap beyond the loop.
                const loopX = lifelineXs.find((x) => Math.abs(x - m.points[0][0]) < EPSILON);
                const loopRight = Math.max(...m.points.map((p) => p[0]));
                const next = lifelineXs.find((x) => x > loopX + EPSILON);
                check(loopX !== undefined && Math.abs(extent(label, face)[0] - (loopRight + SELF_LABEL_GAP)) < EPSILON, `${name}${tag}: self message "${label.string}" starts beside its loop`);
                check(next === undefined || extent(label, face)[1] + ARROW_LABEL_PAD <= next + EPSILON, `${name}${tag}: self message "${label.string}" ends before the next lifeline ${next}`);
                continue;
            }
            // The line leaves one lifeline and stops at its arrow head short of the other, so each end reads the lifeline nearest it.
            const lifelineAt = (x) => lifelineXs.reduce((best, l) => Math.abs(l - x) < Math.abs(best - x) ? l : best);
            const apart = Math.abs(lifelineAt(m.points[m.points.length - 1][0]) - lifelineAt(m.points[0][0]));
            check(w + 2 * ARROW_LABEL_PAD <= apart + EPSILON, `${name}${tag}: message "${label.string}" (${w.toFixed(1)}) fits between its lifelines ${apart} apart`);
        }
        // A class box or an entity holds each of its rows between its own edges, and a row's texts do not overlap.
        for (const kind of ["pele-class", "pele-entity"]) {
            for (const box of nodes(tree).filter((n) => n.el.attrs.class.split(" ").includes(kind))) {
                const rows = new Map();
                for (const t of box.texts) {
                    const [left, right] = extent(t, face);
                    check(left >= box.box[0] - EPSILON && right <= box.box[2] + EPSILON, `${name}${tag}: ${kind} "${t.string}" stays inside its box ${box.box[0].toFixed(1)} to ${box.box[2].toFixed(1)}`);
                    rows.set(t.y, [...(rows.get(t.y) ?? []), [left, right, t.string]]);
                    // The size table is the theme font's, so a row drawn in any other face would be sized with the wrong table.
                    check(t.family === theme.font, `${name}${tag}: ${kind} row text is drawn in the theme font ${theme.font}, got ${t.family}`);
                }
                for (const [, row] of rows) {
                    row.sort((a, b) => a[0] - b[0]);
                    for (let i = 1; i < row.length; i++)
                        check(row[i - 1][1] + ROW_PAD <= row[i][0] + EPSILON, `${name}${tag}: "${row[i - 1][2]}" and "${row[i][2]}" do not overlap in their row`);
                }
            }
        }
    }
}
console.log(`mermaid-fit: ${checks} check(s), ${failures} failed`);
finish(failures);
