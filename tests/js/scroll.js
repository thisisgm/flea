.import "../../ui/js/Scroll.js" as Scroll

function run(check) {
    check("touchpad pixels are accelerated four times", Scroll.delta(-10, -120, 37), 40)
    check("a click-wheel notch moves six rows", Scroll.delta(0, -120, 37), 222)
    check("pixel precision wins when both forms are present", Scroll.delta(-10, -120, 37), 40)
    check("the opposite direction keeps its sign", Scroll.delta(8, 120, 37), -32)

    check("scrolling clamps at the beginning", Scroll.position(5, 0, 1000, 600, 8, 0, 37), 0)
    check("scrolling clamps at the end", Scroll.position(390, 0, 1000, 600, -10, 0, 37), 400)
    check("a nonzero ListView origin is preserved", Scroll.position(25, 20, 1000, 600, 8, 0, 37), 20)
    check("short content never scrolls below its origin", Scroll.position(0, 0, 300, 600, -20, 0, 37), 0)
}
