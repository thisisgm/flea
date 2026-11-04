.pragma library

.import "FigureDate.js" as FigureDate

// One lazy query naming the cache keys the figures' asks will use, shared by the preview and its suite.
// Sample input: [{type:"figure",kind:"math",source:"x^2"}] with a math theme answers one key and ["math"].
function query(blocks, themes, keyOf) {
    var keys = [];
    var kinds = [];
    for (var i = 0; i < blocks.length; i++) {
        if (blocks[i].type !== "figure")
            continue;
        var theme = FigureDate.datedTheme(blocks[i].kind, blocks[i].source, themes[blocks[i].kind]);
        // A figure with no theme has no key and so is never known.
        keys.push(theme ? keyOf(blocks[i].kind, blocks[i].source, theme) : "");
        if (kinds.indexOf(blocks[i].kind) < 0)
            kinds.push(blocks[i].kind);
    }
    return { keys: keys, kinds: kinds };
}
