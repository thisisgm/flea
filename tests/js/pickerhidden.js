.import "../../ui/js/Picker.js" as Picker
.import "../../ui/js/Keymap.js" as Keymap
.import "sourcefixture.js" as Source
.import "pickerkeys.js" as PickerKeys

// Run the production visibility initialization and key handlers, recording each listing request.
function windowFor(text, state, parent) {
    var source = Source.source("ui/PickerWindow.qml")
    // Sample input: property bool showHidden: false
    var declaration = source.match(/property bool showHidden:[^\n]+/)
    if (!declaration) throw new Error("PickerWindow.qml: missing showHidden declaration")
    var completed = Source.block(source, "Component.onCompleted:", "ui/PickerWindow.qml")
    var wrapper = Qt.createQmlObject("import QtQuick\nimport "
        + JSON.stringify(Qt.resolvedUrl("../../ui/js/Picker.js").toString()) + " as Picker\n"
        + "Item { property Component made: Component { Item {\n"
        + "id: win\nproperty var viewState\nproperty var req\nproperty string home: '/fixture'\n"
        + "property string viewMode: 'list'\nproperty string saveName: ''\n"
        + "property bool filterTyping: false\nproperty string filterQuery: ''\n"
        + "property var listings: []\nproperty string path: ''\n"
        + declaration[0].replace(/ViewState/g, "win.viewState") + "\n"
        + "function openWithoutHistory(path) { win.path = path; win.listings = win.listings.concat([win.showHidden]) }\n"
        + "function focusDefault() {}\nComponent.onCompleted: {\n"
        + completed.replace(/ViewState/g, "win.viewState") + "\n}\n} } }", parent,
        Qt.resolvedUrl("picker-hidden-window.qml"))
    return wrapper.made.createObject(parent, { req: Picker.request(text), viewState: state })
}

function run(check) {
    var parent = Qt.createComponent("QtQuick", "Item").createObject(null)
    var state = Qt.createQmlObject("import QtQuick\nQtObject {\n"
        + "property var state: ({})\nproperty string pickerView: 'list'\nproperty int writes: 0\n"
        + "function changeKey(key, value) { writes++ }\nfunction changeLeaf(key, value) { writes++ }\n}", parent)
    var handler = Source.slice(Source.source("ui/PickerList.qml"), "Keys.onPressed:",
        "// The listing is a window").replace(/^Keys.onPressed:\s*/, "").trim()
    var gridSource = Source.source("ui/PickerGrid.qml")
    var dispatch = Source.slice(gridSource, "function handleAction(action, key, modifiers)",
        "Keys.onPressed:").trim()
    var gridHandler = Source.slice(gridSource, "Keys.onPressed:", "// A scroll refetches")
        .replace(/^Keys.onPressed:\s*/, "").trim()
    var inputs = ["{}", '{"hidden":true}', '{"hidden":false}', '{"hidden":"true"}',
        '{"hidden":"false"}', '{"hidden":1}', '{"hidden":0}', '{"hidden":null}',
        '{"hidden":[]}', '{"hidden":{}}', "not json", "null"]
    var before = Keymap.preset
    try {
        Keymap.setPreset("default")
        for (var saved of [false, true]) for (var text of inputs) for (var view of ["list", "grid"]) {
            state.state = { hidden: saved }
            var parsed = Picker.request(text)
            var explicit = text === '{"hidden":true}' || text === '{"hidden":false}'
            check("hidden parser preserves optional boolean " + text, parsed.hidden,
                explicit ? text === '{"hidden":true}' : undefined)
            var expected = explicit ? text === '{"hidden":true}' : saved
            var win = windowFor(text, state, parent)
            var label = view + " saved=" + saved + " request=" + text
            check(label + " seeds first listing", JSON.stringify(win.listings), JSON.stringify([expected]))
            check(label + " opens with resolved visibility", win.showHidden, expected)
            var root = { picker: win, firstArmed: false }
            var press = view === "list"
                ? new Function("root", "Keymap", "Qt", "Sort", "Picker", "return (" + handler + ")")(root, Keymap, Qt, {}, Picker)
                : PickerKeys.gridPress(dispatch, gridHandler, root)
            var event = { key: Qt.Key_Period, text: ".", modifiers: Qt.NoModifier, accepted: false }
            press(event)
            check(label + " hidden key is handled", event.accepted, true)
            check(label + " toggle changes request", win.showHidden, !expected)
            check(label + " toggle relists with local value", JSON.stringify(win.listings), JSON.stringify([expected, !expected]))
            check(label + " toggle preserves saved preference", state.state.hidden, saved)
            check(label + " toggle writes no preference", state.writes, 0)
            var next = windowFor("{}", state, parent)
            check(label + " next request ignores prior override and toggle", next.showHidden, saved)
            state.state = { hidden: !saved }
            check(label + " active request keeps its local toggle", win.showHidden, !expected)
            var fresh = windowFor("{}", state, parent)
            check(label + " next request freshly reads changed preference", fresh.showHidden, !saved)
        }
    } finally {
        Keymap.setPreset(before)
        parent.destroy()
    }
}
