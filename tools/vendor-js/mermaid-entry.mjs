import { render } from "pele";

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

// Sample input: a theme without today answers this day's noon in the helper's own zone in ms; FigureService names the day for a Gantt chart, the day its cache key carries.
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
    const result = render(source, {
        measurer: measurerOf(theme),
        fontFamily: theme.font || "sans-serif",
        fontFamilyMono: theme.font || "monospace",
        fontSize: theme.bodyPx > 0 ? theme.bodyPx : FALLBACK_BODY_PX,
        padding: PADDING,
        responsive: false,
        autoDirection: false,
        links: false,
        images: false,
        idPrefix: "flea",
        // A Gantt today marker moves once a day, so one day's renders match byte for byte.
        now: todayOf(theme)
    });
    const svg = result.svg;
    const table = aliasesOf(svg, rolesOf(theme));
    return inAttributes(svg, (value) => mixed(resolve(value, table, 0)));
}
