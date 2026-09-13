// Run with node tests/menu-dialog-check.js; exercise the QML reply handler against real asynchronous orderings.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const source = fs.readFileSync(path.join(__dirname, '../ui/MenuActionDialog.qml'), 'utf8');
const receive = source.match(/^    function receive\(message\) \{([\s\S]*?)^    \}/m);
assert.ok(receive, 'the production reply handler must be present');

function dialog() {
    const events = [];
    const state = {opened: false, action: 'deletePermanently', committing: true, busy: true, requestId: 3,
        facts: {count: 2}, checkPending: false, errorText: '', confirmation: {close() { events.push('hide'); }, open() { events.push('show'); }},
        closeFocus: {forceActiveFocus() { events.push('focus-cancel'); }},
        refreshDeletion() { events.push('refresh'); }, checkDeletion() { events.push('check'); },
        deleted(message) { events.push(message); }, finish() { events.push('finish'); }, events};
    Object.defineProperty(state, 'deletionActive', {get() { return this.action === 'deletePermanently' && this.committing; }});
    vm.createContext(state);
    vm.runInContext('function receive(message) {' + receive[1] + '\n}', state);
    return state;
}

const deleting = dialog();
deleting.receive({id: 3, op: 'checkDelete', ok: true, valid: true});
assert.equal(deleting.committing, true, 'an earlier check reply cannot clear the active delete');
deleting.receive({id: 3, op: 'delete', ok: true, deleted: 1, failed: 1, remaining: ['/fixture/survivor']});
assert.equal(deleting.events[0].count, 2);
assert.equal(deleting.events[0].deleted, 1);
assert.equal(deleting.events[1], 'finish');

const stale = dialog();
stale.receive({id: 3, op: 'delete', ok: true, stale: true});
assert.deepEqual(stale.events, ['refresh'], 'stale identities require a fresh confirmation before mutation');

const failed = dialog();
failed.receive({id: 3, op: 'delete', ok: false, error: 'backend stopped'});
assert.equal(failed.opened, true);
assert.equal(failed.committing, false);
assert.equal(failed.errorText, 'backend stopped');
assert.deepEqual(failed.events, ['hide', 'focus-cancel']);

const cancelled = dialog();
cancelled.committing = false;
cancelled.receive({id: 3, op: 'prepareDelete', ok: true, token: 19});
assert.deepEqual(cancelled.events, [], 'closing before preparation finishes cannot reopen the strip');
const changedWhileChecking = dialog();
changedWhileChecking.opened = true;
changedWhileChecking.committing = false;
changedWhileChecking.checkPending = true;
changedWhileChecking.receive({id: 3, op: 'checkDelete', ok: true, valid: true});
assert.deepEqual(changedWhileChecking.events, ['refresh'], 'a hidden stale strip must be replaced after a pending filesystem change');
const newer = dialog();
newer.receive({id: 2, op: 'delete', ok: false, error: 'old operation'});
assert.equal(newer.committing, true);
assert.deepEqual(newer.events, []);
const menuSource = fs.readFileSync(path.join(__dirname, '../ui/PaneMenuActions.qml'), 'utf8');
const activate = menuSource.match(/^    function activate\(action, selected\) \{([\s\S]*?)^    \}/m);
assert.ok(activate, 'the production menu activation gate must be present');
let requested = 0;
const activation = vm.createContext({pane: {menuSelectionIdentity: 'current'}, identity: 'current', requestId: 3,
    ready: true, activationUsed: false, deleting: false, survivorId: 0, validateActivation() { requested++; }});
vm.runInContext('function activate(action, selected) {' + activate[1] + '\n}', activation);
activation.activate('duplicate', true);
activation.activate('duplicate', true);
assert.equal(requested, 1, 'repeated activation consumes one menu snapshot only once');
assert.equal(activation.activationUsed, true);
console.log('menu dialog: 17 checks, 0 failed');

const stepFocus = source.match(/^    function stepFocus\(back\) \{([\s\S]*?)^    \}/m);
assert.ok(stepFocus, 'the production focus handler must be present');
let focusChecks = 1;
for (const name of ['field', 'appList', 'closeFocus', 'submitFocus']) {
    const start = source.indexOf('id: ' + name);
    const nextId = source.indexOf('id: ', start + 4);
    const control = source.slice(start, nextId < 0 ? source.length : nextId);
    assert.ok(control.includes('Keys.onTabPressed: root.stepFocus(false)')
        && control.includes('Keys.onBacktabPressed: root.stepFocus(true)'), name + ' must intercept Qt traversal');
    focusChecks++;
}
const controls = {};
for (const name of ['field', 'appList', 'closeFocus', 'submitFocus']) {
    controls[name] = {visible: false, enabled: true, activeFocusOnTab: true, activeFocus: false,
        forceActiveFocus() {
            for (const item of Object.values(controls)) item.activeFocus = false;
            this.activeFocus = true;
        }};
}
vm.createContext(controls);
vm.runInContext('function stepFocus(back) {' + stepFocus[1] + '\n}', controls);
function focused(expected, label) {
    assert.equal(Object.keys(controls).find(name => controls[name].activeFocus), expected, label);
    focusChecks++;
}
controls.closeFocus.visible = true;
controls.stepFocus(false);
focused('closeFocus', 'Properties enters its only visible control');
controls.stepFocus(false);
focused('closeFocus', 'Properties Tab wraps within Close');
controls.stepFocus(true);
focused('closeFocus', 'Properties Shift+Tab wraps within Close');
controls.field.visible = true;
controls.submitFocus.visible = true;
controls.stepFocus(false);
focused('submitFocus', 'an available input action follows Cancel');
controls.stepFocus(false);
focused('field', 'input action wraps to its field');
controls.field.enabled = false;
controls.submitFocus.activeFocusOnTab = false;
controls.stepFocus(true);
focused('closeFocus', 'busy or unavailable controls cannot receive focus');
controls.closeFocus.activeFocus = false;
controls.appList.visible = true;
controls.stepFocus(true);
focused('closeFocus', 'reverse entry without a current target reaches the last available control');
controls.stepFocus(true);
focused('appList', 'Open With reverses into the available application list');
console.log('menu focus: ' + focusChecks + ' checks, 0 failed');
