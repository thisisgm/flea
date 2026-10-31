//@ pragma ShellId flea-mediasession-test
import QtQuick
import Quickshell
import Quickshell.Io
import "flea" as Flea

// tests/mediasession.sh's harness: the real ui/PreviewMedia.qml and Quick Look over the real MediaSession each preview holds, with a stub in place of tools/flea-mpris that logs what it is fed.
ShellRoot {
    id: shell

    property int checks: 0
    property var failures: []
    property int ticks: 0
    property int step: 0
    property bool done: false
    // The condition the next step waits for, and how many ticks it may take.
    property var until: null
    property string untilName: ""
    property int budget: 0
    property var media: null
    property var look: null
    property int baseline: 0
    property int positionTicks: 0

    readonly property string dir: Quickshell.env("STUB_DIR")
    readonly property string clip: Quickshell.env("MEDIASESSION_MEDIA") + "/clip.mp4"
    readonly property string tone: Quickshell.env("MEDIASESSION_MEDIA") + "/tone.ogg"
    readonly property string nextClip: Quickshell.env("MEDIASESSION_MEDIA") + "/next.mp4"

    function log(line) { console.log("MEDIASESSION " + line) }
    function quit() { Quickshell.execDetached(["kill", String(Quickshell.processId)]) }
    function check(name, actual, expected) {
        shell.checks++
        if (JSON.stringify(actual) === JSON.stringify(expected)) return
        shell.failures.push(name)
        shell.log("FAIL " + name + ": got " + JSON.stringify(actual) + ", expected " + JSON.stringify(expected))
    }

    FileView { id: reader; printErrors: false; blockLoading: true }
    // The non-empty lines of one stub log, or none while it does not exist yet.
    function lines(name) {
        reader.path = shell.dir + "/" + name
        reader.reload()
        reader.waitForJob()
        return String(reader.text()).split("\n").filter(function (line) { return line.length > 0 })
    }
    // Whether the nth stub started (1 first) has ended: its pid is no longer a process.
    function stubGone(n) {
        reader.path = "/proc/" + shell.lines("starts.log")[n - 1] + "/stat"
        reader.reload()
        reader.waitForJob()
        return String(reader.text()).length === 0
    }
    function states() { return shell.lines("lines.log").map(function (line) { return JSON.parse(line) }) }
    function command(word) { Quickshell.execDetached(["sh", "-c", "printf '%s\\n' \"$1\" >> \"$2\"", "sh", word, shell.dir + "/cmds.log"]) }
    function waitFor(name, predicate) { shell.untilName = name; shell.until = predicate; shell.budget = 150 }

    FloatingWindow {
        id: win
        implicitWidth: 400
        implicitHeight: 300
        color: "#303030"
        // What ui/WindowBody.qml is to the real player: the ancestor that holds the window as host, and Quick Look's Preview sits under it as it does there.
        Item {
            id: holder
            anchors.fill: parent
            property var host: win
            Flea.Preview {
                id: quick
                anchors.fill: parent
            }
        }
    }

    Component {
        id: mediaComponent
        Flea.PreviewMedia { anchors.fill: parent; autoStart: false }
    }

    function make(path, kind) {
        var item = mediaComponent.createObject(holder, { path: path, kind: kind })
        item.positionChanged.connect(function () { shell.positionTicks++ })
        return item
    }

    // The preview column's player does not start on its own: one handed a second clip has to load it and stay stopped.
    function stepIdleFollow() {
        shell.media = shell.make(shell.clip, "video")
        shell.waitFor("the idle player loads the clip", function () { return shell.media.poster })
    }

    function stepIdleFollowLoad() {
        shell.media.path = shell.nextClip
        shell.waitFor("the idle player loads the next clip", function () { return shell.media.poster && shell.media.position === 0 })
    }

    function stepIdleFollowStill() {
        shell.check("idle player, next clip: still stopped", shell.media.status, "stopped")
        shell.check("idle player, next clip: no helper process", shell.lines("starts.log").length, 0)
        shell.media.destroy()
        shell.media = null
    }

    function stepBefore() {
        shell.check("before play: no helper process was started", shell.lines("starts.log").length, 0)
        shell.media = shell.make(shell.clip, "video")
        shell.check("built, not playing: the inhibitor is off", shell.media.keepsAwake, false)
        shell.check("built, not playing: still no helper process", shell.lines("starts.log").length, 0)
        shell.check("built: the inhibitor's window is the FloatingWindow the host names", shell.media.inhibitedWindow === win, true)
        shell.media.togglePlay()
        shell.waitFor("the video plays", function () { return shell.media.status === "playing" })
    }

    function stepPlaying() {
        shell.check("video playing: the inhibitor is on", shell.media.keepsAwake, true)
        shell.waitFor("the helper is fed", function () { return shell.lines("lines.log").length >= 1 && shell.positionTicks >= 3 })
    }

    function stepFirstLine() {
        var first = shell.states()
        shell.check("first line: one helper process started", shell.lines("starts.log").length, 1)
        shell.check("first line: Playing, the file name and its url", [first[0].status, first[0].title, first[0].url],
                    ["Playing", "clip.mp4", "file://" + shell.clip])
        shell.check("first line: not a seek", first[0].seeked, false)
        shell.baseline = first.length
        shell.positionTicks = 0
        shell.waitFor("position ticks arrive", function () { return shell.positionTicks >= 8 })
    }

    function stepNoTickLines() {
        shell.check("ticks: a dozen position ticks wrote no state line", shell.states().length, shell.baseline)
        shell.command("PlayPause")
        shell.waitFor("PlayPause pauses", function () { return shell.media.status === "paused" })
    }

    function stepPaused() {
        shell.check("PlayPause: the inhibitor is off in the same frame the player pauses", shell.media.keepsAwake, false)
        shell.waitFor("the Paused line", function () { return shell.states().length > shell.baseline })
    }

    function stepPausedLine() {
        var all = shell.states()
        shell.check("pause: one more line, Paused", [all.length, all[all.length - 1].status], [shell.baseline + 1, "Paused"])
        shell.baseline = all.length
        shell.media.seekTo(1500)
        shell.waitFor("the seek line", function () { return shell.states().length > shell.baseline })
    }

    function stepSeekLine() {
        var all = shell.states()
        var last = all[all.length - 1]
        shell.check("seek: one line, flagged as a seek at the target", [all.length, last.seeked, last.position], [shell.baseline + 1, true, 1500])
        shell.baseline = all.length
        shell.command("SetPosition 700")
        shell.waitFor("SetPosition seeks", function () { return shell.states().length > shell.baseline })
    }

    function stepSetPosition() {
        var all = shell.states()
        var last = all[all.length - 1]
        shell.check("SetPosition: the player seeks to it and says so once", [all.length, last.seeked, last.position], [shell.baseline + 1, true, 700])
        shell.baseline = all.length
        shell.command("Seek 300")
        shell.waitFor("Seek seeks", function () { return shell.states().length > shell.baseline })
    }

    function stepRelativeSeek() {
        var all = shell.states()
        var last = all[all.length - 1]
        shell.check("Seek: the player moves by it from where it was and says so once", [all.length, last.seeked, last.position], [shell.baseline + 1, true, 1000])
        shell.baseline = all.length
        shell.command("Play")
        shell.waitFor("Play resumes", function () { return shell.media.status === "playing" })
    }

    function stepResumed() {
        shell.check("Play: the inhibitor is back on", shell.media.keepsAwake, true)
        shell.command("Stop")
        shell.waitFor("Stop stops", function () { return shell.media.status === "stopped" })
    }

    function stepStopped() {
        shell.check("Stop: the inhibitor is off", shell.media.keepsAwake, false)
        shell.media.destroy()
        shell.waitFor("the first helper ends", function () { return shell.stubGone(1) })
    }

    function stepClosed() {
        shell.baseline = shell.states().length
        shell.media = shell.make(shell.tone, "audio")
        shell.media.togglePlay()
        shell.waitFor("the tone plays", function () { return shell.media.status === "playing" && shell.lines("starts.log").length === 2 })
    }

    function stepAudio() {
        shell.check("audio playing: the inhibitor stays off", shell.media.keepsAwake, false)
        shell.waitFor("the tone is announced", function () { return shell.lines("lines.log").length > shell.baseline })
    }

    function stepAudioLine() {
        var all = shell.states()
        shell.check("audio playing: a second helper was started and told Playing", [shell.lines("starts.log").length, all[shell.baseline].status, all[shell.baseline].title],
                    [2, "Playing", "tone.ogg"])
        shell.media.destroy()
        shell.waitFor("the second helper ends", function () { return shell.stubGone(2) })
    }

    function stepQuickLook() {
        quick.show(shell.clip, "video-x-generic", 1000, "MPEG-4 video", "", -1)
        shell.waitFor("Quick Look plays the clip", function () {
            var item = quick.surfaceItem()
            return item !== null && item.status === "playing" && shell.lines("starts.log").length === 3
        })
    }

    // Down in Quick Look follows to the next row: the same PreviewMedia takes the new path, and the next clip has to play as the first did.
    function stepQuickLookNext() {
        shell.media = quick.surfaceItem()
        quick.follow(shell.nextClip, "video-x-generic", 1000, "MPEG-4 video", "")
        shell.waitFor("Quick Look plays the next clip", function () {
            var item = quick.surfaceItem()
            return item !== null && item.path === shell.nextClip && item.status === "playing"
        })
    }

    function stepQuickLookNextPlaying() {
        shell.check("Quick Look next clip: the same player took the new path", quick.surfaceItem() === shell.media, true)
    }

    function stepQuickLookPlaying() {
        var item = quick.surfaceItem()
        shell.check("Quick Look playing: the inhibitor is on", item.keepsAwake, true)
        shell.check("Quick Look playing: the inhibitor's window is the FloatingWindow the host names", item.inhibitedWindow === win, true)
        quick.close()
        shell.waitFor("closing Quick Look ends the helper", function () { return shell.stubGone(3) })
    }

    // A Stop that lands while the next clip loads is the listener's choice: that clip loads and stays stopped.
    function stepStopWhileLoading() {
        shell.media = mediaComponent.createObject(holder, { path: shell.clip, kind: "video", autoStart: true })
        shell.waitFor("the autostart player plays", function () { return shell.media.status === "playing" })
    }

    function stepStopWhileLoadingNext() {
        shell.media.path = shell.nextClip
        shell.media.remote("Stop")
        shell.waitFor("the stopped player settles on the next clip", function () { return shell.media.poster || shell.media.status === "playing" })
    }

    function stepStopWhileLoadingStill() {
        shell.check("Stop while loading: the next clip stays stopped", shell.media.status, "stopped")
        shell.media.destroy()
        shell.media = null
    }

    readonly property var script: [
        shell.stepIdleFollow, shell.stepIdleFollowLoad, shell.stepIdleFollowStill, shell.stepBefore, shell.stepPlaying, shell.stepFirstLine, shell.stepNoTickLines, shell.stepPaused,
        shell.stepPausedLine, shell.stepSeekLine, shell.stepSetPosition, shell.stepRelativeSeek, shell.stepResumed,
        shell.stepStopped, shell.stepClosed, shell.stepAudio, shell.stepAudioLine, shell.stepQuickLook,
        shell.stepQuickLookNext, shell.stepQuickLookNextPlaying, shell.stepQuickLookPlaying,
        shell.stepStopWhileLoading, shell.stepStopWhileLoadingNext, shell.stepStopWhileLoadingStill
    ]

    function finish() {
        shell.done = true
        ticker.running = false
        if (shell.failures.length === 0) shell.log("PASS " + shell.checks + " checks")
        shell.log("DONE failures=" + shell.failures.length)
        shell.quit()
    }

    Timer {
        id: ticker
        interval: 40
        repeat: true
        running: true
        onTriggered: {
            shell.ticks++
            if (shell.done || shell.ticks < 4) return
            if (shell.until !== null) {
                if (shell.until()) shell.until = null
                else if (--shell.budget <= 0) {
                    shell.failures.push("timeout waiting for " + shell.untilName)
                    shell.log("FAIL timeout waiting for " + shell.untilName)
                    shell.finish()
                    return
                }
                else return
            }
            if (shell.step >= shell.script.length) { shell.finish(); return }
            var run = shell.script[shell.step]
            shell.step++
            run()
        }
    }

    // A harness that never reaches its end is a failure with a receipt, not a hung suite.
    Timer {
        interval: 80000
        running: true
        onTriggered: {
            if (shell.done) return
            shell.failures.push("timeout at step " + shell.step)
            shell.log("FAIL timeout at step " + shell.step)
            shell.finish()
        }
    }
}
