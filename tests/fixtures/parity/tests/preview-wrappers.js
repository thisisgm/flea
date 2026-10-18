function record(ok, label) { check(label, ok) }
var deferred = function(label) { queue(function () { record(false, label) }) }
const indirect = (label) => deferred(label)
const block = label => { indirect(label) }
const pureArrow = label => label
const multiline = label => label
    ? block(label)
    : indirect(label)
var methods = {
    pass(label) { check(label, true) },
    forward(label) { this.pass(label) }
}
function first(label) { second(label) }
function second(label) { block(label) }
first("top-level JS transitive case")
deferred("top-level JS deferred case")
methods.forward("method wrapper case")
methods.pass("method assertion case")
multiline("multiline arrow wrapper")
