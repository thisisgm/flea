.import "../../ui/js/Ranger.js" as Ranger

function run(check) {
    var at = Ranger.layout(1097, 791)
    check("the referenced 1097 px window hides the previous column", at.previousVisible, false)
    check("and splits its ranger area into two columns", at.columnWidth + "|" + at.lastWidth, "395|396")

    var below = Ranger.layout(800, 700)
    check("a narrower window stays on two columns", below.previousVisible + "|" + below.columnWidth, "false|350")

    var above = Ranger.layout(1098, 791)
    check("one pixel above the threshold restores the previous column", above.previousVisible, true)
    check("and three columns consume every pixel without a gap",
          above.columnWidth + "|" + above.lastWidth + "|" + (above.columnWidth * 2 + above.lastWidth),
          "263|265|791")
}
