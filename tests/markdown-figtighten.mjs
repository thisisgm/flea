// The post-pass over real library output and hand-built shapes: never a broken viewBox, never a cut label, one text per label line.
import { postMermaid } from '../ui/js/FigureWorker.mjs';
import { mermaidToSvg } from '../ui/vendor/mermaid.mjs';
import { actors, nodes, parse, texts } from './mermaid-corpus.mjs';

// A full-width or other non-ASCII glyph advances one em, the widest a label's glyph goes.
const FULL_WIDTH_ADVANCE_EM = 1;
// Per-glyph maximum advance in em over DejaVu Sans, Liberation Sans and Noto Sans, measured with PIL at 1000 px.
const MAX_ADVANCE_EM = {
    '0': 0.636, '1': 0.636, '2': 0.636, '3': 0.636, '4': 0.636, '5': 0.636, '6': 0.636, '7': 0.636, '8': 0.636,
    '9': 0.636, 'a': 0.613, 'b': 0.635, 'c': 0.55, 'd': 0.635, 'e': 0.615, 'f': 0.352, 'g': 0.635, 'h': 0.634,
    'i': 0.278, 'j': 0.278, 'k': 0.579, 'l': 0.278, 'm': 0.974, 'n': 0.634, 'o': 0.612, 'p': 0.635, 'q': 0.635,
    'r': 0.413, 's': 0.521, 't': 0.392, 'u': 0.634, 'v': 0.592, 'w': 0.818, 'x': 0.592, 'y': 0.592, 'z': 0.525,
    'A': 0.684, 'B': 0.686, 'C': 0.722, 'D': 0.77, 'E': 0.667, 'F': 0.611, 'G': 0.778, 'H': 0.752, 'I': 0.339,
    'J': 0.5, 'K': 0.667, 'L': 0.557, 'M': 0.907, 'N': 0.76, 'O': 0.787, 'P': 0.667, 'Q': 0.787, 'R': 0.722,
    'S': 0.667, 'T': 0.611, 'U': 0.732, 'V': 0.684, 'W': 0.989, 'X': 0.685, 'Y': 0.667, 'Z': 0.685, '!': 0.401,
    '"': 0.46, '#': 0.838, '$': 0.636, '%': 0.95, '&': 0.78, '\'': 0.275, '(': 0.39, ')': 0.39, '*': 0.551,
    '+': 0.838, ',': 0.318, '-': 0.361, '.': 0.318, '/': 0.372, ':': 0.337, ';': 0.337, '<': 0.838, '=': 0.838,
    '>': 0.838, '?': 0.556, '@': 1.015, '[': 0.39, '\\': 0.372, ']': 0.39, '^': 0.838, '_': 0.556, '`': 0.5,
    '{': 0.636, '|': 0.551, '}': 0.636, '~': 0.838, ' ': 0.318
};
const ASCII_LIMIT = 0x7f;
const TABLE_FIRST = 32;
const TABLE_LENGTH = 95;
const THOUSAND = 1000;
// The widest face of the three, as the advance table a pane hands the helper, so the library sizes every label at its measured maximum.
const widest = Array.from({ length: TABLE_LENGTH }, (unused, i) => Math.round(MAX_ADVANCE_EM[String.fromCharCode(TABLE_FIRST + i)] * THOUSAND));
const theme = { bg: '#101315', fg: '#c0caf5', accent: '#7aa2f7', font: 'sans-serif', bodyPx: 14, advances: widest, boldAdvances: widest };
const LABEL_FONT_PX = 13;
// v0.3.12's padding beside a box node's label at the body 13 layout, in px: the box is 41.95 wider than its label.
const NODE_PAD = 20.975;
// How far a narrow node may stand from its label's measured width and padding, in px: the layout rounds to a hundredth.
const NARROW_SLACK = 0.05;
const failures = [];
let checks = 0;
function check(ok, why) {
    checks++;
    if (!ok) failures.push(why);
}
// Sample input: <svg viewBox="0 0 100 50"/> answers [0, 0, 100, 50].
function box(svg) {
    return svg.match(/<svg\b[^>]*>/)[0].match(/viewBox="([^"]*)"/)[1].split(/\s+/).map(Number);
}
// Sample input: <svg width="100" height="50"/> answers 100 for "width".
function rootNumber(svg, name) { return Number(svg.match(/<svg\b[^>]*>/)[0].match(new RegExp('\\s' + name + '="([^"]*)"'))[1]); }
function sound(svg, why) {
    const view = box(svg);
    check(view.length === 4 && view.every(Number.isFinite) && view[2] > 0 && view[3] > 0
        && Number.isFinite(rootNumber(svg, 'width')) && rootNumber(svg, 'width') > 0, why + ': the canvas stays finite, got [' + view.join(' ') + ']');
}
// Sample input: "iW" advances 1.267 em, the measured maxima 0.278 and 0.989; a non-ASCII glyph advances one em.
function glyphEm(text) {
    return Array.from(text).reduce((em, glyph) => em + (glyph.codePointAt(0) > ASCII_LIMIT ? FULL_WIDTH_ADVANCE_EM : MAX_ADVANCE_EM[glyph]), 0);
}
// Every label's [left, right] at the measured maximum advances, by its anchor and inherited size and offset.
// Sample input: <g transform="translate(70,0)" font-size="13"><text text-anchor="middle">ab</text></g> reaches 70 less half of ab's 1.248 em at 13 px.
function reaches(svg) {
    return texts(parse(svg)).filter((t) => t.string.trim() !== '').map((t) => {
        const w = glyphEm(t.string) * t.size;
        const share = t.anchor === 'middle' ? 0.5 : t.anchor === 'end' ? 1 : 0;
        return { string: t.string, left: t.x - share * w, right: t.x - share * w + w };
    });
}
// No label reaches past either side of the canvas the library sized.
function uncut(svg, why) {
    const [x, , width] = box(svg);
    const found = reaches(svg);
    check(found.length > 0, why + ': the figure draws a label');
    for (const r of found)
        check(r.left >= x && r.right <= x + width, why + ': "' + r.string + '" stays inside the canvas, ' + r.left.toFixed(1) + ' to ' + r.right.toFixed(1) + ' of ' + x + ' to ' + (x + width));
}
// The library's own canvas, as the post-pass must keep it.
function rawBox(source) {
    return box(mermaidToSvg(source, theme));
}
function real(source) {
    const out = postMermaid(mermaidToSvg(source, theme), theme);
    check(box(out).join(' ') === rawBox(source).join(' '), source.split('\n')[0] + ' figure: the post-pass keeps the library canvas, got [' + box(out).join(' ') + ']');
    return out;
}
const canvas = (inner) => '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 100" width="300" height="100">' + inner + '</svg>';
const frame = '<rect x="100" y="10" width="50" height="40" stroke="#fff" stroke-width="1"/>';

// A text the post-pass cannot read must leave the canvas alone, never an Infinity viewBox.
for (const [name, text] of [
    ['a self-closing text', '<text x="50" y="40" font-size="13"/>'],
    ['a text holding a title', '<text x="50" y="40" font-size="13"><title>a</title>hi</text>'],
    ['a text whose tspan sets no x after the first line', '<text x="50" y="40" font-size="13"><tspan>a</tspan><tspan y="54">b</tspan></text>'],
    ['a text taking its anchor from a style', '<text x="50" y="40" font-size="13" style="text-anchor: middle">hi</text>']
]) {
    const out = postMermaid(canvas(text), theme);
    sound(out, name);
    check(box(out).join(' ') === '0 0 300 100', name + ': the canvas is kept, got [' + box(out).join(' ') + ']');
}
for (const [name, text] of [
    ['a self-closing text beside a rect', '<text x="50" y="40" font-size="13"/>'],
    ['a text holding a title beside a rect', '<text x="50" y="40" font-size="13"><title>a</title>hi</text>']
]) {
    const out = postMermaid(canvas(frame + text), theme);
    sound(out, name);
    check(box(out).join(' ') === '0 0 300 100', name + ': the canvas is kept, got [' + box(out).join(' ') + ']');
}

// A text of positioned tspans becomes one text per line at the tspan's x and y, which QtSvg places, keeping the parent's anchor and size.
const stack = postMermaid(canvas('<text font-size="26" text-anchor="middle"><tspan x="125" y="30">会議室会議</tspan><tspan x="125" y="60">会議室会議室会</tspan></text>'), theme);
sound(stack, 'two stacked tspans');
const stacked = texts(parse(stack));
check(stacked.length === 2 && stacked[0].y === 30 && stacked[1].y === 60 && stacked.every((t) => t.x === 125 && t.size === 26 && t.anchor === 'middle'),
    'two stacked tspans: one text per line at its own x and y with the anchor and size of its parent, got ' + JSON.stringify(stacked));
check(!/<tspan\b/.test(stack), 'two stacked tspans: no tspan is left for QtSvg to misplace');
// A run that styles one word stays a tspan inside its line.
const runs = postMermaid(canvas('<text text-anchor="middle"><tspan x="0" y="-9">The </tspan><tspan font-weight="bold">dog</tspan><tspan x="0" y="9">ran</tspan></text>'), theme);
check(/<text text-anchor="middle" x="0" y="-9">The <tspan font-weight="bold">dog<\/tspan><\/text><text text-anchor="middle" x="0" y="9">ran<\/text>/.test(runs),
    'a bold run stays inside its line, got ' + runs);
// No bundle writes a tspan that moves by dy, which QtSvg ignores, so the vocabulary refuses one.
const relative = '<text x="50" y="40" font-size="13"><tspan>a</tspan><tspan dy="14">b</tspan></text>';
const moved = (() => { try { postMermaid(canvas(relative), theme); return ''; } catch (error) { return error.message; } })();
check(moved === 'unsafe diagram: unlisted attribute dy', 'a tspan moving by dy is refused, got ' + moved);

// Mermaid's own multi-line labels are tspans: each line becomes its own text, and the library's canvas holds every line.
const lines = real('sequenceDiagram\n    A->>B: first<br>second line');
sound(lines, 'real sequence with a two line message');
check(!/<tspan\b/.test(lines), 'real sequence with a two line message: no tspan is left');
uncut(lines, 'real sequence with a two line message');
const flow = real('flowchart TD\n    A["line one<br>line two"] --> B');
sound(flow, 'real flowchart with a two line node');
uncut(flow, 'real flowchart with a two line node');

// A glyph wider than the monospace cell must not let a label be cut, in a node, an edge label and a message.
const ENTITIES = { '"': '#quot;', '`': '#96;' };
const node = (word) => real('flowchart LR\n    A["' + Array.from(word).map((g) => ENTITIES[g] || g).join('') + '"] --> B');
for (const [name, word] of [['CJK', '会議室会議室会議室会議室'], ['capital W', 'WWWWWWWWWWWW'], ['at signs', '@@@@@@@@@@@@'],
    ['capitals and symbols', 'DOMAIN +=^ QUERY'], ['percent runs', '100%%%% #### +++++'], ['capitals', 'OQGDNHUCRBXZ']]) {
    const out = node(word);
    sound(out, name + ' label');
    check(texts(parse(out)).some((t) => t.string === word), name + ' label: the node draws its label whole');
    uncut(out, name + ' label');
}
// Every printable ASCII glyph, in a run, must stay inside the canvas at its measured maximum advance.
// A space alone draws nothing, so it has no reach to check.
for (const glyph of Object.keys(MAX_ADVANCE_EM).filter((g) => /\S/.test(g))) {
    const out = node(glyph.repeat(12));
    check(texts(parse(out)).some((t) => t.string === glyph.repeat(12)), 'the glyph "' + glyph + '" run: the node draws all twelve');
    uncut(out, 'the glyph "' + glyph + '" run');
}
// A narrow label keeps a narrow box: the library sizes it from the narrow glyphs' own advances, not the widest class's.
const narrowLabel = 'iiiiiiiiiiii';
const narrowNode = nodes(parse(node(narrowLabel))).find((n) => n.id === 'A');
const narrowBox = narrowNode ? narrowNode.box[2] - narrowNode.box[0] : NaN;
const narrowWant = glyphEm(narrowLabel) * LABEL_FONT_PX + 2 * NODE_PAD;
check(Math.abs(narrowBox - narrowWant) <= NARROW_SLACK, 'a narrow label keeps a narrow box, ' + narrowBox + ' wide, want ' + narrowWant.toFixed(2));
// A sequence message of capitals and symbols, and an edge label of at signs, must not be cut.
const domain = real('sequenceDiagram\n    A->>B: DOMAIN <=> QUERY @@@@ %%%%');
sound(domain, 'real sequence with a capitals and symbols message');
uncut(domain, 'real sequence with a capitals and symbols message');
const edge = real('flowchart LR\n    A -->|"@@@@@@@@ WWWW DOMAIN"| B');
sound(edge, 'real flowchart with an at sign edge label');
uncut(edge, 'real flowchart with an at sign edge label');
// A long CJK message between two actors stays inside the canvas.
const message = real('sequenceDiagram\n    A->>B: ' + '会議室'.repeat(8));
sound(message, 'real sequence with a long CJK message');
uncut(message, 'real sequence with a long CJK message');

// QtSvg ignores every dy, so a label sits at its y, the box centre; the cap height is the lowest of DejaVu Sans, Liberation Sans and Noto Sans at 1000 px.
const CAP_HEIGHT_EM = 0.714;
const CENTRE_TOLERANCE_PX = 1;
// The ink centre of a one-line label's capitals as QtSvg draws it, less the centre of the node or actor box that holds it.
// Sample input: a text "A" at baseline 4.9 and 14 px in a box from -22 to 22 answers 4.9 - 5 - 0.
function inkOffset(svg, label) {
    const tree = parse(svg);
    const text = texts(tree).find((t) => t.string === label);
    if (!text) return NaN;
    const home = [...nodes(tree), ...actors(tree)].map((n) => n.box).find((b) => b && text.x >= b[0] && text.x <= b[2] && text.y >= b[1] && text.y <= b[3]);
    return home ? text.y - CAP_HEIGHT_EM * text.size / 2 - (home[1] + home[3]) / 2 : NaN;
}
for (const [name, source] of [['flowchart', 'flowchart TD\n    A --> B'], ['sequence', 'sequenceDiagram\n    A->>B: hi']]) {
    const out = real(source);
    check(!/\sdy=/.test(out), name + ' labels carry no dy, which QtSvg ignores');
    const offset = inkOffset(out, 'A');
    check(Math.abs(offset) <= CENTRE_TOLERANCE_PX, name + ' label A: its ink centre sits ' + offset + ' px from its box centre, want within ' + CENTRE_TOLERANCE_PX);
}
// A two line label's lines stack on their own baselines, never on one: Sample input: y="-5.6" then y="15.4" are two baselines 21 apart.
const lineYs = texts(parse(flow)).filter((t) => t.string === 'line one' || t.string === 'line two').map((t) => t.y);
check(lineYs.length === 2 && lineYs.every(Number.isFinite) && lineYs[1] - lineYs[0] > LABEL_FONT_PX, 'a two line label stacks its lines on their own baselines, got [' + lineYs.join(' ') + ']');

console.log('MARKDOWN_FIGTIGHTEN ' + checks + ' checks, ' + failures.length + ' failed');
failures.forEach(why => console.log('FAIL ' + why));
if (failures.length > 0) throw new Error(failures.length + ' figure post-pass checks failed');
