#!/usr/bin/env node
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const repo = path.resolve(process.argv[2] || path.join(__dirname, ".."));
const source = name => fs.readFileSync(path.join(repo, "ui", name), "utf8");

// The actual QML handlers run against two panes; native tests cover Qt binding and input delivery.
function body(name, pattern) {
    const match = source(name).match(pattern);
    assert.ok(match, `${name}: expected QML handler is missing`);
    return match[1];
}
const reset = new Function("root", "ViewState", body("Backend.qml", /    function resetSort\(\) \{([\s\S]*?)\n    \}/));
const preference = new Function("root", body("Backend.qml", /^    onSortPreferenceChanged: (.*)$/m));
const listed = new Function("pane", "ViewState", "Search", "total",
    body("PaneWire.qml", /        function onListed\(total, readMs, sortMs\) \{([\s\S]*?)\n        \}/));
const changed = new Function("root", "ViewState", "preferences",
    body("Pane.qml", /    onListingPreferencesChanged: (\{[\s\S]*?\n    \}|[^\n]*)/));
const apply = new Function("root", "ViewState",
    body("Pane.qml", /        id: preferences[\s\S]*?        onTriggered: \{([\s\S]*?)\n        \}/));
const open = new Function("root", "ViewState", "Nav", "newPath",
    body("Pane.qml", /    function openWithoutHistory\(newPath\) (\{[^\n]*\}|\{[\s\S]*?\n    \})/));
const navigate = new Function("pane", "newPath", "Thumbs", "DirSizes", "Filter",
    body("js/Nav.js", /function openWithoutHistory\(pane, newPath\) \{([\s\S]*?)\n\}/));
const closeSearch = new Function("root", "RESULTS", "OFF",
    body("js/Search.js", /function close\(root\) \{([\s\S]*?)\n\}/));
const list = new Function("root", "ViewState", "path", "first", "hidden",
    body("Backend.qml", /    function list\(path, first, hidden\) \{([\s\S]*?)\n    \}/));
const searchChanged = new Function("root", "preferences",
    (source("Pane.qml").match(/^    onSearchModeChanged: (.*)$/m) || ["", ""])[1]);
let checked = 0;
function equal(actual, expected) { assert.deepEqual(actual, expected); checked++; }
const view = {state: {view: "dual", hidden: false, sort: {key: "name", reverse: false}, foldersFirst: true, groupByKind: false}};
const signature = () => JSON.stringify([view.state.hidden, view.state.sort, view.state.foldersFirst, view.state.groupByKind]);
function pane(visible = true) {
    const p = {dualMode: true, visible, path: "/fixture/list", viewMode: "list", listOnly: false,
        listInFlight: false, searchMode: "", appliedListingPreferences: signature(), relists: 0,
        cursor: 12, selected: [12], scroll: 240, opened() {}};
    Object.defineProperty(p, "listingPreferences", {get: signature});
    p.backend = {preserveSort: true, hasListed: true, sortBy: "size", sortDesc: true};
    p.backend.resetSort = () => reset(p.backend, view);
    p.openWithoutHistory = next => open(p, view, {openWithoutHistory() {
        p.relists++; p.cursor = 0; p.selected = []; p.scroll = 0; p.appliedListingPreferences = signature();
    }}, next);
    return p;
}

const first = pane();
first.backend.hasListed = false;
preference(first.backend);
equal([first.backend.sortBy, first.backend.sortDesc], ["name", false]);
const retained = pane();
preference(retained.backend);
equal([retained.backend.sortBy, retained.backend.sortDesc], ["size", true]);
retained.backend.preserveSort = false;
preference(retained.backend);
equal([retained.backend.sortBy, retained.backend.sortDesc], ["name", false]);

const saves = [];
view.changeLeaf = (key, value) => saves.push([key, value]);
listed(pane(), view, {RESULTS: "results"}, 80);
equal(saves, []);
const single = pane();
single.dualMode = false;
single.backend.sortBy = "mtime";
listed(single, view, {RESULTS: "results"}, 80);
equal(saves, [["sort", {key: "date", reverse: true}]]);

const hidden = pane(false), live = pane();
view.state.sort = {key: "date", reverse: false};
for (const p of [hidden, live]) {
    preference(p.backend);
    changed(p, view, {restart() {}});
    apply(p, view);
}
equal([hidden.backend.sortBy, hidden.backend.sortDesc, hidden.relists, hidden.cursor, hidden.selected, hidden.scroll],
    ["size", true, 0, 12, [12], 240]);
equal([live.backend.sortBy, live.backend.sortDesc, live.relists], ["mtime", false, 1]);
hidden.visible = true;
apply(hidden, view);
equal([hidden.backend.sortBy, hidden.backend.sortDesc, hidden.relists, hidden.cursor, hidden.selected, hidden.scroll],
    ["size", true, 0, 12, [12], 240]);

hidden.visible = false;
view.state.hidden = true;
changed(hidden, view, {restart() {}});
apply(hidden, view);
equal(hidden.relists, 0);
hidden.visible = true;
apply(hidden, view);
equal([hidden.backend.sortBy, hidden.backend.sortDesc, hidden.relists, hidden.showHidden], ["size", true, 1, true]);

const normal = pane();
normal.dualMode = false;
normal.backend.preserveSort = false;
view.state.view = "list";
view.state.sort = {key: "kind", reverse: false};
preference(normal.backend);
changed(normal, view, {restart() {}});
apply(normal, view);
equal([normal.backend.sortBy, normal.backend.sortDesc, normal.relists], ["kind", false, 1]);
changed({backend: null, appliedListingPreferences: ""}, view, {restart() {}});

// Search exit must consume deferred Settings through the real close, navigation and list handlers.
for (const mode of ["results", "typing"]) {
    view.state.view = "dual";
    view.state.sort = {key: "name", reverse: false};
    const p = pane(), requests = [];
    let searchMode = mode, scheduled = false;
    const timer = {restart() { scheduled = true; }};
    Object.defineProperty(p, "searchMode", {get: () => searchMode, set(value) {
        searchMode = value;
        searchChanged(p, timer);
    }});
    p.searchFrom = mode === "results" ? p.path : "";
    p.listArea = {primeSettle() {}};
    p.clearSelection = () => { p.selected = []; };
    p.backend.listRequests = 0;
    p.backend.askFsInfo = () => {};
    p.backend.send = request => requests.push(request);
    p.backend.list = (...args) => list(p.backend, view, ...args);
    const Nav = {openWithoutHistory(current, path) {
        navigate(current, path, {empty: () => ({})}, {empty: () => ({})}, {close() {}});
    }};
    p.openWithoutHistory = path => open(p, view, Nav, path);
    view.state.sort = {key: "date", reverse: true};
    preference(p.backend);
    changed(p, view, timer);
    apply(p, view);
    scheduled = false;
    equal([p.backend.sortBy, p.backend.sortDesc, requests.length], ["size", true, 0]);
    closeSearch(p, "results", "");
    if (scheduled) apply(p, view);
    equal(requests.map(request => [request.c, request.by, request.desc]), [["list", "mtime", true]]);
    equal(p.appliedListingPreferences, p.listingPreferences);
}
console.log(`dual-sort: ${checked} checks, 0 failed`);
