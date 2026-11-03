// Shared figure assertions read the live tree and its painted pixels.
var ACCENT_BLEND_TOLERANCE = 3;
var FONT_SIZE_EPSILON = 0.000001;
var RGBA_CHANNELS = 4;
var LINK_BLUE_MIN = 200;
var LINK_RED_MAX = 110;
var LINK_GREEN_MAX = 170;
var MATH_INK_MIN_BODY_RATIO = 1.04; // Minimum painted x^2 height relative to the body font.
var MATH_INK_MAX_BODY_RATIO = 1.25; // Maximum painted x^2 height relative to the body font.
var FIGURE_GAP_TOLERANCE_PX = 2; // Painted figure gaps may differ from paragraph gaps by this many pixels.
var BLOCK_GAP_TOLERANCE_PX = 0.5; // Drawn block edges may differ from the token gap by this many pixels.
var GROUND_R = 16; // The preview ground in the grab; ink is any pixel holding a channel the ground lacks.
var GROUND_G = 19;
var GROUND_B = 21;

// A pixel counts as ink when any channel is set and the pixel is not the preview ground.
function isInk(r, g, b) {
    return (r || g || b) && !(r === GROUND_R && g === GROUND_G && b === GROUND_B);
}

function figure(md, index) {
    var block = md.blockItem(index);
    if (!block)
        return null;
    for (var b = 0; b < block.children.length; b++) {
        var box = block.children[b];
        if (box.objectName !== "figureBox")
            continue;
        for (var i = 0; i < box.children.length; i++)
            if (box.children[i].objectName === "figureItem")
                return box.children[i];
    }
    return null;
}

function bodyFont(md) {
    var block = md.blockItem(5);
    if (block)
        for (var i = 0; i < block.children.length; i++)
            if (block.children[i].visible && block.children[i].font !== undefined)
                return block.children[i].font;
    return null;
}

var LABEL_PAINT = ["font-size", "font-family", "fill"];

// Sample input: <svg font-size="14"><g fill="#c0caf5"><text>A</text></g></svg> reads one label whose size is 14 and fill #c0caf5, each from the nearest element that sets it.
function labelPaints(svg) {
    var tags = svg.match(/<\/?[a-zA-Z][^<>]*>/g) || [];
    var stack = [];
    var labels = [];
    for (var i = 0; i < tags.length; i++) {
        var tag = tags[i];
        if (tag.charAt(1) === "/") {
            stack.pop();
            continue;
        }
        var parent = stack.length > 0 ? stack[stack.length - 1] : {};
        var paint = {};
        for (var p = 0; p < LABEL_PAINT.length; p++) {
            var own = tag.match(new RegExp("\\s" + LABEL_PAINT[p] + "=\"([^\"]*)\""));
            paint[LABEL_PAINT[p]] = own ? own[1] : parent[LABEL_PAINT[p]];
        }
        if (/^<(?:text|tspan)[\s>/]/.test(tag))
            labels.push({ tag: tag, paint: paint });
        if (!/\/>$/.test(tag))
            stack.push(paint);
    }
    return labels;
}

// Sample input: <svg width="20" viewBox="0 0 20 20" font-size="14" font-family="monospace" fill="#c0caf5"><text>A</text></svg>.
function labelError(svg, font, foreground) {
    var labels = labelPaints(svg);
    if (labels.length === 0)
        return "no Mermaid labels";
    var escaped = font.family.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
    var root = svg.match(/<svg\b[^>]*>/)[0];
    var width = Number(root.match(/\swidth="([^"]*)"/)[1]);
    var viewWidth = Number(root.match(/\sviewBox="([^"]*)"/)[1].trim().split(/\s+/)[2]);
    for (var i = 0; i < labels.length; i++) {
        var label = labels[i];
        var size = label.paint["font-size"];
        if (size === undefined || Math.abs(Number(size) * width / viewWidth - font.pixelSize) > FONT_SIZE_EPSILON)
            return "label size differs from body " + font.pixelSize + "px: " + label.tag;
        if (label.paint["font-family"] !== escaped)
            return "label family differs from body " + font.family + ": " + label.tag;
        if (label.paint.fill !== foreground)
            return "label ink differs from foreground: " + label.tag;
    }
    return "";
}

// Sample input: <path fill="#101315" stroke="#7aa2f7"/>.
function paletteError(svg, roles) {
    var paints = svg.match(/(?:fill|stroke)="[^"]*"/g) || [];
    for (var i = 0; i < paints.length; i++) {
        var value = paints[i].replace(/^[^"]*"/, "").replace(/"$/, "");
        if (value !== "none" && roles.indexOf(value) < 0)
            return "diagram paint outside theme roles: " + value;
    }
    return "";
}

function surfaceError(item) {
    if (!item.visible)
        return "";
    if (item.border !== undefined && item.color !== undefined
            && item.width > 0 && item.height > 0) {
        if (item.border.width > 0 || item.color.a > 0)
            return "visible rectangle border=" + item.border.width + " alpha=" + item.color.a;
    }
    for (var i = 0; i < item.children.length; i++) {
        var error = surfaceError(item.children[i]);
        if (error !== "")
            return error;
    }
    return "";
}

function paddingCount(cacheEnd, measuredEnd, paragraphHeight, spacing) {
    if (!(paragraphHeight > 0) || !(spacing >= 0))
        throw new Error("the fixture has no measured paragraph extent");
    return Math.max(2, Math.ceil((cacheEnd - measuredEnd) / (paragraphHeight + spacing)) + 2);
}

function farTop(md, count, paragraphHeight) {
    var last = md.blockItem(5);
    if (!last)
        return -1;
    return last.y + last.height + count * (paragraphHeight + md.bodyItem.spacing) + md.bodyItem.spacing;
}

function inkBounds(pixels, width, rect, ground, chrome) {
    var height = pixels.length / (width * RGBA_CHANNELS);
    if (!(width > 0) || !Number.isInteger(width) || !Number.isInteger(height)
            || !Number.isInteger(rect.x) || !Number.isInteger(rect.y) || rect.x < 0 || rect.y < 0
            || !(rect.w > 0 && rect.h > 0) || rect.x + rect.w > width || rect.y + rect.h > height)
        throw new Error("ink rect " + JSON.stringify(rect) + " outside buffer " + width + "x" + height);
    var top = -1;
    var bottom = -1;
    for (var y = rect.y; y < rect.y + rect.h; y++) {
        for (var x = rect.x; x < rect.x + rect.w; x++) {
            var at = (y * width + x) * RGBA_CHANNELS;
            var r = pixels[at], g = pixels[at + 1], b = pixels[at + 2];
            if ((r === ground[0] && g === ground[1] && b === ground[2])
                    || (r === chrome[0] && g === chrome[1] && b === chrome[2])
                    || (r === 0 && g === 0 && b === 0))
                continue;
            if (top < 0)
                top = y;
            bottom = y;
            break;
        }
    }
    return { top: top, bottom: bottom, height: top < 0 ? 0 : bottom - top + 1 };
}

// Keep every sent source even after its delegate or ticket disappears.
function farRequestError(history, source, sends) {
    if (history.length !== sends)
        return "request history recorded " + history.length + " of " + sends + " sends";
    for (var i = 0; i < history.length; i++)
        if (history[i].source === source)
            return "far figure request " + history[i].id + " was sent despite sitting past the cache";
    return "";
}

// Antialiasing places themed accent pixels along the ground-to-accent RGB segment.
function accentBlend(pixel, ground, accent) {
    var dot = 0;
    var lengthSquared = 0;
    for (var i = 0; i < ground.length; i++) {
        var delta = accent[i] - ground[i];
        dot += (pixel[i] - ground[i]) * delta;
        lengthSquared += delta * delta;
    }
    var coverage = lengthSquared > 0 ? Math.max(0, Math.min(1, dot / lengthSquared)) : 0;
    var distanceSquared = 0;
    for (var c = 0; c < ground.length; c++) {
        var residual = pixel[c] - ground[c] - coverage * (accent[c] - ground[c]);
        distanceSquared += residual * residual;
    }
    return distanceSquared <= ACCENT_BLEND_TOLERANCE * ACCENT_BLEND_TOLERANCE;
}

function defaultLinkBlue(pixel, ground, accent) {
    return pixel[2] >= LINK_BLUE_MIN && pixel[0] <= LINK_RED_MAX && pixel[1] <= LINK_GREEN_MAX
        && !accentBlend(pixel, ground, accent);
}

function mathError(rows, bodyPx) {
    var minimum = Math.ceil(bodyPx * MATH_INK_MIN_BODY_RATIO);
    var maximum = Math.ceil(bodyPx * MATH_INK_MAX_BODY_RATIO);
    return rows >= minimum && rows <= maximum ? ""
        : "x^2 paints " + rows + " rows at body " + bodyPx + "px, want " + minimum + ".." + maximum;
}

function spacingError(previous, figure, next, ink, paragraphGap) {
    var above = figure.y - previous.y - previous.h + ink.top - figure.y;
    var below = next.y - figure.y - figure.h + figure.y + figure.h - 1 - ink.bottom;
    return Math.abs(above - paragraphGap) <= FIGURE_GAP_TOLERANCE_PX && Math.abs(below - paragraphGap) <= FIGURE_GAP_TOLERANCE_PX ? ""
        : "figure gaps " + above + "/" + below + "px, paragraph gap " + paragraphGap + "px";
}

// Read the drawn SVG image, including its live size after asynchronous decoding.
function imageOf(figure) {
    if (figure)
        for (var i = 0; i < figure.children.length; i++) {
            var image = figure.children[i];
            if (image.visible && image.status !== undefined && image.source !== undefined)
                return image;
        }
    return null;
}

// Figure block bounds include the same vertical inset as the fenced figure fallback.
function drawnBlockRect(md, index, target, inset) {
    var block = md.blockItem(index);
    if (!block)
        return null;
    var image = md.blockList[index].type === "figure" ? imageOf(figure(md, index)) : null;
    var item = image;
    if (!item)
        for (var i = 0; i < block.children.length; i++) {
            var child = block.children[i];
            if (child.visible && (child.objectName === "fenceBox" || child.box !== undefined))
                item = child;
        }
    if (!item)
        return null;
    var at = item.mapToItem(target, 0, 0);
    return { x: at.x, y: at.y - (image ? inset : 0), w: item.width,
        h: item.height + (image ? 2 * inset : 0) };
}

function blockGapError(previous, next, gap) {
    if (!previous || !next)
        return "a drawn block is missing";
    var seen = next.y - previous.y - previous.h;
    return Math.abs(seen - gap) <= BLOCK_GAP_TOLERANCE_PX ? ""
        : "drawn block gap " + seen + "px, want " + gap + "px";
}

// Sample input: two stacked ink bands six rows apart answer "" for a want of 6; a missing band names which figure drew nothing.
// The two ranges must be disjoint image rows: padded rects overlap the neighbour's ink and read a negative gap.
function inkGapError(pixels, width, height, upper, lower, want) {
    function rowHasInk(rect, y) {
        if (y < 0 || y >= height)
            return false;
        for (var x = Math.floor(rect.x); x < Math.ceil(rect.x + rect.w); x++) {
            if (x < 0 || x >= width)
                continue;
            var at = (y * width + x) * RGBA_CHANNELS;
            var r = pixels[at], g = pixels[at + 1], b = pixels[at + 2];
            if (isInk(r, g, b))
                return true;
        }
        return false;
    }
    var last = -1;
    for (var uy = Math.ceil(upper.y + upper.h) - 1; uy >= Math.floor(upper.y); uy--) {
        if (rowHasInk(upper, uy)) {
            last = uy;
            break;
        }
    }
    var first = -1;
    for (var ly = Math.floor(lower.y); ly < Math.ceil(lower.y + lower.h); ly++) {
        if (rowHasInk(lower, ly)) {
            first = ly;
            break;
        }
    }
    if (last < 0 || first < 0)
        return "no ink in " + (last < 0 ? "upper" : "lower") + " figure";
    var seen = first - last - 1;
    return Math.abs(seen - want) <= BLOCK_GAP_TOLERANCE_PX ? ""
        : "figure ink gap " + seen + "px, want " + want + "px";
}
