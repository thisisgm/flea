// Run with node tests/window-rect-check.js; native resize and drag tests exercise the same observer.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(require('node:path').join(__dirname, '../ui/shell.qml'), 'utf8');
const handler = source.match(/^        function rectOf\(item\) \{[^]*?^        \}/m);
assert.ok(handler, 'production rectangle observer exists');
const context = vm.createContext({fleaWindow: {itemRect: item => item}});
vm.runInContext(handler[0], context);
assert.equal(context.rectOf(null), '');
assert.equal(context.rectOf({x: 1364.5, y: 69, width: 1171.5, height: 1294}), '1365 69 1171 1294');
assert.equal(context.rectOf({x: 0.5, y: 0.5, width: 879.5, height: 619.5}), '1 1 879 619');
assert.equal(context.rectOf({x: 192, y: 81, width: 344, height: 512}), '192 81 344 512');
console.log('Window rectangle: 4 checks, 0 failed');
