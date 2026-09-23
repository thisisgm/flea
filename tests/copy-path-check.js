// Run with node tests/copy-path-check.js; exercise the keyboard route through backend replies.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

const source = fs.readFileSync(path.join(__dirname, '../ui/PaneMenuActions.qml'), 'utf8');
function qmlFunction(name, indent) {
    const match = source.match(new RegExp('^' + ' '.repeat(indent) + 'function ' + name
        + '\\(([^)]*)\\) \\{([\\s\\S]*?)^' + ' '.repeat(indent) + '\\}', 'm'));
    assert.ok(match, name + ' must be present');
    return 'function ' + name + '(' + match[1] + ') {' + match[2] + '\n}';
}

const requests = [];
const copied = [];
const opened = [];
const pane = {
    path: '/fixture', cursorIndex: 2, picked: [7], menuSelectionIdentity: 'listing-1',
    rowFor() { return null; },
    backend: {send(request) { requests.push(request); }},
    contextMenu() { return {opened: false}; },
    performMenu(action, id, paths) { copied.push([action, id, paths[0]]); },
    message(message) { this.said = message; }
};
const actions = {
    pane, requestId: 0, deleting: false, survivorId: 0, opened: false,
    copyingPath: false, afterCopyPath: null,
    finishProviders() {},
    show(action) { opened.push(action); },
    Ops: {
        targetIndices(pane) { return pane.picked.length ? pane.picked : pane.cursorIndex < 0 ? [] : [pane.cursorIndex]; },
        sayNoTarget(pane) { pane.message('There is nothing to act on.'); }
    }
};
actions.root = actions;
const context = vm.createContext(actions);
for (const [name, indent] of [['snapshot', 4], ['copyPath', 4], ['open', 4], ['onMenuResult', 8]])
    vm.runInContext('root.' + name + ' = ' + qmlFunction(name, indent), context);

actions.copyPath();
assert.deepEqual(Array.from(requests[0].rows), [7], 'an offscreen selection is sent by index');
assert.equal(requests[0].cursor, 7, 'the snapshot does not depend on a different cursor row');
actions.onMenuResult({id: 1, op: 'snapshot', ok: true});
assert.equal(requests[1].action, 'copypath', 'a successful snapshot activates Copy path');
actions.onMenuResult({id: 1, op: 'activate', ok: true, action: 'copypath', paths: ['/fixture/picked.txt']});
assert.deepEqual(copied, [['copypath', 1, '/fixture/picked.txt']], 'the backend path reaches the clipboard action');

pane.picked = [];
actions.copyPath();
assert.deepEqual(Array.from(requests[2].rows), [2], 'an unselected listing uses the cursor');
pane.cursorIndex = -1;
actions.copyPath();
assert.equal(pane.said, 'There is nothing to act on.');
assert.equal(requests.length, 3, 'an empty listing makes no backend request');
actions.onMenuResult({id: 2, op: 'snapshot', ok: false, error: 'Selection expired.'});

pane.picked = [7];
actions.copyPath();
pane.menuSelectionIdentity = 'listing-2';
actions.onMenuResult({id: 3, op: 'snapshot', ok: true});
assert.equal(requests.length, 4, 'a changed selection cannot activate the stale snapshot');
assert.equal(copied.length, 1, 'a stale snapshot cannot overwrite the clipboard');

actions.copyPath();
actions.onMenuResult({id: 4, op: 'snapshot', ok: true});
pane.menuSelectionIdentity = 'listing-3';
actions.onMenuResult({id: 4, op: 'activate', ok: true, action: 'copypath', paths: ['/fixture/picked.txt']});
assert.equal(copied.length, 1, 'a changed selection cannot copy after activation either');

const requestsBeforeOpen = requests.length;
actions.copyPath();
const copyId = actions.requestId;
actions.open('properties', 0);
assert.equal(requests.length, requestsBeforeOpen + 1, 'a new action waits for the copy reply');
actions.onMenuResult({id: copyId, op: 'snapshot', ok: true});
assert.equal(actions.requestId, copyId + 1, 'the next action takes its own snapshot');
assert.equal(requests.length, requestsBeforeOpen + 2, 'a superseded copy cannot activate');
actions.onMenuResult({id: copyId + 1, op: 'snapshot', ok: true});
assert.deepEqual(opened, ['properties'], 'the new action opens after its own snapshot');

actions.copyPath();
const nextCopyId = actions.requestId;
actions.open('newFile', 0);
assert.equal(actions.copyingPath, false, 'a new-file dialog also supersedes the copy');
actions.onMenuResult({id: nextCopyId, op: 'snapshot', ok: true});
assert.deepEqual(opened, ['properties', 'newFile'], 'a stale copy reply cannot interrupt the dialog');

pane.picked = [7, 8];
actions.copyPath();
const multiId = actions.requestId;
actions.onMenuResult({id: multiId, op: 'snapshot', ok: true});
actions.onMenuResult({id: multiId, op: 'activate', ok: true, action: 'copypath', paths: ['/fixture/picked.txt']});
actions.open('properties', 0);
assert.deepEqual(Array.from(requests.at(-1).rows), [7, 8], 'a later action snapshots the full selection');

actions.copyPath();
const activatingId = actions.requestId;
actions.onMenuResult({id: activatingId, op: 'snapshot', ok: true});
const beforeActivationReply = requests.length;
actions.open('properties', 0);
assert.equal(requests.length, beforeActivationReply, 'a new action waits for copy activation too');
actions.onMenuResult({id: activatingId, op: 'activate', ok: true, action: 'copypath', paths: ['/fixture/picked.txt']});
assert.equal(requests.length, beforeActivationReply + 1, 'activation reply starts the new snapshot');
assert.equal(copied.length, 2, 'a superseded activation does not overwrite the clipboard');
assert.deepEqual(Array.from(requests.at(-1).rows), [7, 8], 'the new snapshot retains the full selection');
console.log('copy path checks passed');
