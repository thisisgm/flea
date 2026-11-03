import QtQuick
Item {
    IpcHandler {
        target: "fleapicker"
        function ready(): bool { return true }
    }
}
