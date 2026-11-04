// Regenerates captions.golden from the frozen ui/js/GridNames.js: node crates/flea-gui/tests/g1_08/captions_golden.js > crates/flea-gui/tests/g1_08/captions.golden
"use strict";
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const crypto = require("crypto");

const root = path.join(__dirname, "../../../..");
function load(name, imports) {
    const file = path.join(root, "ui/js", name);
    const source = fs.readFileSync(file, "utf8").split("\n").filter((line) => !line.startsWith(".")).join("\n");
    const context = vm.createContext(Object.assign({}, imports));
    vm.runInContext(source, context, { filename: file });
    return context;
}
const Format = load("Format.js", {});
const Match = load("Match.js", {});
const GridNames = load("GridNames.js", { Format, Match });

// The seeds tests/js/names.js stretches, so the corpus covers wide glyphs, pairs, marks, separators, dotfiles and extensions.
const seeds = [
    "screenshot-2026-08-30-final-review-for-gm-after-the-bench-v3.png",
    "a-very-long-filename-with-no-extension-at-all",
    ".bashrc-hidden-config-name",
    ".profile.json",
    "name.",
    "data." + "y".repeat(48),
    "a b-c_d.e f",
    "中写漢字テスト한글αβ",
    "🎉📷🚀🎵",
    "a中🎉-_.Z",
    "éclair-resumé",
    "café-naïve-klinik",
    "x",
];
const lens = [0, 1, 2, 3, 5, 8, 13, 15, 16, 17, 20, 31, 33, 48, 64];
const pers = [1, 2, 7, 16, 40];
const counts = [1, 2, 3];

// One seed stretched to whole code points, so every name is valid UTF-8 for the Rust side.
function stretched(seed, len) {
    let out = Array.from(seed);
    while (out.length < len) out = out.concat(out);
    return out.slice(0, len).join("");
}

const hash = crypto.createHash("sha256").update(fs.readFileSync(path.join(root, "ui/js/GridNames.js"))).digest("hex");
const lines = ["# GridNames.js sha256 " + hash];
for (const seed of seeds) {
    for (const len of lens) {
        const name = stretched(seed, len);
        for (const per of pers)
            for (const count of counts)
                lines.push(JSON.stringify(GridNames.gridCaption(name, per, count)));
        lines.push(String(GridNames.lastLineCells("ab\n" + name)));
    }
}
process.stdout.write(lines.join("\n") + "\n");
