#!/usr/bin/env node
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const repo = path.resolve(process.argv[2] || path.join(__dirname, ".."));
const source = name => fs.readFileSync(path.join(repo, "ui", name), "utf8");

function body(name, pattern) {
    const match = source(name).match(pattern);
    assert.ok(match, `${name}: expected QML handler is missing`);
    return match[1];
}

// QML libraries are evaluated unchanged after their namespace imports are resolved.
function library(file) {
    const context = vm.createContext({});
    const text = fs.readFileSync(file, "utf8").replace(/^\.pragma library\s*$/gm, "")
        .replace(/^\.import "([^"]+)" as (\w+)\s*$/gm, (_, imported, name) => {
            context[name] = library(path.resolve(path.dirname(file), imported));
            return "";
        });
    vm.runInContext(text, context, {filename: file});
    return context;
}
const Ops = library(path.join(repo, "ui/js/Ops.js"));
const Errors = library(path.join(repo, "ui/js/Errors.js"));
const Nav = library(path.join(repo, "ui/js/Nav.js"));
const clearEditor = new Function("root",
    body("Pane.qml", /    onRenamingIndexChanged: ([\s\S]*?)\n    \}/) + "\n}");
const failed = new Function("pane", "root", "Errors", "Ops", "where", "input", "message", "mode",
    body("PaneWire.qml", /        function onFailed\(where, input, message, mode\) \{([\s\S]*?)\n        \}/));
const renamed = new Function("pane", "root", "Nav", "ok", "path",
    body("PaneWire.qml", /        function onRenamed\(ok, path\) \{([\s\S]*?)\n        \}/));
const refresh = new Function("pane", "root", "watchSettle", "request", "selected",
    body("PaneWire.qml", /    function refreshRename\(request, selected\) \{([\s\S]*?)\n    \}/));
let checked = 0;
function equal(actual, expected) { assert.deepEqual(actual, expected); checked++; }

function editing() {
    const p = {path: "/fixture/list", cursorIndex: 7, renameRequest: null, renameSource: "", renameError: "",
        renameMenuId: 42, renameKeepsPointerRow: false, listInFlight: false, searchMode: "", listingState: "ready",
        sent: [], messages: [], refreshed: [], setCursor(index) { this.cursorIndex = index; },
        rowFor() { return {n: "before.txt"}; }, join(base, name) { return `${base}/${name}`; },
        message(text, error) { this.messages.push([text, error]); }, sticky() {},
        refresh(selected) { this.refreshed.push(selected); }};
    let index = -1;
    Object.defineProperty(p, "renamingIndex", {get: () => index, set(value) { index = value; clearEditor(p); }});
    Object.defineProperty(p, "renamePending", {get: () => p.renameRequest !== null});
    p.backend = {rename(...args) { p.sent.push(args); }};
    const root = {stale: false, refreshRename(request, selected) { refresh(p, root, {stop() {}}, request, selected); }};
    p.fail = (where, input, message) => failed(p, root, Errors, Ops, where, input, message, 0);
    p.done = name => renamed(p, root, Nav, true, name);
    p.wire = root;
    Ops.startRename(p, 42);
    Ops.commitRename(p, "after.txt");
    return p;
}

let p = editing();
equal(p.sent, [["/fixture/list/before.txt", "after.txt", 42]]);
p.fail("journal", "/fixture/list/before.txt", "file or folder not found");
equal([p.renamePending, p.renamingIndex, p.renameError, p.refreshed.length],
    [false, 7, "File or folder not found.", 0]);
Ops.commitRename(p, "retry.txt");
equal([p.renamePending, p.sent.length, p.renameRequest.destination], [true, 2, "/fixture/list/retry.txt"]);

p = editing();
p.fail("journal", "/fixture/list/after.txt", "permission denied");
equal([p.renamePending, p.renamingIndex, p.refreshed], [false, -1, ["/fixture/list/after.txt"]]);
equal(p.messages, [["Renamed, but Undo was not recorded: permission denied.", true]]);

p = editing();
p.fail("rename-kept", "/fixture/list/before.txt", "permission denied");
equal([p.renamePending, p.renamingIndex, p.refreshed], [false, -1, [""]]);
equal(p.messages, [[Errors.sentence("rename-kept", "permission denied"), true]]);

for (const input of ["", "/fixture/list/after.txt", "/fixture/list/after.txt/child"]) {
    p = editing();
    p.fail("rename", input, "permission denied");
    equal([p.renamePending, p.renamingIndex, p.renameError], [false, 7, "Permission denied."]);
}

for (const where of ["rename", "journal", "rename-kept"]) {
    p = editing();
    p.fail(where, "/fixture/unrelated", "permission denied");
    equal([p.renamePending, p.renamingIndex, p.renameError], [true, 7, ""]);
}

p = editing();
p.done("/fixture/unrelated");
equal([p.renamePending, p.renamingIndex, p.refreshed.length], [true, 7, 0]);
p.done("/fixture/list/after.txt");
equal([p.renamePending, p.renamingIndex, p.refreshed], [false, -1, ["/fixture/list/after.txt"]]);

p = editing();
p.path = "/fixture/another-directory";
p.renamingIndex = -1;
equal([p.renamePending, p.renameSource, p.renameError], [true, "", ""]);
Ops.startRename(p, 99);
equal([p.renamingIndex, p.sent.length], [-1, 1]);
p.done("/fixture/list/after.txt");
equal([p.renamePending, p.renamingIndex, p.refreshed.length], [false, -1, 0]);

for (const where of ["backend", "read"]) {
    p = editing();
    p.fail(where, "", "the backend stopped");
    equal([p.renamePending, p.renamingIndex, p.listingState, p.total], [false, -1, "error", 0]);
    equal(p.messages, [["Backend stopped; rename outcome unknown.", true]]);
}

p = editing();
p.searchMode = "results";
p.done("/fixture/list/after.txt");
equal([p.renamePending, p.refreshed.length, p.wire.stale], [false, 0, true]);
console.log(`rename-lifecycle: ${checked} checks, 0 failed`);
