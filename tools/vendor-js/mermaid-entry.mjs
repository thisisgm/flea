import { detectType, render } from "pele";

// A face is a per-character advance table for printable ASCII, 32 to 126, in thousandths of an em.
const TABLE_FIRST = 32;
const TABLE_LENGTH = 95;
const THOUSAND = 1000;
const STYLE_BOLD = 1;
// A share of one colour mixed into another, for the chart series derived from the theme's roles.
const HALF = 0.5;
const MOSTLY = 0.7;
const HEX_RADIX = 16;
// CSS color-mix writes each share as a percentage.
const PERCENT = 100;
// A Gantt today marker draws at this hour UTC of the day the request names, which the helper formats back to that date.
const MIDDAY_HOUR = 12;
// The space Pele lays round the drawing, in px, so a border stroke on the canvas edge is not cut.
const PADDING = 1;
const FALLBACK_BODY_PX = 14;
// v0.3.12 laid every diagram out at body 13 and scaled the whole canvas to the theme's body size, so boxes, gaps and strokes keep their proportion.
const LAYOUT_PX = 13;
// v0.3.12's participant, 80 by 40 and 60 apart; Pele steps a message by its margin less 35 plus the label's 33, so 42 keeps v0.3.12's 40.
const SEQUENCE = { width: 80, height: 40, actorMargin: 60, messageMargin: 42 };
// Pele draws a first message 46 below its participants and closes 20 under the last; v0.3.12 drew 20, closed 40 and set a label 2.5 lower.
const FIRST_ROW_RISE = 26;
const FOOTER_RISE = 6;
const LABEL_DROP = 2.5;
// v0.3.12's flowchart box: square corners, at least 60 wide, 41.95 wider than its label, 16.9 a line plus 20 high, under a 0.75 stroke.
const BOX = { minWidth: 60, padX: 41.95, line: 16.9, padY: 20, stroke: "0.75" };
// Pele 1.6.0's box is at least 48 wide and 36 wider than its label.
const PELE_BOX = { minWidth: 48, padX: 36 };
// Two coordinates closer than this are the same point; a marker this near a path's end belongs to that end.
const EPSILON = 0.6;
const MARKER_REACH = 16;
const PLACES = 100;
const LIFELINES = '<g class="pele-lifelines" stroke-width="0.75" stroke-dasharray="6 4"';

// Sample input: "--- \ntitle: A\n---\nsequenceDiagram" answers 18, through the closing line's break; a source without front matter answers 0.
// Pele 1.6.0's own front matter scan, line for line, so the defaults land exactly where Pele's front matter ends.
function frontMatterLength(e) {
    const blank = (c) => c !== undefined && /\s/.test(c);
    const isBreak = (c) => c === "\n" || c === "\r";
    let t = 0;
    while (t < e.length && blank(e[t]) && !isBreak(e[t]))
        t++;
    const fence = e.slice(0, t) + "---";
    if (!e.startsWith("---", t))
        return 0;
    const opens = [];
    for (let u = t + 3; u < e.length && blank(e[u]); u++) {
        if (isBreak(e[u]))
            opens.push(u);
    }
    if (opens.length === 0)
        return 0;
    const closeEnd = (u) => {
        if (!isBreak(e[u]) || !e.startsWith(fence, u + 1))
            return -1;
        let end = -1;
        for (let g = u + 1 + fence.length; g < e.length && blank(e[g]); g++) {
            if (isBreak(e[g]))
                end = g;
        }
        return end;
    };
    const marker = "\n" + fence;
    let last = -1;
    for (let u = e.lastIndexOf(marker); u > opens[0]; u = e.lastIndexOf(marker, u - 1)) {
        if (closeEnd(u) !== -1) {
            last = u;
            break;
        }
    }
    if (last === -1)
        return 0;
    let open = opens[0];
    for (const u of opens) {
        if (u < last)
            open = u;
    }
    const start = open + 1;
    for (let u = e.indexOf(marker, start > 0 ? start - 1 : 0); u !== -1 && u <= last; u = e.indexOf(marker, u + 1)) {
        if (u < start)
            continue;
        const end = closeEnd(u);
        if (end !== -1)
            return end + 1;
    }
    return 0;
}

// Sample input: '"sequence": {"x": {"y": 1}, "width": 120}}' after config's opening brace adds "x" and "width"; each key lives until its own object closes.
function flowKeys(text, base, found) {
    const keys = [];
    let depth = 1;
    let word = "";
    for (let i = 0; i < text.length; i++) {
        const c = text[i];
        if (c === '"' || c === "'") {
            const end = text.indexOf(c, i + 1);
            word = text.slice(i + 1, end < 0 ? text.length : end);
            i = end < 0 ? text.length : end;
            continue;
        }
        if (c === ":") {
            while (keys.length > 0 && keys[keys.length - 1].depth >= depth)
                keys.pop();
            keys.push({ key: word.trim(), depth });
            const path = base.concat(keys.map((k) => k.key));
            if (path.length === 3 && path[0] === "config" && path[1] === "sequence")
                found.add(path[2]);
        } else if (c === "{") {
            depth++;
        } else if (c === "}") {
            depth--;
            while (keys.length > 0 && keys[keys.length - 1].depth > depth)
                keys.pop();
        }
        word = c === ":" || c === "{" || c === "}" || c === "," ? "" : word + c;
    }
}

// Sample input: "---\ntitle: Login sequence: happy\nconfig:\n  sequence:\n    width: 120\n  flowchart:\n    width: 200\n---\n" answers {"width"}.
// Front matter is YAML: a block key's parents are the keys above it indented less, and a flow value is read by its braces.
function frontSequenceKeys(front) {
    const found = new Set();
    const parents = [];
    const lines = front.split(/\r?\n/);
    for (let n = 1; n < lines.length; n++) {
        const line = lines[n];
        const key = line.trim().match(/^["']?([\w-]+)["']?\s*:(.*)$/);
        if (line.trim() === "---" || !key)
            continue;
        const indent = line.length - line.trimStart().length;
        while (parents.length > 0 && parents[parents.length - 1].indent >= indent)
            parents.pop();
        const path = parents.map((p) => p.key).concat(key[1]);
        const value = key[2].trim();
        if (path.length === 3 && path[0] === "config" && path[1] === "sequence")
            found.add(path[2]);
        if (value.startsWith("{")) {
            let text = value;
            while (n + 1 < lines.length && (text.split("{").length > text.split("}").length) && lines[n + 1].trim() !== "---")
                text += "\n" + lines[++n];
            flowKeys(text.slice(1), path, found);
        } else if (value === "" || value.startsWith("#")) {
            parents.push({ indent, key: key[1] });
        }
    }
    return found;
}

// Sample input: "---\nconfig:\n  sequence:\n    width: 120\n---\nsequenceDiagram" keeps its 120 and gains the default height and gap.
// Pele merges directives key by key but its render config whole, so the defaults go in as the first directive, leaving out what front matter names.
function withSequenceDefaults(source) {
    if (detectType(source) !== "sequence")
        return source;
    const front = source.slice(0, frontMatterLength(source));
    const named = frontSequenceKeys(front);
    const defaults = {};
    for (const [key, value] of Object.entries(SEQUENCE)) {
        if (!named.has(key))
            defaults[key] = value;
    }
    return front + "%%{init: " + JSON.stringify({ sequence: defaults }) + "}%%\n" + source.slice(front.length);
}

// A pass that meets a drawing it cannot place exactly throws this and hands Pele's own drawing on unchanged.
const UNPLACED = new Error("unplaced");
const rounded = (v) => String(Math.round(v * PLACES) / PLACES);

// Sample input: "M25,45L25,86H30Z" reads [["M",[25,45]],["L",[25,86]],["H",[30]],["Z",[]]]; a relative command throws UNPLACED.
function parsePath(d) {
    const segments = [];
    for (const m of d.matchAll(/([A-Za-z])([^A-Za-z]*)/g)) {
        if (!"MLCQSTAHVZ".includes(m[1]))
            throw UNPLACED;
        segments.push([m[1], (m[2].match(/-?(?:\d+\.?\d*|\.\d+)/g) || []).map(Number)]);
    }
    return segments;
}

// The [x, y] slots of one command's numbers, -1 where the command carries no such axis.
function slots(command, count) {
    const out = [];
    if (command === "H" || command === "V") {
        for (let i = 0; i < count; i++)
            out.push(command === "H" ? [i, -1] : [-1, i]);
    } else if (command === "A") {
        for (let i = 0; i + 6 < count; i += 7)
            out.push([i + 5, i + 6]);
    } else {
        for (let i = 0; i + 1 < count; i += 2)
            out.push([i, i + 1]);
    }
    return out;
}

// Sample input: ("M0,40V99", f) hands f [0, 40] then [NaN, 99] with their place and count, and writes back what f answers.
function mapPath(d, f) {
    const segments = parsePath(d);
    const all = [];
    for (const [command, numbers] of segments) {
        for (const slot of slots(command, numbers.length))
            all.push([numbers, slot]);
    }
    all.forEach(([numbers, [xi, yi]], at) => {
        const p = f([xi < 0 ? NaN : numbers[xi], yi < 0 ? NaN : numbers[yi]], at, all.length);
        if (xi >= 0)
            numbers[xi] = p[0];
        if (yi >= 0)
            numbers[yi] = p[1];
    });
    return segments.map(([command, numbers]) => command + numbers.map(rounded).join(",")).join("");
}

// Sample input: ('<rect x="-24" width="48"/>', "width") answers 48; a missing attribute answers NaN.
function attr(tag, name) {
    const m = tag.match(new RegExp(`\\s${name}="([^"]*)"`));
    return m ? Number(m[1]) : NaN;
}

function setAttr(tag, name, value) {
    return tag.replace(new RegExp(`(\\s${name}=")[^"]*"`), (all, head) => head + (typeof value === "string" ? value : rounded(value)) + '"');
}

// Sample input: ('<rect x="1" y="2"/>', p => [p[0], p[1] + 5]) reads '<rect x="1" y="7"/>'; d and points move point by point.
function mapTag(tag, f) {
    let out = tag;
    for (const [a, b] of [["x", "y"], ["x1", "y1"], ["x2", "y2"], ["cx", "cy"]]) {
        const p = [attr(tag, a), attr(tag, b)];
        if (Number.isNaN(p[0]) && Number.isNaN(p[1]))
            continue;
        const q = f(p);
        out = Number.isNaN(p[0]) ? out : setAttr(out, a, q[0]);
        out = Number.isNaN(p[1]) ? out : setAttr(out, b, q[1]);
    }
    const d = tag.match(/\sd="([^"]*)"/);
    if (d)
        out = setAttr(out, "d", mapPath(d[1], f));
    const points = tag.match(/\spoints="([^"]*)"/);
    if (points)
        out = setAttr(out, "points", mapPath("M" + points[1].trim().replace(/\s+/g, ","), f).slice(1).replace(/([^,]*,[^,]*),/g, "$1 "));
    return out;
}

// Sample input: a path's d answers its points in order, [NaN, y] for a V; a circle answers its centre, any other tag its x and y.
function pointsOf(tag) {
    const found = [];
    mapTag(tag, (p) => {
        found.push(p);
        return p;
    });
    return found;
}

// Sample input: (svg, index of '<g class="a">') answers the index just past that group's own '</g>', counting the groups nested in it.
function groupEnd(svg, open) {
    let depth = 0;
    const re = /<g\b[^>]*>|<\/g>/g;
    re.lastIndex = open;
    for (let m = re.exec(svg); m !== null; m = re.exec(svg)) {
        depth += m[0] === "</g>" ? -1 : 1;
        if (depth === 0)
            return re.lastIndex;
    }
    throw UNPLACED;
}

// Sample input: ('<svg viewBox="0 0 50 138" width="50" height="138">', 1, 0, 120) reads viewBox="0 0 50 120" with height 120.
function setCanvas(svg, axis, from, to) {
    return svg.replace(/^<svg\b[^>]*>/, (root) => {
        const box = (root.match(/\sviewBox="([^"]*)"/) || [0, "0 0 0 0"])[1].split(/[\s,]+/).map(Number);
        box[axis] = from;
        box[axis + 2] = to - from;
        return setAttr(setAttr(root, "viewBox", box.map(rounded).join(" ")), axis === 0 ? "width" : "height", box[axis + 2]);
    });
}

// Sample input: '%%{init: {"sequence": {"messageMargin": 35}}}%%' or front matter naming config.sequence.messageMargin sets one; a message reading "messageMargin" does not.
function ownMargin(source) {
    const front = source.slice(0, frontMatterLength(source));
    return frontSequenceKeys(front).has("messageMargin") || [...source.matchAll(/%%\{[^]*?\}%%/g)].some((m) => m[0].includes("messageMargin"));
}

// Only a straight first message moves: a note, frame, rect or activation above it, a created or destroyed participant, no closing row or a source's own messageMargin keeps Pele's rows.
function oldMessageRows(svg, source) {
    try {
        return messageRows(svg, source);
    } catch (error) {
        if (error === UNPLACED)
            return svg;
        throw error;
    }
}

// Sample input: Pele's '<path class="pele-lifeline" d="M0,46V112"/>' and '<g class="pele-edge pele-message"><path d="M0,92H140"/>' rise to a message 20 below.
function messageRows(svg, source) {
    if (ownMargin(source) || svg.includes('class="pele-destroys"') || !svg.includes("pele-actor-footer"))
        return svg;
    const lifelines = [...svg.matchAll(/<path class="pele-lifeline"[^>]*\sd="M[-\d.]+,([-\d.]+)V([-\d.]+)"/g)].map((m) => Number(m[1]));
    const messages = [...svg.matchAll(/<g class="pele-edge pele-message"[^>]*><path d="M[-\d.]+,([-\d.]+)([^"]*)"/g)].map((m) => [Number(m[1]), m[2]]);
    if (lifelines.length === 0 || lifelines.some((y) => y !== lifelines[0]) || messages.length === 0)
        return svg;
    const first = messages.reduce((a, b) => (b[0] < a[0] ? b : a));
    if (!/^H[-\d.]+$/.test(first[1]))
        return svg;
    for (const name of ["rects", "activations", "frames", "notes"]) {
        const open = svg.indexOf(`<g class="pele-${name}"`);
        if (open >= 0 && [...svg.slice(open, groupEnd(svg, open)).matchAll(/\sy="([-\d.]+)"/g)].some((m) => Number(m[1]) < first[0]))
            return svg;
    }
    const rise = (by) => (tag) => tag.replace(/>$/, ` transform="translate(0,${rounded(-by)})">`);
    let out = svg.replace(/<g class="pele-(?:rects|activations|frames|edges|notes)"[^>]*>/g, rise(FIRST_ROW_RISE))
        .replace(/<g class="pele-edge-labels"[^>]*>/, rise(FIRST_ROW_RISE - LABEL_DROP))
        .replace(/<g class="pele-node pele-actor[^"]*pele-actor-footer[^"]*"[^>]*>/g, rise(FOOTER_RISE))
        .replace(/(<path class="pele-lifeline"[^>]*\sd="M[-\d.]+,[-\d.]+V)([-\d.]+)/g, (all, head, end) => head + rounded(Number(end) - FOOTER_RISE))
        .replace(/(<g class="pele-cluster pele-box"[^>]*><rect\b[^>]*\sheight=")([-\d.]+)/g, (all, head, h) => head + rounded(Number(h) - FOOTER_RISE));
    const box = out.match(/^<svg\b[^>]*\sviewBox="([^"]*)"/)[1].split(/[\s,]+/).map(Number);
    return setCanvas(out, 1, box[1], box[1] + box[3] - FOOTER_RISE);
}

// Sample input: '<circle r="26.53"/><text>Done</text>' answers [26.53, 26.53], how far a node's own shapes reach from its centre, its text left out.
function halfOf(body) {
    const half = [0, 0];
    const reach = (p) => p.forEach((v, i) => {
        half[i] = Number.isNaN(v) ? half[i] : Math.max(half[i], Math.abs(v));
    });
    for (const tag of body.match(/<(?!text|tspan)[a-z]+\b[^>]*>/g) || []) {
        const name = tag.match(/^<([a-z]+)/)[1];
        const [x, y, w, h, cx, cy] = ["x", "y", "width", "height", "cx", "cy"].map((key) => attr(tag, key) || 0);
        const [rx, ry] = name === "circle" ? [attr(tag, "r"), attr(tag, "r")] : [attr(tag, "rx"), attr(tag, "ry")];
        if (name === "circle" || name === "ellipse")
            [[cx - rx, cy - ry], [cx + rx, cy + ry]].forEach(reach);
        else if (w > 0 || h > 0)
            [[x, y], [x + w, y + h]].forEach(reach);
        else
            pointsOf(tag).forEach(reach);
    }
    return half;
}

// Sample input: '<g class="pele-node pele-shape-rect" transform="translate(25,23)"><rect x="-24" y="-22" width="48" height="44"/><text class="pele-label" x="0" y="4.55">A</text></g>'.
function nodesOf(svg) {
    const nodes = [];
    for (const m of svg.matchAll(/<g class="pele-node pele-shape-([\w-]+)[^"]*"[^>]*\stransform="translate\(([-\d.]+),([-\d.]+)\)"[^>]*>/g)) {
        const end = groupEnd(svg, m.index);
        const body = svg.slice(m.index + m[0].length, end - "</g>".length);
        const node = { open: m[0], at: m.index, end, body, c: [Number(m[2]), Number(m[3])], box: null, half: halfOf(body) };
        const rect = body.match(/^<rect\b[^>]*>/);
        // One label line is the text itself; Pele sets each line of a longer label as a tspan carrying its own y.
        const label = rect && /^<text class="pele-label"[^>]*>(?:[^<]*|(?:<tspan\b[^>]*>[^<]*<\/tspan>)+)<\/text>$/.test(body.slice(rect[0].length)) ? body.slice(rect[0].length) : "";
        const lines = label.match(/<tspan\b[^>]*\sy="[^"]*"[^>]*>/g) || label.match(/^<text\b[^>]*\sy="[^"]*"[^>]*>/g) || [];
        if (m[1] === "rect" && label && lines.length > 0) {
            const [x, y, w, h] = ["x", "y", "width", "height"].map((name) => attr(rect[0], name));
            if (Math.abs(x + w / 2) < EPSILON && Math.abs(y + h / 2) < EPSILON) {
                const count = lines.length;
                const width = w > PELE_BOX.minWidth + EPSILON ? Math.max(BOX.minWidth, w - PELE_BOX.padX + BOX.padX) : BOX.minWidth;
                node.box = { rect: rect[0], label, lines, half: [w / 2, h / 2], next: [width / 2, (count * BOX.line + BOX.padY) / 2] };
            }
        }
        node.next = node.box ? node.box.next : node.half;
        node.half = node.box ? node.box.half : node.half;
        nodes.push(node);
    }
    return nodes;
}

// Sample input: two ranks of 44 high boxes 92 apart answer a remap that shrinks each to 36.9 and keeps 48 between them; a rank whose other shapes reach further keeps its size.
function rankRemap(nodes, axis) {
    const ranks = new Map();
    for (const node of nodes) {
        const key = rounded(node.c[axis]);
        const rank = ranks.get(key) || { c: node.c[axis], e: 0, next: 0 };
        rank.e = Math.max(rank.e, node.half[axis]);
        rank.next = Math.max(rank.next, node.next[axis]);
        ranks.set(key, rank);
    }
    // Ranks that overlap along the flow, as side by side subgraphs laid out apart do, keep their spacing and only move together.
    const bands = [];
    for (const r of [...ranks.values()].sort((a, b) => a.c - b.c)) {
        const last = bands[bands.length - 1];
        if (last && last.c + last.e > r.c - r.e + EPSILON) {
            const [low, high] = [Math.min(last.c - last.e, r.c - r.e), Math.max(last.c + last.e, r.c + r.e)];
            bands[bands.length - 1] = { c: (low + high) / 2, e: (high - low) / 2, next: (high - low) / 2 };
        } else {
            bands.push(r);
        }
    }
    const changed = bands.filter((r) => r.e > 0 && Math.abs(r.e - r.next) > 0);
    return (v) => {
        let shift = 0;
        for (const r of changed) {
            const d = r.e - r.next;
            if (v < r.c - r.e)
                break;
            if (v <= r.c + r.e)
                return r.c - shift - d + (v - r.c) * r.next / r.e;
            shift += 2 * d;
        }
        return v - shift;
    };
}

// Sample input: Pele's "flowchart TD\n    A --> B" with round 48 by 44 boxes reads v0.3.12's square 60 by 36.9, every edge end, marker, label and group moved with its rank.
function oldFlowchartBoxes(svg, source) {
    const axis = /^\s*(?:flowchart|graph)(?:-elk)?\s+(?:LR|RL)\b/m.test(source) ? 0 : 1;
    // A group is known by its first class, which Pele writes before any class the source gives it, and every pass below matches that word alone.
    const groups = [...svg.matchAll(/<g class="([\w-]+)/g)].map((m) => m[1]);
    const known = ["pele-clusters", "pele-cluster", "pele-cluster-titles", "pele-cluster-title", "pele-edges", "pele-edge", "pele-edge-labels", "pele-edge-label", "pele-nodes", "pele-node"];
    if (groups.some((name) => !known.includes(name)))
        return svg;
    try {
        const nodes = nodesOf(svg);
        if (!nodes.some((node) => node.box))
            return svg;
        const remap = rankRemap(nodes, axis);
        const along = (p) => p.map((v, i) => (i === axis && !Number.isNaN(v) ? remap(v) : v));
        const centre = (node) => along(node.c);
        // A point on a box's side stays on that side at the same offset along it, held inside the box; a point on another shape moves with its centre.
        const onNode = (node, p) => {
            const c = centre(node);
            return p.map((v, i) => {
                const r = v - node.c[i];
                const [half, next] = [node.half[i], node.next[i]];
                return c[i] + (!node.box ? r : Math.abs(Math.abs(r) - half) < EPSILON ? Math.sign(r) * next : Math.max(-next, Math.min(next, r)));
            });
        };
        const inside = (node, p) => [0, 1].every((i) => Math.abs(p[i] - node.c[i]) <= node.half[i] + EPSILON);
        const owner = (p) => nodes.find((node) => node.box && inside(node, p) && [0, 1].some((i) => Math.abs(Math.abs(p[i] - node.c[i]) - node.half[i]) < EPSILON))
            || nodes.find((node) => !node.box && inside(node, p));
        const edits = [];
        for (const node of nodes) {
            const c = centre(node);
            let body = node.body;
            if (node.box) {
                const [w, h] = node.box.next.map((v) => 2 * v);
                let rect = setAttr(setAttr(setAttr(setAttr(node.box.rect, "x", -w / 2), "y", -h / 2), "width", w), "height", h);
                // Pele writes pele-radius as the style's first declaration, rx:4px; v0.3.12's box is square, and a source's own rx or ry after it wins.
                rect = rect.replace(/(\sstyle=")rx:[^;"]*;?/, "$1").replace(/\sstyle=""/, "").replace(/\sr[xy]="[^"]*"/g, "");
                rect = /\sstroke-width="/.test(rect) ? rect : rect.replace(/\s*(\/?)>$/, ` stroke-width="${BOX.stroke}"$1>`);
                const ys = node.box.lines.map((line) => attr(line, "y"));
                const mid = (ys[0] + ys[ys.length - 1]) / 2;
                const pitch = ys.length > 1 ? (ys[ys.length - 1] - ys[0]) / (ys.length - 1) : 1;
                body = rect + node.box.lines.reduce((text, line, i) => text.replace(line, setAttr(line, "y", mid + (ys[i] - mid) * BOX.line / pitch)), node.box.label);
            }
            edits.push([node.at, node.end, node.open.replace(/translate\([^)]*\)/, `translate(${rounded(c[0])},${rounded(c[1])})`) + body + "</g>"]);
        }
        for (const m of svg.matchAll(/<g class="pele-edge(?:\s[^"]*)?"[^>]*>/g)) {
            const end = groupEnd(svg, m.index);
            const tags = svg.slice(m.index + m[0].length, end - "</g>".length).match(/<[^/][^>]*>|<\/[^>]*>/g) || [];
            const main = tags.findIndex((tag) => tag.startsWith("<path") && !tag.includes("pele-marker"));
            if (main < 0)
                throw UNPLACED;
            const line = pointsOf(tags[main]);
            const ends = [line[0], line[line.length - 1]];
            const near = (tag) => {
                const reach = ends.map((e) => Math.min(...pointsOf(tag).filter((p) => !p.some(Number.isNaN)).map((p) => Math.hypot(p[0] - e[0], p[1] - e[1]))));
                return reach[0] < MARKER_REACH || reach[1] < MARKER_REACH ? (reach[0] <= reach[1] ? 0 : 1) : -1;
            };
            const marks = tags.map((tag, i) => (i !== main && tag.includes("pele-marker") ? near(tag) : -1));
            const shift = ends.map((e, side) => {
                const candidates = [e].concat(...tags.filter((t, i) => marks[i] === side).map(pointsOf));
                for (const p of candidates) {
                    const node = Number.isNaN(p[0]) || Number.isNaN(p[1]) ? undefined : owner(p);
                    if (node) {
                        const q = onNode(node, p);
                        const r = along(p);
                        return [q[0] - r[0], q[1] - r[1]];
                    }
                }
                return [0, 0];
            });
            const moved = (side) => (p) => along(p).map((v, i) => v + (side < 0 ? 0 : shift[side][i]));
            const out = tags.map((tag, i) => (tag.startsWith("</") ? tag : i === main
                ? setAttr(tag, "d", mapPath(tag.match(/\sd="([^"]*)"/)[1], (p, at, count) => moved(at === 0 ? 0 : at === count - 1 ? 1 : -1)(p)))
                : mapTag(tag, moved(marks[i]))));
            edits.push([m.index + m[0].length, end - "</g>".length, out.join("")]);
        }
        for (const m of svg.matchAll(/<g class="pele-(edge-label|cluster-title|cluster)(?:\s[^"]*)?"[^>]*>/g)) {
            const end = groupEnd(svg, m.index);
            const tags = svg.slice(m.index + m[0].length, end - "</g>".length).match(/<[^>]*>|[^<]+/g) || [];
            const rect = tags.find((tag) => tag.startsWith("<rect"));
            const size = axis === 0 ? "width" : "height";
            const from = rect ? attr(rect, axis === 0 ? "x" : "y") : NaN;
            const cluster = m[1] === "cluster";
            const text = tags.find((tag) => tag.startsWith("<text"));
            const anchor = cluster ? from : rect ? from + attr(rect, size) / 2 : text ? attr(text, axis === 0 ? "x" : "y") : NaN;
            if (Number.isNaN(anchor))
                throw UNPLACED;
            const delta = remap(anchor) - anchor;
            const out = tags.map((tag) => {
                if (!tag.startsWith("<") || tag.startsWith("</"))
                    return tag;
                if (cluster && tag === rect)
                    return setAttr(setAttr(tag, axis === 0 ? "x" : "y", remap(from)), size, remap(from + attr(tag, size)) - remap(from));
                return mapTag(tag, (p) => p.map((v, i) => (i === axis ? v + delta : v)));
            });
            edits.push([m.index + m[0].length, end - "</g>".length, out.join("")]);
        }
        edits.sort((a, b) => b[0] - a[0]);
        let out = svg;
        for (const [from, to, text] of edits)
            out = out.slice(0, from) + text + out.slice(to);
        const box = out.match(/^<svg\b[^>]*\sviewBox="([^"]*)"/)[1].split(/[\s,]+/).map(Number);
        out = setCanvas(out, axis, box[axis], remap(box[axis] + box[axis + 2]));
        // A box that grew across the flow widens the canvas to hold it, as Pele's padding held the narrower one.
        const across = 1 - axis;
        const reach = nodes.filter((node) => node.box).map((node) => [node.c[across] - node.box.next[across], node.c[across] + node.box.next[across]]);
        const low = Math.min(box[across], ...reach.map((r) => r[0] - PADDING));
        const high = Math.max(box[across] + box[across + 2], ...reach.map((r) => r[1] + PADDING));
        return setCanvas(out, across, low, high);
    } catch (error) {
        if (error === UNPLACED)
            return svg;
        throw error;
    }
}

// Sample input: a canvas 52 wide at body 14 reads width 56 over the same viewBox, the way v0.3.12 scaled a body 13 drawing, unrounded as it was.
function scaledCanvas(svg, bodyPx) {
    const share = (bodyPx > 0 ? bodyPx : FALLBACK_BODY_PX) / LAYOUT_PX;
    return svg.replace(/^<svg\b[^>]*>/, (root) => root.replace(/\s(width|height)="([\d.]+)"/g,
        (all, name, value) => ` ${name}="${Number(value) * share}"`));
}

// Sample input: 95 positive numbers answer { table, mean } in em; anything else answers null.
function faceOf(thousandths) {
    if (!Array.isArray(thousandths) || thousandths.length !== TABLE_LENGTH || !thousandths.every((v) => v > 0 && v < Infinity))
        return null;
    const table = thousandths.map((v) => v / THOUSAND);
    return { table, mean: table.reduce((sum, v) => sum + v, 0) / TABLE_LENGTH };
}

// Wide East Asian blocks and fullwidth forms take two cells, as in Pele's own table measurer.
function isWide(c) {
    return (c >= 0x1100 && c <= 0x115f) || (c >= 0x2e80 && c <= 0xa4cf) || (c >= 0xac00 && c <= 0xd7a3) || (c >= 0xf900 && c <= 0xfaff)
        || (c >= 0xfe30 && c <= 0xfe4f) || (c >= 0xff00 && c <= 0xff60) || (c >= 0xffe0 && c <= 0xffe6) || (c >= 0x20000 && c <= 0x3fffd);
}

// Combining marks, zero-width spaces and joiners, variation selectors and the byte order mark draw nothing.
function isZeroWidth(c) {
    return (c >= 0x300 && c <= 0x36f) || (c >= 0x200b && c <= 0x200f) || (c >= 0xfe00 && c <= 0xfe0f) || c === 0xfeff;
}

// Sample input: "Wi" at 14 px sums the font's own W and i advances; a CJK character counts twice the table's mean, any other non-ASCII once.
function measurerOf(theme) {
    const regular = faceOf(theme.advances);
    if (!regular)
        return undefined;
    const bold = faceOf(theme.boldAdvances) || regular;
    return {
        width(text, size, style) {
            const face = style & STYLE_BOLD ? bold : regular;
            let em = 0;
            for (const ch of text) {
                const c = ch.codePointAt(0);
                if (c < TABLE_FIRST)
                    continue;
                if (c < TABLE_FIRST + TABLE_LENGTH)
                    em += face.table[c - TABLE_FIRST];
                else if (!isZeroWidth(c))
                    em += isWide(c) ? 2 * face.mean : face.mean;
            }
            return em * size;
        }
    };
}

// Sample input: ("#ffffff", "#000000", 0.7) answers #b3b3b3; a colour that is not #rgb or #rrggbb answers the first one unchanged.
function mix(a, b, share) {
    const rgb = (h) => {
        const s = String(h).trim().replace(/^#([0-9a-f])([0-9a-f])([0-9a-f])$/i, "#$1$1$2$2$3$3");
        return /^#[0-9a-f]{6}$/i.test(s) ? [1, 3, 5].map((i) => parseInt(s.slice(i, i + 2), HEX_RADIX)) : null;
    };
    const x = rgb(a);
    const y = rgb(b);
    if (!x || !y)
        return a;
    return "#" + x.map((v, i) => Math.round(v * share + y[i] * (1 - share)).toString(HEX_RADIX).padStart(2, "0")).join("");
}

// Every Pele variable as the concrete value of the Flea theme role that draws it.
function rolesOf(theme) {
    const bg = theme.bg || "#ffffff";
    const fg = theme.fg || "#000000";
    const accent = theme.accent || fg;
    const muted = theme.muted || fg;
    const surface = theme.surface || bg;
    const font = theme.font || "sans-serif";
    return {
        "pele-bg": bg,
        "pele-fg": fg,
        "pele-muted": muted,
        // Edges and borders draw in the muted role and subgraphs on the pane ground, as the figure path drew them before.
        "pele-line": muted,
        "pele-border": muted,
        "pele-surface": surface,
        "pele-surface-alt": bg,
        "pele-accent": accent,
        "pele-critical": accent,
        "pele-error": accent,
        "pele-font": font,
        "pele-font-mono": font,
        "pele-radius": "4px",
        "pele-title-weight": "bold",
        "pele-heading-weight": "bold",
        "pele-bold-weight": "bold",
        // Eight chart series from the four colour roles: each pure, then each mixed toward another, so neighbours differ in hue or in lightness.
        "pele-series-1": accent,
        "pele-series-2": fg,
        "pele-series-3": muted,
        "pele-series-4": mix(accent, bg, HALF),
        "pele-series-5": mix(accent, fg, HALF),
        "pele-series-6": mix(fg, bg, MOSTLY),
        "pele-series-7": mix(muted, accent, HALF),
        "pele-series-8": mix(muted, bg, MOSTLY)
    };
}

// A theme alias chain 12 deep still resolves; deeper nesting refuses, so a cyclic alias fails instead of hanging the helper.
const RESOLVER_DEPTH_MAX = 12;

// Sample input: 'a="var(--_x)" b="var(--y, #fff)"' with --_x defined as var(--z) and --z as #000 reads 'a="#000" b="#fff"'.
function resolve(text, table, depth) {
    return text.replace(/var\(\s*--([\w-]+)\s*(?:,((?:[^()]|\([^()]*\))*))?\)/g, (all, name, fallback) => {
        if (depth > RESOLVER_DEPTH_MAX)
            throw new Error("diagram style nests too deep");
        if (Object.prototype.hasOwnProperty.call(table, name))
            return resolve(table[name], table, depth + 1);
        if (fallback !== undefined)
            return resolve(fallback.trim(), table, depth + 1);
        return all;
    });
}

// The root's private aliases (--_bg:var(--pele-bg,#fff); ...) join the table, so each alias reads its role's value.
function aliasesOf(svg, table) {
    const root = svg.match(/^<svg\b[^>]*\sstyle="([^"]*)"/);
    const out = Object.assign({}, table);
    if (root) {
        for (const declaration of root[1].split(";")) {
            const at = declaration.indexOf(":");
            const name = declaration.slice(0, at).trim();
            if (at > 0 && name.startsWith("--"))
                out[name.slice(2)] = declaration.slice(at + 1).trim();
        }
    }
    return out;
}

// Sample input: '<text fill="var(--x)">var(--x)</text>' hands the rewrite "var(--x)" once, from the fill: every attribute value, in either quote style, and never a label's words.
function inAttributes(svg, rewrite) {
    return svg.replace(/<[^<>"']*(?:(?:"[^"]*"|'[^']*')[^<>"']*)*>/g, (tag) => tag.replace(/(=\s*)(?:"([^"]*)"|'([^']*)')/g,
        (all, equals, double, single) => (double !== undefined ? equals + '"' + rewrite(double) + '"' : equals + "'" + rewrite(single) + "'")));
}

// Sample input: a theme without today answers this day's noon UTC in ms; the helper's jail pins TZ=UTC, so local getters read it back as the asked day.
function todayOf(theme) {
    if (theme.today > 0)
        return theme.today;
    const now = new Date();
    return new Date(now.getFullYear(), now.getMonth(), now.getDate(), MIDDAY_HOUR).getTime();
}

// Sample input: 'stroke="color-mix(in srgb,#ffffff 25%,#000000)"' reads 'stroke="#404040"', since neither renderer reads color-mix.
function mixed(text) {
    return text.replace(/color-mix\(\s*in srgb\s*,\s*(#(?:[0-9a-f]{3}){1,2})\s+([\d.]+)%\s*,\s*(#(?:[0-9a-f]{3}){1,2})\s*\)/gi,
        (all, a, share, b) => mix(a, b, parseFloat(share) / PERCENT));
}

// One diagram as SVG whose every colour, font and weight is a concrete value of the theme it is handed.
export function mermaidToSvg(source, theme) {
    theme = theme || {};
    const result = render(withSequenceDefaults(source), {
        measurer: measurerOf(theme),
        fontFamily: theme.font || "sans-serif",
        fontFamilyMono: theme.font || "monospace",
        fontSize: LAYOUT_PX,
        padding: PADDING,
        responsive: false,
        autoDirection: false,
        links: false,
        images: false,
        idPrefix: "flea",
        // A Gantt today marker moves once a day, so one day's renders match byte for byte.
        now: todayOf(theme)
    });
    const svg = result.svg.replace('<g class="pele-lifelines"', LIFELINES);
    const table = aliasesOf(svg, rolesOf(theme));
    const drawn = inAttributes(svg, (value) => mixed(resolve(value, table, 0)));
    const type = detectType(source);
    const old = type === "sequence" ? oldMessageRows(drawn, source) : type === "flowchart" ? oldFlowchartBoxes(drawn, source) : drawn;
    return scaledCanvas(old, theme.bodyPx);
}
