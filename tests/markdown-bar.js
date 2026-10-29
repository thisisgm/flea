.import "markdown-mark.js" as Mark

// Every text Quick Look bar draws the language mark, name, muted count, text actions and close.
function barError(g, tokens) {
    if (!g)
        return "the pane exposes no bar geometry";
    if (g.segment || g.rendered || g.source)
        return "the bar exposes a retired view control";
    var drawn = Mark.markError(g, tokens);
    if (drawn)
        return drawn;
    if (g.mark.width !== tokens.chromeMark || g.mark.x !== tokens.padX)
        return "the mark is " + g.mark.width + " px at " + g.mark.x + ", want " + tokens.chromeMark + " at " + tokens.padX;
    if (g.height !== tokens.chromeHeight)
        return "the bar is " + g.height + " px tall, want " + tokens.chromeHeight;
    if (!g.ready)
        return "the count never became ready";
    if (!(g.mark.x < g.name.x && g.name.x < g.lines.x && g.lines.x < g.close.x))
        return "the bar does not read mark, name, count, close from left to right";
    if (g.name.x - (g.mark.x + g.mark.width) !== tokens.gap)
        return "the name does not start one gap after the mark";
    if (g.lines.x - g.nameEnd !== tokens.gap)
        return "the count does not start one gap after the name";
    if (g.lines.x + g.lines.implicitWidth > g.close.x - tokens.gap)
        return "the count overlaps the close mark";
    if (g.name.font.pixelSize !== tokens.caption || g.lines.font.pixelSize !== tokens.caption)
        return "the name and count do not use the caption size";
    if (String(g.name.color) !== tokens.foreground || String(g.lines.color) !== tokens.muted)
        return "the name or count uses the wrong ink";
    return pressError(g.bar, g.close, g.select, g.copy);
}

// Every press target belongs to a declared text action or close, including hidden handlers.
function pressError(bar, close, select, copy) {
    var stack = [bar], strays = [];
    while (stack.length) {
        var item = stack.pop();
        var owned = item.data || [];
        for (var i = 0; i < owned.length; i++) {
            if (owned[i].acceptedButtons !== undefined && !isWithin(item, close) && !isWithin(item, select) && !isWithin(item, copy))
                strays.push(String(item));
            if (owned[i].data !== undefined)
                stack.push(owned[i]);
        }
    }
    return strays.length ? "the bar holds a press target outside the text actions and close: " + strays.join(", ") : "";
}

function isWithin(item, ancestor) {
    for (var node = item; node; node = node.parent)
        if (node === ancestor)
            return true;
    return false;
}

// Tab belongs to close alone; no other bar child takes focus or a Tab stop.
function tabWalkError(g) {
    var stack = [g.bar];
    while (stack.length) {
        var item = stack.pop();
        if (!isWithin(item, g.close) && (item.activeFocus || item.activeFocusOnTab || item.focus || item.tabStop))
            return "a bar item besides close takes the keyboard: " + item;
        for (var i = 0; i < item.children.length; i++)
            stack.push(item.children[i]);
    }
    return "";
}

// A long name takes the freed room and leaves the count one gap before close.
function nameRoomError(g, tokens) {
    if (!g || !(g.name.width < g.name.implicitWidth))
        return "the long name never elided";
    if (!g.countShown)
        return "the count is hidden while its room is measured";
    var rightGap = (g.trail !== undefined ? g.trail : g.close.x) - (g.lines.x + g.lines.implicitWidth);
    return rightGap === tokens.gap ? "" : "the count ends " + rightGap + " px before the controls, want " + tokens.gap;
}

function plainBarError(g, tokens) {
    if (!g || g.segment || g.rendered || g.source)
        return "the plain bar exposes a retired view control";
    var drawn = Mark.markError(g, tokens);
    if (drawn)
        return drawn;
    if (!g.countShown)
        return "the plain bar hides its count";
    if (!(g.mark.x < g.name.x && g.name.x < g.lines.x && g.lines.x < g.close.x))
        return "the bar does not read mark, name, count, close from left to right";
    return nameRoomError(g, tokens) || pressError(g.bar, g.close, g.select, g.copy);
}

// Checker controls reject hidden counts and wrong order using the same live geometry as the positive check.
function plainBarChecks(g, longName, tokens) {
    var error = plainBarError(g, tokens);
    if (error) return error;
    if (!plainBarError(Object.assign({}, g, { countShown: false }), tokens))
        return "the plain bar checker accepted a hidden count";
    if (!nameRoomError(Object.assign({}, longName, { countShown: false }), tokens))
        return "the name-room checker measured a hidden count";
    var wrongOrder = Object.assign({}, g, { name: Object.assign({}, g.name, { x: -1 }) });
    return plainBarError(wrongOrder, tokens) ? "" : "the plain bar checker accepted a name before its mark";
}

function fixedSizes(g) {
    return { close: g.close.width, height: g.height, right: g.bar.width - g.close.x,
        countWidth: g.lines.implicitWidth, nameX: g.name.x };
}

function hidingWidth(was, tokens) {
    return was.nameX + 2 * tokens.gap + was.countWidth + was.right - Math.ceil(was.countWidth / 2) - 1;
}

// Narrowing keeps close fixed; the name gives way first, then the count hides.
function squeezeError(was, g, tokens, countHides) {
    var now = fixedSizes(g);
    var keys = ["close", "height", "right"];
    for (var i = 0; i < keys.length; i++)
        if (now[keys[i]] !== was[keys[i]])
            return "the " + keys[i] + " changed when the bar narrowed";
    if (g.close.x < g.name.x + g.name.width + tokens.gap)
        return "the close mark overlaps the name";
    if (g.countShown === countHides)
        return "the count has the wrong visibility at " + g.bar.width + " px";
    if (!countHides && g.lines.x + g.lines.implicitWidth > g.close.x - tokens.gap)
        return "the count overlaps close when the bar narrows";
    return "";
}
