import QtQuick
import Quickshell.Io
import "Run.js" as Run

// The doing half of the shelf. Actions rule 1: every action is a `flea shelf` call, so this starts
// processes and parses their lines and knows nothing else about files.
Item {
  id: root
  visible: false

  // The flea that owns the pile, which the service resolved from the plugin's own setting.
  property string fleaCommand: "flea"
  property var run: Run.idle()
  signal landed(string sentence)
  // An action asked for while one is already running: the card says so rather than losing it.
  signal refused(string why)
  // The pile changed under an action, so whoever owns the file is told to read it again.
  signal ran()
  // What the running action is, so its result can be named when it lands.
  property string runVerb: ""
  property string runDest: ""
  property int runFailed: 0
  property string runError: ""

  function transfer(moving, dest, paths) {
    if (paths.length === 0) {
      return
    }
    if (action.running) {
      root.refused("The shelf is still busy with the last one.")
      return
    }
    root.runVerb = moving ? "Moved" : "Copied"
    root.runDest = dest
    root.runFailed = 0
    root.runError = ""
    action.command = [root.fleaCommand, "shelf", moving ? "move" : "copy", dest].concat(paths)
    action.running = true
  }

  function zip(date, paths) {
    if (paths.length === 0) {
      return
    }
    if (action.running) {
      root.refused("The shelf is still busy with the last one.")
      return
    }
    root.runVerb = "Zipped"
    root.runDest = String(paths.length)
    action.command = [root.fleaCommand, "shelf", "zip", date].concat(paths)
    action.running = true
  }

  function send(peer, paths) {
    if (paths.length === 0) {
      return
    }
    if (action.running) {
      root.refused("The shelf is still busy with the last one.")
      return
    }
    root.runVerb = "Sent"
    root.runDest = peer
    // What this send handed over, which is the chosen rows and not the whole pile.
    root.sending = paths.length
    action.command = [root.fleaCommand, "shelf", "send", peer].concat(paths)
    action.running = true
  }

  // esc while an action runs, which the transfer reads as its own cancel rather than a kill.
  function cancelRun() {
    if (stop.running) {
      return
    }
    stop.command = [root.fleaCommand, "shelf", "cancel"]
    stop.running = true
  }

  Process {
    id: action
    running: false
    stdout: SplitParser {
      splitMarker: "\n"
      onRead: function (line) { root.sampled(line) }
    }
    stderr: StdioCollector { waitForEnd: true }
    onExited: function (code) { root.finished(code) }
  }

  Process {
    id: stop
    running: false
    onExited: function (code) {
      if (code !== 0) {
        root.refused("The transfer could not be cancelled.")
      }
    }
  }

  // Sample input, one JSON object per line:
  //   {"t":"transferitem","ok":false,"err":"drafts is read-only for cover-grade.jpg"}
  //   {"t":"transferdone","ok":3,"cancelled":false}
  function sampled(line) {
    var next = Run.sampled(root.run, line)
    root.run = next
    var wire
    try {
      wire = JSON.parse(line)
    } catch (e) {
      return
    }
    // The one sentence a partial failure gets: the first thing that could not be done, named.
    if (wire && wire.t === "transferitem" && wire.ok === false) {
      root.runFailed += 1
      if (root.runError.length === 0) {
        root.runError = String(wire.err || "")
      }
    }
    if (wire && wire.t === "transferdone") {
      root.ok = Number(wire.ok) || 0
      root.cancelled = wire.cancelled === true
    }
  }

  property int ok: 0
  property int sending: 0
  property bool cancelled: false

  function finished(code) {
    root.run = Run.idle()
    root.ran()
    if (root.runVerb === "Zipped") {
      root.landed(code === 0 ? Run.zippedText(Number(root.runDest)) : "That pile could not be zipped.")
    } else if (root.runVerb === "Sent") {
      root.landed(code === 0 ? Run.sentText(root.sending, root.runDest) : "The items could not be sent.")
    } else if (root.runVerb.length > 0) {
      var sentence = root.cancelled
                     ? "Cancelled"
                     : Run.movedText(root.runVerb, root.ok, root.runFailed, root.ok + root.runFailed,
                                     Run.destName(root.runDest), root.runError)
      // A run that died before its first item reports no counts at all, so the exit status is the
      // only thing left that knows a copy did not happen.
      root.landed(sentence.length > 0 || code === 0 ? sentence : Run.runFailedText(root.runVerb))
    }
    root.runVerb = ""
    root.ok = 0
    root.runFailed = 0
    root.cancelled = false
  }

  // The flyouts' own rows: Flea's places, and Taildrop's peers.
  property var places: []
  property var peers: []
  // Actions rule 3: unavailable by configuration disappears, so a box whose peers call fails has
  // no Tailscale and loses the Send action rather than being offered an empty flyout.
  property bool peerable: true

  function askPlaces() {
    if (placeList.running) {
      return
    }
    placeList.command = [root.fleaCommand, "shelf", "places"]
    placeList.running = true
  }

  function askPeers() {
    if (peerList.running) {
      return
    }
    peerList.command = [root.fleaCommand, "shelf", "peers"]
    peerList.running = true
  }

  Process {
    id: placeList
    running: false
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.places = Run.lines(text)
    }
    stderr: StdioCollector { waitForEnd: true }
    onExited: function (code) {
      if (code !== 0) {
        root.refused("Flea could not list its places.")
      }
    }
  }

  Process {
    id: peerList
    running: false
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.peers = Run.lines(text)
    }
    stderr: StdioCollector { waitForEnd: true }
    onExited: function (code) { root.peerable = code === 0 }
  }

  // The chooser, which is Flea's own picker: it answers with a directory or with nothing.
  signal browsed(string dest)

  function choose(title, start) {
    if (chooser.running) {
      return
    }
    chooser.command = [root.fleaCommand, "shelf", "choose", title, start]
    chooser.running = true
  }

  Process {
    id: chooser
    running: false
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var dest = String(text).trim()
        if (dest.length > 0) {
          root.browsed(dest)
        }
      }
    }
    stderr: StdioCollector { waitForEnd: true }
    // A chooser that never opened answers with nothing, which is the same silence as a cancel.
    onExited: function (code) {
      if (code !== 0) {
        root.refused("The folder chooser could not open.")
      }
    }
  }

  // Copy to Clipboard: the files themselves, through Flea's own clipboard owner. It never takes the
  // transfer's slot, so a copy to the clipboard still works while a move runs.
  function clip(paths) {
    if (paths.length === 0) {
      return
    }
    if (clipper.running) {
      root.refused("The clipboard is still taking the last copy.")
      return
    }
    clipper.count = paths.length
    clipper.command = [root.fleaCommand, "shelf", "clip"].concat(paths)
    clipper.running = true
  }

  Process {
    id: clipper
    property int count: 0
    running: false
    stderr: StdioCollector { id: clipError; waitForEnd: true }
    onExited: function (code) {
      if (code === 0) {
        root.landed(Run.clippedText(clipper.count))
      } else {
        root.refused(Run.clipFailedText(String(clipError.text).split("\n")[0]))
      }
    }
  }

  // Paths: the card puts them on the clipboard, because a clipboard is a display and not a file.
  function yank(paths) {
    return paths.join("\n")
  }

}
