.pragma library

// Filesystem signatures handled by the encrypted-container unlock path.
function isContainer(fstype) {
    var fs = String(fstype || "").toLowerCase()
    return fs.indexOf("crypto_") === 0 || fs === "bitlocker"
}
