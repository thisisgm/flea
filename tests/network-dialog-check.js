// Run with node tests/network-dialog-check.js; native coverage lives in tests/ui.sh network.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const dialogSource = fs.readFileSync(path.join(__dirname, '../ui/NetworkDialog.qml'), 'utf8');
const serviceSource = fs.readFileSync(path.join(__dirname, '../ui/NetworkMounts.qml'), 'utf8');
let checks = 0;
function check(actual, expected, message) {
    checks++;
    assert.deepEqual(actual, expected, message);
}
function install(context, source, names, indent = '    ') {
    for (const name of names) {
        const match = source.match(new RegExp('^' + indent + 'function ' + name + '\\([^]*?^' + indent + '}', 'm'));
        assert.ok(match, 'production handler exists: ' + name);
        vm.runInContext(match[0].trim(), context);
        context.root[name] = context[name];
    }
}
function dialog() {
    const events = [];
    const form = {complete: true, spec: {credentials: true}, user: '', password: 'fixture-password',
        uri: 'smb://fixture.test/share', labelText: () => 'Fixture', focusHost() {},
        takePassword() { const password = this.password; this.password = ''; return password; }};
    const root = {opened: true, connecting: false, saving: false, saveNewPlace: true, requestSerial: 0,
        requestId: '', mountedUri: '', pendingUri: '', pendingLabel: '', statusText: '', retrying: false,
        mountRequested(...args) { events.push(['mount', ...args]); },
        cancelRequested(id) { events.push(['cancel', id]); }, closed() { events.push(['closed']); }};
    Object.defineProperty(root, 'busy', {get() { return this.connecting || this.saving; }});
    const context = vm.createContext({root, form, Mounts: {normalize: uri => uri},
        pendingFocus: {forceActiveFocus() {}}, Favourites: {add(...args) { events.push(['save', ...args]); return true; }}});
    install(context, dialogSource, ['close', 'submitLocation', 'mountFinished', 'saveFavourite', 'saveFailed']);
    install(context, dialogSource, ['onCompleted'], '        ');
    return {root, form, context, events};
}

const first = dialog();
first.root.submitLocation();
first.root.submitLocation();
check(first.events.map(event => event[0]), ['mount'], 'mount starts once and no favourite is saved first');
check(first.form.password, 'fixture-password', 'mount request leaves the draft password in the form');
first.root.mountFinished('old-request', first.form.uri, true, '');
first.root.mountFinished(first.root.requestId, 'smb://other/share', true, '');
check(first.events.length, 1, 'stale request and wrong endpoint completions cannot save');
first.root.mountFinished(first.root.requestId, first.form.uri, false, 'Connect failed: refused');
check([first.root.opened, first.root.saving, first.root.retrying], [true, false, true], 'failed mount retains a retryable form');
check(first.events.length, 1, 'failed mount creates no favourite');
check(first.form.password, 'fixture-password', 'failed mount retains its draft password');

const draft = dialog();
draft.form.load = () => draft.events.push(['overwritten']);
draft.form.reset = () => draft.events.push(['reset']);
draft.root.valuesFor = (uri, label, password) => ({uri, label, password});
draft.root.present = () => { draft.root.opened = true; };
install(draft.context, dialogSource, ['open', 'openLocation']);
draft.root.openLocation('smb://older/share', 'Old request', '', 'Old failure', true);
draft.root.open();
check(draft.events, [], 'late retry and repeated open cannot overwrite the current editable draft');
check(draft.root.saveNewPlace, true, 'late retry cannot turn a new draft into an existing location');
draft.root.opened = false;
draft.root.openLocation('smb://older/share', 'Old request', '', 'Old failure', true);
check([draft.root.opened, draft.root.saveNewPlace, draft.root.statusText], [true, false, 'Old failure'], 'a retry can present when no draft owns the dialog');

const pendingKeys = dialogSource.slice(dialogSource.indexOf('id: pendingFocus'), dialogSource.indexOf('// Read back'));
for (const key of ['Tab', 'Backtab']) {
    const handler = pendingKeys.match(new RegExp('Keys.on' + key + 'Pressed: function\\(event\\) \\{([^}]+)\\}'));
    assert.ok(handler, 'pending focus intercepts ' + key);
    const event = {accepted: false};
    vm.runInNewContext('(function(event) {' + handler[1] + '})(event)', {event});
    check(event.accepted, true, key + ' stays inside the pending modal');
}

const saving = dialog();
saving.root.submitLocation();
saving.root.mountFinished(saving.root.requestId, saving.form.uri, true, '');
check(saving.events.map(event => event[0]), ['mount', 'save'], 'successful mount starts the identified favourite write');
saving.root.onCompleted('other-writer', false, 'Unrelated inspection failure');
check(saving.root.saving, true, 'unrelated writer results cannot complete this save');
saving.root.onCompleted(saving.root.requestId, false, 'Store could not be written');
check([saving.root.opened, saving.root.statusText, saving.form.password],
    [true, 'Store could not be written', 'fixture-password'], 'save failure retains form, error and password');
saving.root.submitLocation();
check(saving.events.map(event => event[0]), ['mount', 'save', 'save'], 'persistence retry does not remount');
saving.root.close();
check(saving.root.opened, true, 'an accepted persistence commit is not presented as a cancellable draft');
saving.root.onCompleted(saving.root.requestId, true, '');
check([saving.root.opened, saving.form.password], [false, ''], 'successful persistence closes and clears the draft password');
saving.root.onCompleted(saving.root.requestId, true, '');
check(saving.events.filter(event => event[0] === 'closed').length, 1, 'duplicate writer completion does not close twice');

const occupied = dialog();
occupied.context.Favourites.add = () => false;
occupied.root.submitLocation();
occupied.root.mountFinished(occupied.root.requestId, occupied.form.uri, true, '');
check([occupied.root.opened, occupied.root.saving, occupied.root.retrying], [true, false, true], 'busy writer refusal leaves an actionable draft');
check(occupied.form.password, 'fixture-password', 'busy writer refusal retains credentials');
occupied.form.uri = 'smb://changed.test/share';
occupied.root.submitLocation();
check(occupied.events.filter(event => event[0] === 'mount').length, 2, 'editing the endpoint after refusal requires its own mount');

const cancelled = dialog();
cancelled.root.submitLocation();
const cancelledId = cancelled.root.requestId;
cancelled.root.close();
cancelled.root.mountFinished(cancelledId, cancelled.form.uri, true, '');
check(cancelled.events.map(event => event[0]), ['mount', 'cancel', 'closed'], 'cancelled completion cannot create a favourite or reopen the form');
check(cancelled.form.password, '', 'Cancel discards the draft password');

const committed = dialog();
committed.root.submitLocation();
committed.root.mountFinished(committed.root.requestId, committed.form.uri, true, '');
committed.root.onCompleted(committed.root.requestId, true, 'Favourites were saved, but their new state could not be read.');
check([committed.root.opened, committed.root.saveCommitted], [true, true], 'committed response error remains visible without offering another write');
committed.root.submitLocation();
check(committed.events.map(event => event[0]), ['mount', 'save', 'closed'], 'acknowledging unreadable committed state never adds a duplicate');

const existing = dialog();
existing.root.saveNewPlace = false;
existing.root.submitLocation();
existing.root.mountFinished(existing.root.requestId, existing.form.uri, true, '');
check(existing.events.map(event => event[0]), ['mount', 'closed'], 'retrying an existing location never imports or duplicates a favourite');

const refused = dialog();
refused.form.complete = false;
refused.root.submitLocation();
check([refused.events.length, refused.root.statusText], [0, 'Enter a valid host and port.'], 'invalid host or port never starts a mount');
refused.form.complete = true;
refused.form.user = 'fixture-user';
refused.form.password = '';
refused.root.submitLocation();
check(refused.events.length, 0, 'credentialed form without password never starts a mount');

const serviceEvents = [];
const service = {_requestId: 'network-3', _requestPassword: 'fixture-password', _pendingUri: 'smb://fixture/share',
    remember(...args) { serviceEvents.push(['remember', ...args]); },
    completed(...args) { serviceEvents.push(['completed', ...args]); }};
const serviceContext = vm.createContext({root: service});
install(serviceContext, serviceSource, ['finishRequest']);
check(service.finishRequest(true, ''), true, 'an owned successful mount produces completion');
check(service.finishRequest(true, ''), false, 'the service consumes each completion identity once');
check(serviceEvents.map(event => event[0]), ['remember', 'completed'], 'credentials enter the session cache only after successful mounting');
check(service._requestPassword, '', 'completion clears the transient password');

const panePaths = [];
const paneA = {open(value) { panePaths.push(['A', value]); }};
const paneB = {open(value) { panePaths.push(['B', value]); }};
const originService = {origin: paneA, result: 'idle', message() {}, pollMounts() {}, remember() {},
    completed() {}, passwordFor: () => '', credentialed: () => false,
    opened(value, origin) { paneContext.onNetworkOpened(value, origin); },
    runInfo() { originContext.infoProcess.running = true; }};
const originContext = vm.createContext({root: originService, Mounts: {normalize: uri => uri, localPath: () => '/fixture/mounted'},
    mountProcess: {running: false}, authProcess: {running: false}, infoProcess: {running: false},
    listSharesProcess: {running: false}, mountTimeout: {stop() {}, restart() {}}, infoOut: {text: 'local path: /fixture/mounted'}});
install(originContext, serviceSource, ['openShare', 'openChildShare', 'finishRequest']);
const paneSource = fs.readFileSync(path.join(__dirname, '../ui/Pane.qml'), 'utf8');
const paneHandler = paneSource.match(/onNetworkOpened: function\(path, origin\) \{([^}]+)}/);
assert.ok(paneHandler, 'the production pane receiver preserves network origin');
const paneContext = vm.createContext({root: {railPane: paneB}});
vm.runInContext('function onNetworkOpened(path, origin) {' + paneHandler[1] + '}', paneContext);
originService.openShare('smb://fixture/root', true, 'Root');
check(originService._pendingOrigin, paneA, 'direct rail activation captures its pane');
originService.origin = paneB;
originService.openShare('smb://other/root', true, 'Other');
check(originService._pendingOrigin, paneA, 'a refused overlapping request cannot steal the in-flight origin');
const infoExit = serviceSource.slice(serviceSource.indexOf('id: infoProcess')).match(/onExited: function \(exitCode\) \{([^]*?)^        }/m);
assert.ok(infoExit, 'the production mount resolver handler exists');
vm.runInContext('function infoExited(exitCode) {' + infoExit[1] + '\n}', originContext);
originContext.infoExited(0);
check(panePaths, [['A', '/fixture/mounted']], 'mount completion navigates A after focus moved to B');
originContext.infoProcess.running = false;
originService.openChildShare('smb://fixture/child', 'Child', paneA);
check(originService._pendingOrigin, paneA, 'child-share activation retains the share browser owner');

const writerSource = fs.readFileSync(path.join(__dirname, '../ui/Favourites.qml'), 'utf8');
const writerExit = writerSource.slice(writerSource.indexOf('id: writer')).match(/onExited: function \(code\) \{([^]*?)^        }/m);
assert.ok(writerExit, 'production favourites writer exit handler exists');
const writerEvents = [];
const writerContext = vm.createContext({
    root: {finish() {}, failed() {}, wrote() {}, completed(...args) { writerEvents.push(args); }},
    writer: {pending: true, requestId: 'network-8', answer: '{broken', errorText: ''},
    ViewState: {refreshFavourites: () => true}
});
vm.runInContext('function exited(code) {' + writerExit[1] + '\n}', writerContext);
writerContext.exited(0);
check(writerEvents[0], ['network-8', true, 'Favorites were saved, but their new state could not be read.'],
    'exit-zero response parse failure reports a committed operation');
writerContext.writer.errorText = 'flea: State write refused';
writerContext.exited(2);
check(writerEvents[1], ['network-8', false, 'State write refused'], 'failed CLI write remains retryable');
writerContext.writer.answer = '{"places":{"favourites":[{"label":"Fixture","path":"/fixture"}]}}';
writerContext.exited(0);
check(writerEvents[2], ['network-8', true, ''], 'successful writer response reports completion after updating state');
writerContext.ViewState.refreshFavourites = () => false;
writerContext.exited(0);
check(writerEvents[3], ['network-8', true, 'Favorites were saved, but their new state could not be read.'],
    'failed post-commit read cannot replay the saved operation');

console.log('network dialog: ' + checks + ' checks, 0 failed');
