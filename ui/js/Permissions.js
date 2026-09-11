.pragma library

// Sample input: "644" or "0644"; invalid text remains in the input until corrected.
function parse(text) {
    return /^(0?[0-7]{3})$/.test(String(text)) ? parseInt(text, 8) : -1
}
function octal(value) { return ("0000" + value.toString(8)).slice(-4) }
function toggle(text, bit) {
    var value = parse(text)
    return value < 0 ? text : octal(value ^ bit)
}
