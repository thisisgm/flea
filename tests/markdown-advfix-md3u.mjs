import { postMermaid, renderFigure } from '../ui/js/FigureWorker.mjs';
import { mermaidToSvg } from '../ui/vendor/mermaid.mjs';
import { nodes, parse, texts } from './mermaid-corpus.mjs';

const EPSILON = 0.000001;
const LAYOUT_PX = 13;
const theme = { bg: '#101315', fg: '#c0caf5', accent: '#7aa2f7', font: 'monospace' };
const sizes = [12, 14, 28];
const cases = [];
const failures = [];
let checks = 0;
function check(ok, why) {
    checks++;
    if (!ok) failures.push(why);
}
// Sample input: <rect y="10" height="80" stroke-width="8"/>.
function attr(tag, name) {
    const match = tag.match(new RegExp('\\s' + name + '="([^"]*)"'));
    return match ? match[1] : '';
}
// Sample input: <svg viewBox="0 0 100 50"/> answers [0, 0, 100, 50].
function box(svg) { return attr(svg.match(/<svg\b[^>]*>/)[0], 'viewBox').split(/\s+/).map(Number); }
const sources = {
    F2: 'flowchart TD\n  A["AAAAAAAAAAAA"] --> B["BBBBBBBBBBBB"]',
    F7: 'flowchart TD\nA --> B\nstyle B stroke-width:8'
};
for (const id of Object.keys(sources)) {
    const source = sources[id];
    for (const bodyPx of sizes) {
        const svg = renderFigure('mermaid', source, true, { ...theme, bodyPx }, { mermaidToSvg });
        const viewBox = box(svg);
        const width = Number(attr(svg.match(/<svg\b[^>]*>/)[0], 'width'));
        const scale = width / viewBox[2];
        const tree = parse(svg);
        const labels = texts(tree);
        const node = nodes(tree).find((n) => n.id === (id === 'F2' ? 'A' : 'B'));
        const rect = node.el.children.find((el) => el.name === 'rect');
        // The bundle lays the figure out at body 13, as v0.3.12 did, and the canvas scales by the body size over 13, so every label draws at the body size.
        check(Math.abs(scale - bodyPx / LAYOUT_PX) < EPSILON, id + ' body ' + bodyPx + ': canvas scales with the body size, got ' + scale);
        check(labels.length > 0 && labels.every((t) => t.size === LAYOUT_PX), id + ' body ' + bodyPx + ': labels laid out at 13, got ' + labels.map((t) => t.size));
        const half = Number(rect.attrs['stroke-width'] ?? 1) / 2;
        const paint = [node.box[0] - half, node.box[1] - half, node.box[2] + half, node.box[3] + half];
        const paintBottom = paint[3];
        if (id === 'F7') check(viewBox[0] <= paint[0] && viewBox[1] <= paint[1] && viewBox[0] + viewBox[2] >= paint[2] && viewBox[1] + viewBox[3] >= paint[3],
            'F7 body ' + bodyPx + ': thick library stroke clipped, paints ' + paint + ' in ' + viewBox);
        cases.push({ id, bodyPx, svg, source, viewBox, scale,
            nodeWidth: (node.box[2] - node.box[0]) * scale,
            canvasWidth: width, label: id === 'F2' ? 'AAAAAAAAAAAA' : 'B',
            fontPx: labels[0].size * scale, paintBottom });
    }
}
const MITER_VERTEX_Y = 15;
const MITER_HALF_STROKE = 4;
const MITER_SEGMENT_DX = 5;
const MITER_SEGMENT_DY = 10;
// Both bottom joins have miter ratio sqrt(5), below SVG's default limit of 4.
const MITER_TIP_Y = MITER_VERTEX_Y + MITER_HALF_STROKE
    * Math.hypot(MITER_SEGMENT_DX, MITER_SEGMENT_DY) / MITER_SEGMENT_DX;
const primitiveCases = [
    ['rect', '<rect x="5" y="5" width="10" height="10" stroke="#c0caf5" stroke-width="6"/>', 2, 18],
    ['rect-large', '<rect x="10" y="10" width="80" height="80" stroke="#c0caf5" stroke-width="8"/>', 6, 94],
    ['line', '<line x1="5" y1="10" x2="15" y2="10" stroke="#c0caf5" stroke-width="8"/>', 6, 14],
    ['square-cap', '<line x1="5" y1="5" x2="15" y2="15" stroke="#c0caf5" stroke-width="8" stroke-linecap="square"/>', 5 - 4 * Math.SQRT2, 15 + 4 * Math.SQRT2],
    ['circle', '<circle cx="10" cy="10" r="5" stroke="#c0caf5" stroke-width="8"/>', 1, 19],
    ['ellipse', '<ellipse cx="10" cy="10" rx="3" ry="5" stroke="#c0caf5" stroke-width="8"/>', 1, 19],
    ['polygon', '<polygon points="5,5 15,5 10,15" stroke="#c0caf5" stroke-width="8"/>', 1, MITER_TIP_Y],
    ['path', '<path d="M5,5L10,15L15,5" stroke="#c0caf5" stroke-width="8"/>', 1, MITER_TIP_Y],
    ['arc', '<path d="M2,6A8,8 0 0 1 18,6" fill="none" stroke="#c0caf5" stroke-width="4" stroke-linecap="round"/>', -4, 8],
    ['translated', '<g transform="translate(0,-4)"><rect x="5" y="5" width="10" height="10" stroke="#c0caf5" stroke-width="6" rx="2"/></g>', -2, 14],
    ['rotated', '<g transform="rotate(90 10 10)"><line x1="1" y1="10" x2="9" y2="10" stroke="#c0caf5" stroke-width="4" stroke-linecap="square"/></g>', -1, 11]
];
for (const [name, primitive, low, high] of primitiveCases) {
    const svg = postMermaid('<svg width="100" height="100" viewBox="0 0 100 100">' + primitive + '</svg>', theme);
    const bounds = box(svg);
    check(bounds[1] <= low && bounds[1] + bounds[3] >= high, 'F7 ' + name + ': stroke extent clipped');
}
// No bundle draws a polyline, so the vocabulary refuses one before its stroke is measured.
const polyline = (() => { try { postMermaid('<svg width="100" height="100" viewBox="0 0 100 100"><polyline points="5,5 10,15 15,5" stroke="#c0caf5" stroke-width="8"/></svg>', theme); return ''; } catch (error) { return error.message; } })();
check(polyline === 'unsafe diagram: unlisted element polyline', 'F7 polyline: refused, got ' + polyline);
for (const unknown of ['stroke-width="8px"', 'stroke-width="inherit"', 'stroke-width=""', 'style="stroke-width:8"']) {
    const svg = postMermaid('<svg width="100" height="100" viewBox="0 0 100 100"><rect y="10" height="80" stroke="#c0caf5" ' + unknown + '/></svg>', theme);
    check(box(svg).join(' ') === '0 0 100 100', 'F7 unknown ' + unknown + ': retain original canvas');
}
console.log('MARKDOWN_MD3U ' + checks + ' checks, ' + failures.length + ' failed');
failures.forEach(why => console.log('FAIL ' + why));
console.log(JSON.stringify(cases));
if (failures.length) throw new Error('md3u figure regressions');
