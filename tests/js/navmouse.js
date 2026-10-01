.import "../../ui/js/Nav.js" as Nav
.import "nav.js" as NavSuite

// The mouse forward button, beside tests/js/nav.js's back-button checks, which that file has no room
// left to hold. browsing() is nav.js's own navigating pane, so both buttons take the same route.
function run(check) {
    // Back then forward is a round trip, and forward hands back the history back took.
    var round = NavSuite.browsing(["/home/gm"])
    Nav.mouseBack(round)
    check("mouse back goes to the remembered directory", round.path, "/home/gm")
    // ui/PaneSwap.qml clears this when the rows land; the stub's listing answers at the request.
    round.listInFlight = false
    Nav.mouseForward(round)
    check("mouse forward retraces it", round.path, "/home/gm/Work")
    check("and puts the directory it left back behind it", round.history.join(",") + "|" + round.forwardHistory.length, "/home/gm|0")

    // With nothing ahead, forward is no navigation at all: it neither climbs nor asks for a listing.
    var ahead = NavSuite.browsing(["/home/gm"])
    Nav.mouseForward(ahead)
    check("mouse forward with nothing ahead stays put and asks for no listing",
          ahead.path + "|" + ahead.sent.length, "/home/gm/Work|0")

    // A press during a load keeps the entry it would have taken, as back's guard does.
    var busy = NavSuite.browsing([])
    busy.forwardHistory = ["/home/gm/Work/flea"]
    busy.listInFlight = true
    Nav.mouseForward(busy)
    check("a forward press during a load keeps the entry it would have taken",
          busy.forwardHistory.join(",") + "|" + busy.path, "/home/gm/Work/flea|/home/gm/Work")

    // Behind the pane's own context menu or the collision card the press does nothing, as back's does.
    var menuUp = NavSuite.browsing([])
    menuUp.forwardHistory = ["/home/gm/Work/flea"]
    menuUp.menuVisible = true
    Nav.mouseForward(menuUp)
    check("mouse forward behind an open context menu goes nowhere",
          menuUp.path + "|" + menuUp.sent.length + "|" + menuUp.forwardHistory.length, "/home/gm/Work|0|1")
    var cardUp = NavSuite.browsing([])
    cardUp.forwardHistory = ["/home/gm/Work/flea"]
    cardUp.collide = { opened: true }
    Nav.mouseForward(cardUp)
    check("mouse forward behind an open collision card goes nowhere",
          cardUp.path + "|" + cardUp.sent.length + "|" + cardUp.forwardHistory.length, "/home/gm/Work|0|1")
}
