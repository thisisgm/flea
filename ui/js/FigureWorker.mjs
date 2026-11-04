// Maths (MathJax) and diagrams (Pele) through a pure ES module shared by node and quickjs-ng.

// Requests carry {id, kind, source, display, theme}; answers carry {id, svg} or {id, error}; FigureService caches by kind, source, display and theme.

export var MATH_LIMIT = 4096;
export var MERMAID_LIMIT = 32768;
// MathJax uses a 16 px body when the theme supplies no body size.
var DEFAULT_BODY_PX = 16;
// MathJax's TeX SVG sizes one ex at 442 thousandths of an em.
var EX_PER_EM = 0.442;
// Keep converted SVG dimensions to two decimal places.
var PX_ROUNDING_FACTOR = 100;

// A start or end tag whole, a quoted value holding ">" included; the words between two tags are never inside one.
var TAG = /<[^<>"']*(?:(?:"[^"]*"|'[^']*')[^<>"']*)*>/g;
var ATTRIBUTE = /([^\s=<>"'\/]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;
var NAMED_REFERENCES = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };
var DECIMAL_RADIX = 10;
var HEX_RADIX = 16;
var MAX_CODE_POINT = 0x10ffff;

// Sample input: "&#102;ile:x" reads "file:x" and "&amp;#102;" reads "&#102;", in one pass, as an XML parser reads an attribute value.
function decodeReferences(text) {
    return text.replace(/&(?:#(\d+)|#x([0-9a-f]+)|(\w+));/gi, function (all, decimal, hex, name) {
        if (decimal === undefined && hex === undefined)
            return Object.prototype.hasOwnProperty.call(NAMED_REFERENCES, name) ? NAMED_REFERENCES[name] : all;
        var code = decimal !== undefined ? parseInt(decimal, DECIMAL_RADIX) : parseInt(hex, HEX_RADIX);
        return code > 0 && code <= MAX_CODE_POINT ? String.fromCodePoint(code) : all;
    });
}

// The corpus vocabulary, the formulas' and diagrams' output and every style attribute the post-pass writes; anything else refuses.
var ELEMENTS = ["a", "circle", "desc", "ellipse", "g", "line", "path", "polygon", "rect", "svg", "text", "title", "tspan"];
var ATTRIBUTES = ["class", "cx", "cy", "d", "fill", "fill-opacity", "focusable", "font-family", "font-size", "font-style",
    "font-weight", "height", "href", "id", "letter-spacing", "opacity", "pointer-events", "points", "preserveAspectRatio", "r", "role",
    "rx", "ry", "stroke", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-opacity", "stroke-width",
    "style", "text-anchor", "text-decoration", "transform", "viewBox", "width", "word-spacing", "x", "x1", "x2", "xml:space", "y", "y1", "y2"];
// A data attribute holds words, a formula's TeX among them, which no renderer loads or paints, so only its name is checked.
var DATA_ATTRIBUTES = ["array-padding", "bgcolor", "c", "frame", "frame-styles", "hitbox", "icon", "id", "id-align", "idbox", "labels", "latex",
    "line", "mjx-linebox", "mjx-lineno", "mjx-linestack", "mjx-texclass", "mjx-viewBox", "mml-node", "table", "variant", "width-includes-label"];
var STYLE_PROPERTIES = ["min-width", "vertical-align"];
var SVG_NAMESPACE = "http://www.w3.org/2000/svg";

// Sample input: "vertical-align: -2.5px; min-width: 40px" answers null and "fill: red" "style property"; a declaration is an allowed property and a length.
function styleError(style) {
    var declarations = style.split(";");
    for (var i = 0; i < declarations.length; i++) {
        var declaration = /^\s*([a-z-]+)\s*:\s*-?\d*\.?\d+(?:px)?\s*$/i.exec(declarations[i]);
        if (declarations[i].trim() !== "" && (declaration === null || STYLE_PROPERTIES.indexOf(declaration[1].toLowerCase()) < 0))
            return "style property";
    }
    return null;
}

// Sample input: ("href", "#glyph") answers null and ("href", "x.svg#a") "remote link"; the value arrives decoded, and a link or url() reads without the spaces and controls a URL parser drops.
function attributeError(name, value) {
    if (name === "xmlns")
        return value === SVG_NAMESPACE ? null : "namespace";
    if (/^data-/.test(name))
        return DATA_ATTRIBUTES.indexOf(name.slice("data-".length)) >= 0 ? null : "unlisted attribute " + name;
    if (ATTRIBUTES.indexOf(name) < 0)
        return "unlisted attribute " + name;
    var compact = value.replace(/[\x00-\x20\x7f]/g, "");
    if (name === "href" && compact !== "" && compact.charAt(0) !== "#")
        return "remote link";
    if (/url\((?!#)/i.test(compact))
        return "non-local url";
    if (/https?:/i.test(compact))
        return "remote reference";
    if (/var\(|color-mix\(/i.test(compact))
        return "unresolved style";
    return name === "style" ? styleError(value) : null;
}

// Sample input: <svg xmlns="http://www.w3.org/2000/svg"><a href="#e"><path d="M0 0"/></a></svg> answers null; anything the figure corpus never draws answers its reason.
export function checkSafe(svg) {
    var reason = null;
    // Every "<" opens a tag read whole, so a comment, a document type, an instruction or character data is an unlisted element.
    var words = svg.replace(TAG, function (tag) {
        var name = /^<\/?([^\s\/>]*)/.exec(tag)[1];
        if (ELEMENTS.indexOf(name) < 0)
            reason = reason || "unlisted element " + name;
        var unread = tag.slice(0, -1).replace(/^<\/?[^\s\/>]*/, "").replace(ATTRIBUTE, function (all, attribute, double, single) {
            reason = reason || attributeError(attribute, decodeReferences(double !== undefined ? double : single));
            return "";
        });
        if (!/^\s*\/?$/.test(unread))
            reason = reason || "unread tag text";
        return "";
    });
    if (words.indexOf("<") >= 0)
        return reason || "unread markup";
    // The literal checks read all but the namespace name, so a label's words or a formula's TeX still refuse a remote reference.
    var s = svg.replace(/\sxmlns\s*=\s*(?:"[^"]*"|'[^']*')/g, "");
    if (/@import/i.test(s))
        return reason || "remote import";
    if (s.indexOf("http:") >= 0 || s.indexOf("https:") >= 0)
        return reason || "remote reference";
    if (/url\((?!\s*#)/.test(s))
        return reason || "non-local url";
    return reason;
}

// Sample input: <text fill="currentColor">currentColor</text> with a rewrite to "#fff" reads <text fill="#fff">currentColor</text>: every attribute value, in either quote style, and never a label's words.
function inAttributes(svg, rewrite) {
    return svg.replace(TAG, function (tag) {
        return tag.replace(/(=\s*)(?:"([^"]*)"|'([^']*)')/g, function (all, equals, double, single) {
            return double !== undefined ? equals + '"' + rewrite(double) + '"' : equals + "'" + rewrite(single) + "'";
        });
    });
}

function escAttr(s) {
    return String(s).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}

// Presentation attributes a Pele style attribute carries, as attributes since Qt SVG reads no rx and no var() from a style; no font family, since every label is measured in the theme font the root names.
var STYLE_ATTRIBUTES = ["fill", "fill-opacity", "stroke", "stroke-width", "stroke-dasharray", "stroke-dashoffset",
    "stroke-linecap", "stroke-linejoin", "stroke-opacity", "opacity", "rx", "ry",
    "font-size", "font-weight", "font-style", "text-decoration", "letter-spacing", "word-spacing"];

// Sample input: <rect x="1" style="rx:4px;fill:#fff"/> reads <rect x="1" rx="4" fill="#fff"/>; a property already set as an attribute keeps the attribute.
function styleToAttributes(svg) {
    return svg.replace(/<([a-zA-Z]+)((?:\s[^<>]*?)?)\sstyle="([^"]*)"([^<>]*?)(\/?)>/g, function (tag, name, before, style, after, self) {
        if (name === "svg")
            return tag;
        var attrs = before + after;
        var add = [];
        style.split(";").forEach(function (declaration) {
            var at = declaration.indexOf(":");
            if (at <= 0)
                return;
            var prop = declaration.slice(0, at).trim().toLowerCase();
            var value = declaration.slice(at + 1).trim();
            if (STYLE_ATTRIBUTES.indexOf(prop) < 0 || new RegExp("\\s" + prop + "=").test(attrs + " " + add.join(" ")))
                return;
            if (prop === "rx" || prop === "ry")
                value = value.replace(/px$/, "");
            add.push(prop + '="' + escAttr(value) + '"');
        });
        return "<" + name + attrs + (add.length ? " " + add.join(" ") : "") + self + ">";
    });
}

// Sample input: <svg class="pele-icon" x="10" y="8" width="20" height="20" viewBox="0 0 24 24" fill="none"><path/></svg> reads a group translated to 10,8 and scaled by 20/24.
function iconGroups(svg) {
    var open = /<svg\b([^<>]*)>/g;
    var root = open.exec(svg);
    if (!root)
        return svg;
    var head = svg.slice(0, open.lastIndex);
    var body = svg.slice(open.lastIndex).replace(/<svg\b([^<>]*)>([\s\S]*?)<\/svg>(?=[\s\S]*<\/svg>\s*$)/g, function (all, attrs, inner) {
        function num(name, fallback) {
            var found = attrs.match(new RegExp("\\s" + name + '="([^"]*)"'));
            return found ? Number(found[1]) : fallback;
        }
        var box = (attrs.match(/\sviewBox="([^"]*)"/) || [0, ""])[1].trim().split(/[\s,]+/).map(Number);
        var width = num("width", NaN);
        var height = num("height", NaN);
        if (box.length !== 4 || !box.every(Number.isFinite) || !(box[2] > 0) || !(box[3] > 0) || !Number.isFinite(width) || !Number.isFinite(height))
            return "";
        var keep = attrs.replace(/\s(?:x|y|width|height|viewBox|xmlns|preserveAspectRatio)="[^"]*"/g, "");
        var transform = "translate(" + num("x", 0) + "," + num("y", 0) + ") scale(" + width / box[2] + "," + height / box[3] + ") translate(" + -box[0] + "," + -box[1] + ")";
        return "<g" + keep + ' transform="' + transform + '">' + inner + "</g>";
    });
    return head + body;
}

// Sample input: <svg ... style="max-width:100%;--_bg:#101315;font:14px sans-serif;font-family:mono" fill="#c0caf5" role="graphics-document document"> keeps its size and fill and names its font.
function rootFont(svg, t) {
    return svg.replace(/<svg\b[^<>]*>/, function (root) {
        var out = root.replace(/\s(?:style|role|aria-[\w-]+|class)="[^"]*"/g, "");
        return out.replace(/>$/, ' font-family="' + escAttr(t.font || "sans-serif") + '" font-size="' + MERMAID_LAYOUT_PX + '">');
    });
}

// Sample input: <text a="1"><tspan x="0" y="-9">The </tspan><tspan font-weight="bold">dog</tspan><tspan x="0" y="9">ran</tspan></text> reads two texts, <text a="1" x="0" y="-9">The <tspan font-weight="bold">dog</tspan></text> and <text a="1" x="0" y="9">ran</text>, because Qt SVG places no tspan by its own x and y.
function lineTexts(svg) {
    return svg.replace(/<text\b([^<>]*)>((?:<tspan\b[^<>]*>[^<]*<\/tspan>)+)<\/text>/g, function (all, attrs, content) {
        if (/\s(?:x|y)="/.test(attrs))
            return all;
        var lines = [];
        content.replace(/<tspan\b([^<>]*)>([^<]*)<\/tspan>/g, function (span, own, words) {
            var x = own.match(/\sx="([^"]*)"/);
            var y = own.match(/\sy="([^"]*)"/);
            if (x && y)
                lines.push({ x: x[1], y: y[1], runs: [] });
            else if (x || y || lines.length === 0)
                lines = null;
            if (lines)
                lines[lines.length - 1].runs.push({ attrs: own.replace(/\s(?:x|y)="[^"]*"/g, ""), words: words });
            return span;
        });
        if (!lines)
            return all;
        return lines.map(function (line) {
            var body = line.runs.map(function (run) {
                return /\S/.test(run.attrs) ? "<tspan" + run.attrs + ">" + run.words + "</tspan>" : run.words;
            }).join("");
            return "<text" + attrs + ' x="' + line.x + '" y="' + line.y + '">' + body + "</text>";
        }).join("");
    });
}

// Sample input: <g class="pele-edges" stroke="#565f89"><g class="pele-edge"><path class="pele-marker" fill="#565f89" stroke="none"/></g></g> fills the mark with the accent, as Flea drew a link's marks before; a mark inheriting the stroke gets an accent stroke, and an edge a user style colours keeps its marks.
function accentMarks(svg, t) {
    var edges = svg.match(/<g class="pele-edges"[^<>]*\sstroke="([^"]*)"/);
    if (!edges)
        return svg;
    var line = edges[1];
    var accent = escAttr(t.accent || t.fg);
    // One entry per open group: true inside an edge no user style colours, false inside a styled one, or the enclosing group's value.
    var open = [];
    return svg.replace(/<\/g>|<g\b[^<>]*>|<[a-z]+\b[^<>]*\sclass="pele-marker(?:\s[^"]*)?"[^<>]*>/g, function (tag) {
        if (tag === "</g>") {
            open.pop();
            return tag;
        }
        var inherited = open.length ? open[open.length - 1] : false;
        if (/^<g\b/.test(tag)) {
            open.push(/\sclass="pele-edge(?:\s[^"]*)?"/.test(tag) ? !/\sstroke="/.test(tag) : inherited);
            return tag;
        }
        if (!inherited)
            return tag;
        var out = tag.split('fill="' + line + '"').join('fill="' + accent + '"').split('stroke="' + line + '"').join('stroke="' + accent + '"');
        return /\sstroke="/.test(out) ? out : out.replace(/\s*(\/?)>$/, ' stroke="' + accent + '"$1>');
    });
}

// The bundle lays a diagram out at body 13 and scales its canvas to the theme's body size, as v0.3.12 did, so its text is set at 13.
const MERMAID_LAYOUT_PX = 13;

// SVG's defaults for a stroke the figure does not style: one unit wide, mitred joins cut off past a ratio of 4, butt caps.
const DEFAULT_STROKE_WIDTH = 1;
const DEFAULT_MITER_LIMIT = 4;
const HALF_STROKE = 0.5;
// The middle of two ends, and the quarter of a half turn in degrees.
const HALF = 0.5;
const RIGHT_ANGLE = 90;
const DEGREES = 180;
// The canvas grows in hundredths of a unit, rounded outward, so a stroke on its edge is never cut by rounding.
const ROOM_PRECISION = 100;
const IDENTITY = [1, 0, 0, 1, 0, 0];
const PATH_ARGUMENTS = { M: 2, L: 2, H: 1, V: 1, C: 6, S: 4, Q: 4, T: 2, A: 7, Z: 0 };

// Sample input: ([1, 0, 0, 1, 5, 0], [2, 0, 0, 2, 0, 0]) answers [2, 0, 0, 2, 5, 0], the second applied first.
function compose(m, n) {
    return [m[0] * n[0] + m[2] * n[1], m[1] * n[0] + m[3] * n[1], m[0] * n[2] + m[2] * n[3], m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4], m[1] * n[4] + m[3] * n[5] + m[5]];
}

// Sample input: "translate(10,4) rotate(90)" answers [0, 1, -1, 0, 10, 4]; a skew or anything unread answers null.
function transformOf(text) {
    var m = IDENTITY;
    var rest = String(text).replace(/([a-zA-Z]+)\s*\(([^()]*)\)/g, function (all, name, args) {
        var a = args.trim() === "" ? [] : args.trim().split(/[\s,]+/).map(Number);
        var r;
        if (!a.every(Number.isFinite))
            m = null;
        else if (name === "translate" && (a.length === 1 || a.length === 2))
            r = [1, 0, 0, 1, a[0], a.length > 1 ? a[1] : 0];
        else if (name === "scale" && (a.length === 1 || a.length === 2))
            r = [a[0], 0, 0, a.length > 1 ? a[1] : a[0], 0, 0];
        else if (name === "rotate" && (a.length === 1 || a.length === 3)) {
            var t = a[0] * Math.PI / DEGREES;
            var cx = a.length > 1 ? a[1] : 0;
            var cy = a.length > 1 ? a[2] : 0;
            r = compose(compose([1, 0, 0, 1, cx, cy], [Math.cos(t), Math.sin(t), -Math.sin(t), Math.cos(t), 0, 0]), [1, 0, 0, 1, -cx, -cy]);
        } else if (name === "matrix" && a.length === 6)
            r = a;
        else
            m = null;
        if (m)
            m = compose(m, r);
        return "";
    });
    return rest.trim() === "" ? m : null;
}

// Sample input: "4px" answers 4 and "2" answers 2; a width in any other unit answers NaN, which the figure cannot place.
function strokeWidthOf(text) {
    var found = String(text).trim().match(/^(\d+(?:\.\d+)?|\.\d+)(px)?$/);
    return found ? Number(found[1]) : NaN;
}

// Sample input: <polygon points="0,0 10,0 5,8"/> answers one closed run of three joins, and <path d="M0,0C5,5 5,5 10,0"/> an open run whose control points join nothing: a shape's outline as runs of points, each marked where a stroke joins two segments, plus the loose points a curve or arc may reach.
function outlineOf(name, attrs) {
    function num(key, fallback) {
        var found = attrs.match(new RegExp("\\s" + key + '="([^"]*)"'));
        return found ? Number(found[1]) : fallback;
    }
    var runs = [];
    var loose = [];
    if (name === "rect" || name === "circle" || name === "ellipse") {
        var rect = name === "rect";
        var rx = rect ? 0 : name === "circle" ? num("r", NaN) : num("rx", NaN);
        var ry = rect ? 0 : name === "circle" ? num("r", NaN) : num("ry", NaN);
        var x = rect ? num("x", 0) : num("cx", 0) - rx;
        var y = rect ? num("y", 0) : num("cy", 0) - ry;
        var w = rect ? num("width", NaN) : 2 * rx;
        var h = rect ? num("height", NaN) : 2 * ry;
        // A rounded rect, a circle and an ellipse have no corner for a miter, so only a sharp rect joins at its corners.
        var sharp = rect && !(num("rx", 0) > 0) && !(num("ry", 0) > 0);
        runs.push({ closed: true, points: [[x, y, sharp], [x + w, y, sharp], [x + w, y + h, sharp], [x, y + h, sharp]] });
    } else if (name === "line") {
        runs.push({ closed: false, points: [[num("x1", 0), num("y1", 0), false], [num("x2", 0), num("y2", 0), false]] });
    } else if (name === "polygon") {
        var found = attrs.match(/\spoints="([^"]*)"/);
        var n = found ? found[1].trim().split(/[\s,]+/).map(Number) : [];
        var points = [];
        for (var i = 0; i + 1 < n.length; i += 2)
            points.push([n[i], n[i + 1], true]);
        if (n.length % 2 !== 0)
            points.push([NaN, NaN, false]);
        runs.push({ closed: true, points: points });
    } else if (name === "path") {
        var d = attrs.match(/\sd="([^"]*)"/);
        var px = 0;
        var py = 0;
        var run = null;
        String(d ? d[1] : "").replace(/([MLHVCSQTAZmlhvcsqtaz])([^MLHVCSQTAZmlhvcsqtaz]*)/g, function (all, letter, body) {
            var cmd = letter.toUpperCase();
            var relative = letter !== cmd;
            var a = body.match(/[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:[eE][-+]?\d+)?/g) || [];
            var step = PATH_ARGUMENTS[cmd];
            if (cmd === "Z") {
                if (run)
                    run.closed = true;
                run = null;
                return all;
            }
            if (a.length === 0 || a.length % step !== 0) {
                runs.push({ closed: false, points: [[NaN, NaN, false]] });
                return all;
            }
            for (var k = 0; k < a.length; k += step) {
                var v = a.slice(k, k + step).map(Number);
                var ox = relative ? px : 0;
                var oy = relative ? py : 0;
                if (cmd === "M" && k === 0) {
                    run = { closed: false, points: [] };
                    runs.push(run);
                } else if (!run) {
                    run = { closed: false, points: [[px, py, true]] };
                    runs.push(run);
                }
                if (cmd === "H")
                    px = ox + v[0];
                else if (cmd === "V")
                    py = oy + v[0];
                else if (cmd === "A") {
                    var ex = ox + v[5];
                    var ey = oy + v[6];
                    loose.push.apply(loose, arcExtremes(px, py, v[0], v[1], v[2], v[3] !== 0, v[4] !== 0, ex, ey));
                    px = ex;
                    py = ey;
                } else {
                    // A curve stays inside the hull of its control points, which stand in the run so a join reads the curve's tangent from them.
                    for (var c = 0; c + 2 < step; c += 2)
                        run.points.push([ox + v[c], oy + v[c + 1], false]);
                    px = ox + v[step - 2];
                    py = oy + v[step - 1];
                }
                run.points.push([px, py, true]);
            }
            return all;
        });
    }
    return { runs: runs, loose: loose };
}

const QUARTER_TURN = Math.PI * HALF;
const FULL_TURN = 2 * Math.PI;
const QUARTERS = 4;
// Sample input: (10, 0, 10, 10, 0, false, true, -10, 0), the lower half turn about (0, 0), answers its bottom point [[0, 10]], its ends counting elsewhere: the points of an arc reaching furthest up, down, left and right, from SVG's endpoint form (F.6.5); an axis-aligned ellipse is a circle with y stretched by ry / rx, and one turned off the axes answers its ends' box grown by its larger radius.
function arcExtremes(x0, y0, rx, ry, turn, large, sweep, x1, y1) {
    var quarter = Math.abs(turn) % DEGREES;
    var across = quarter === RIGHT_ANGLE;
    var r = Math.abs(across ? ry : rx);
    var stretch = r > 0 ? Math.abs(across ? rx : ry) / r : 0;
    if ((quarter !== 0 && !across) || !(stretch > 0)) {
        var far = Math.max(Math.abs(rx), Math.abs(ry));
        return [[Math.min(x0, x1) - far, Math.min(y0, y1) - far], [Math.max(x0, x1) + far, Math.max(y0, y1) + far]];
    }
    var hx = (x0 - x1) * HALF;
    var hy = (y0 - y1) / stretch * HALF;
    var chord = hx * hx + hy * hy;
    if (!(chord > 0))
        return [];
    // A radius too short for its chord grows until the arc is a half turn.
    r = Math.max(r, Math.sqrt(chord));
    var lift = Math.sqrt(Math.max(0, (r * r - chord) / chord)) * (large === sweep ? -1 : 1);
    var cx = lift * hy + (x0 + x1) * HALF;
    var cy = -lift * hx + (y0 + y1) / stretch * HALF;
    var from = Math.atan2(y0 / stretch - cy, x0 - cx);
    var span = Math.atan2(y1 / stretch - cy, x1 - cx) - from;
    if (sweep && span < 0)
        span += FULL_TURN;
    else if (!sweep && span > 0)
        span -= FULL_TURN;
    var out = [];
    for (var q = -QUARTERS; q <= 2 * QUARTERS; q++) {
        var a = q * QUARTER_TURN;
        var into = sweep ? a - from : from - a;
        if (into > 0 && into < Math.abs(span))
            out.push([cx + r * Math.cos(a), (cy + r * Math.sin(a)) * stretch]);
    }
    return out;
}

// Sample input: a right-angled corner answers the miter ratio 1.41; a join sharper than the limit is bevelled, so it answers 1.
function miterRatio(prev, at, next, limit) {
    var ux = prev[0] - at[0];
    var uy = prev[1] - at[1];
    var vx = next[0] - at[0];
    var vy = next[1] - at[1];
    var lu = Math.hypot(ux, uy);
    var lv = Math.hypot(vx, vy);
    if (!(lu > 0) || !(lv > 0))
        return 1;
    var cos = (ux * vx + uy * vy) / (lu * lv);
    var sinHalf = Math.sqrt(Math.max(0, (1 - cos) * HALF));
    var ratio = sinHalf > 0 ? 1 / sinHalf : Infinity;
    return ratio <= limit ? Math.max(1, ratio) : 1;
}

// Sample input: <svg viewBox="0 0 50 140" width="50" height="140"><rect x="1" y="94" width="48" height="45" stroke="#fff" stroke-width="8"/></svg> reads viewBox="-3 -3 56 146" width="56" height="146": the canvas grows, never shrinks, to hold every stroke as it paints, half its width past its outline and more at a mitred join or a square cap; a stroke inside the canvas, or a shape it cannot place, leaves it alone.
function strokeRoom(svg) {
    var root = svg.match(/<svg\b[^<>]*>/);
    var view = root ? root[0].match(/\sviewBox="([^"]*)"/) : null;
    var box = view ? view[1].trim().split(/[\s,]+/).map(Number) : [];
    if (box.length !== 4 || !box.every(Number.isFinite) || !(box[2] > 0) || !(box[3] > 0))
        return svg;
    var low = [box[0], box[1]];
    var high = [box[0] + box[2], box[1] + box[3]];
    var stack = [{ m: IDENTITY, stroke: "none", width: DEFAULT_STROKE_WIDTH, join: "miter", cap: "butt", limit: DEFAULT_MITER_LIMIT }];
    function own(attrs, key) {
        var found = attrs.match(new RegExp("\\s" + key + '="([^"]*)"'));
        return found ? found[1] : null;
    }
    function reach(m, x, y, pad) {
        // A transform stretches the stroke by at most the larger of its column lengths.
        var stretch = m ? Math.max(Math.hypot(m[0], m[1]), Math.hypot(m[2], m[3])) : 0;
        var tx = m[0] * x + m[2] * y + m[4];
        var ty = m[1] * x + m[3] * y + m[5];
        low = [Math.min(low[0], tx - pad * stretch), Math.min(low[1], ty - pad * stretch)];
        high = [Math.max(high[0], tx + pad * stretch), Math.max(high[1], ty + pad * stretch)];
    }
    var body = svg.slice(root.index + root[0].length);
    var placed = true;
    body.replace(/<(\/?)([a-zA-Z][\w:-]*)((?:\s[^<>]*?)?)(\/?)>/g, function (tag, closing, name, attrs, self) {
        if (closing) {
            if (stack.length > 1)
                stack.pop();
            return tag;
        }
        var up = stack[stack.length - 1];
        var transform = own(attrs, "transform");
        var m = up.m && transform !== null ? (transformOf(transform) ? compose(up.m, transformOf(transform)) : null) : up.m;
        var widthText = own(attrs, "stroke-width");
        var limitText = own(attrs, "stroke-miterlimit");
        var state = { m: m, stroke: own(attrs, "stroke") !== null ? own(attrs, "stroke") : up.stroke,
            width: widthText !== null ? strokeWidthOf(widthText) : up.width, join: own(attrs, "stroke-linejoin") || up.join,
            cap: own(attrs, "stroke-linecap") || up.cap, limit: limitText !== null ? Number(limitText) : up.limit };
        var shape = /^(rect|circle|ellipse|line|polygon|path)$/.test(name);
        if (shape && state.stroke !== "none" && state.stroke !== "transparent") {
            var outline = outlineOf(name, attrs);
            var half = state.width * HALF_STROKE;
            var capRatio = state.cap === "square" ? Math.SQRT2 : 1;
            var mitred = state.join === "miter" || state.join === "miter-clip" || state.join === "arcs";
            var known = state.m !== null && Number.isFinite(half) && half >= 0 && state.limit >= 1;
            outline.runs.forEach(function (run) {
                var n = run.points.length;
                run.points.forEach(function (p, i) {
                    if (!Number.isFinite(p[0]) || !Number.isFinite(p[1]) || !known) {
                        placed = false;
                        return;
                    }
                    var end = !run.closed && (i === 0 || i === n - 1);
                    var ratio = end ? capRatio : p[2] && mitred && n > 2
                        ? miterRatio(run.points[(i + n - 1) % n], p, run.points[(i + 1) % n], state.limit) : 1;
                    reach(state.m, p[0], p[1], half * ratio);
                });
            });
            outline.loose.forEach(function (p) {
                if (known && Number.isFinite(p[0]) && Number.isFinite(p[1]))
                    reach(state.m, p[0], p[1], half);
                else
                    placed = false;
            });
        }
        if (!self)
            stack.push(state);
        return tag;
    });
    var grown = [Math.floor(low[0] * ROOM_PRECISION) / ROOM_PRECISION, Math.floor(low[1] * ROOM_PRECISION) / ROOM_PRECISION];
    var far = [Math.ceil(high[0] * ROOM_PRECISION) / ROOM_PRECISION, Math.ceil(high[1] * ROOM_PRECISION) / ROOM_PRECISION];
    if (!placed || (grown[0] >= box[0] && grown[1] >= box[1] && far[0] <= box[0] + box[2] && far[1] <= box[1] + box[3]))
        return svg;
    var x = Math.min(box[0], grown[0]);
    var y = Math.min(box[1], grown[1]);
    var w = Math.round((Math.max(box[0] + box[2], far[0]) - x) * ROOM_PRECISION) / ROOM_PRECISION;
    var h = Math.round((Math.max(box[1] + box[3], far[1]) - y) * ROOM_PRECISION) / ROOM_PRECISION;
    // The canvas keeps its scale: the root's width and height grow by the share its viewBox does.
    var head = root[0].replace(/\sviewBox="[^"]*"/, ' viewBox="' + x + " " + y + " " + w + " " + h + '"')
        .replace(/\s(width|height)="([\d.]+)"/g, function (all, key, value) {
            var share = key === "width" ? w / box[2] : h / box[3];
            return " " + key + '="' + Math.round(Number(value) * share * ROOM_PRECISION) / ROOM_PRECISION + '"';
        });
    return svg.slice(0, root.index) + head + body;
}

// Pele's SVG with every value concrete, rewritten into the subset Qt SVG and the GPUI renderer both draw, then checked.
export function postMermaid(svg, t) {
    var out = lineTexts(iconGroups(rootFont(styleToAttributes(svg), t)));
    // An icon draws in currentColor, which the root's text colour set before its style went.
    out = inAttributes(out, function (value) { return value.split("currentColor").join(t.fg); });
    out = accentMarks(out, t);
    // A link and an image never ship: the bundle renders neither, and a stray one unwraps or goes.
    out = out.replace(/<a\s[^<>]*>/g, "").replace(/<\/a>/g, "").replace(/<image\b[^<>]*\/?>/g, "");
    // The layout pads its geometry by one unit, so a border wider than two units, a user style's, would be cut at the canvas edge.
    out = strokeRoom(out);
    var bad = checkSafe(out);
    if (bad)
        throw new Error("unsafe diagram: " + bad);
    return out;
}

export function postMath(svg, t) {
    if (svg.indexOf("merror") >= 0 || svg.indexOf("data-mjx-error") >= 0)
        throw new Error("formula did not render");
    var out = inAttributes(svg, function (value) { return value.split("currentColor").join(t.fg); });
    // MathJax's TeX SVG has 442 units per ex; a formula draws its ex at the body x-height the pane measured, or the em rule when none came.
    var exPx = t.exPx > 0 ? t.exPx : (t.bodyPx || DEFAULT_BODY_PX) * EX_PER_EM;
    out = out.replace(/(-?\d+(?:\.\d+)?)ex/g, function (m, v) {
        var px = Math.round(parseFloat(v) * exPx * PX_ROUNDING_FACTOR) / PX_ROUNDING_FACTOR;
        return String(px) + "px";
    });
    var bad = checkSafe(out);
    if (bad)
        throw new Error("unsafe formula: " + bad);
    return out;
}

// Render through the two bundle entry points in apis, returning SVG or throwing a reason the caller puts in {id, error}.
export function renderFigure(kind, source, display, theme, apis) {
    if (kind !== "math" && kind !== "mermaid")
        throw new Error("unknown figure kind");
    if (kind === "math" && source.length > MATH_LIMIT)
        throw new Error("formula over 4 KiB");
    if (kind === "mermaid" && source.length > MERMAID_LIMIT)
        throw new Error("diagram over 32 KiB");
    if (kind === "math")
        return postMath(apis.texToSvg(source, !!display), theme);
    // The bundle takes the whole theme: its colour roles, its font and body size, and the font's own advance tables, so every label is sized from the font that is drawn.
    return postMermaid(apis.mermaidToSvg(source, theme), theme);
}
