import QtQuick
QtObject {
    function helper() {}
    property IpcHandler seam: IpcHandler {
        target: "flea"
        // function commented() {}
        function ready(): bool { return "}" !== "{" }
        function previewState(): string {
            function nested() { return "not an endpoint" }
            return nested()
        }
    }
}
