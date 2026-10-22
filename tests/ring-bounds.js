.pragma library

// DialogButton rounds its local frame extent, while its hit box and caption keep their fractional layout.
function snappedFrame(frame, button) {
    var local = frame.mapToItem(button, 0, 0, frame.width, frame.height)
    return local.x === 0 && local.y === button.frameY
        && local.width === Math.round(button.width) && local.height === Math.round(button.frameHeight)
}

// Where each dialog's fields, buttons and check boxes lie in the 1000 x 650 test window on the 0.3.10 stage (Open With and Convert hold their status slot), before any ring room.
// Re-recorded under Qt 6.12, which lays a one-line Text at ceil(ascent + descent) of the face's fractional metrics where 6.11 laid
// it at ceil(ascent) + ceil(descent): a 12 px line is 16 px, not 17, and a 14 px line 19, not 20, so content under body lines rose
// a pixel or two while every x, width and height held.
// Sample input: "stop 12 new file" reads field, Cancel and Create as "x,y,width,height" strings in tree order; the save field follows 6b1824a75.
var PINS = ({
    "stop 12 new file": ["field 276.00,324.00,449.00,26.00",
        "DialogButton 594.75,358.00,61.13,26.00",
        "DialogButton 663.88,358.00,61.13,26.00"],
    "stop 12 open with": ["field 242.00,153.00,516.00,26.00",
        "DialogButton 642.13,511.00,61.13,26.00",
        "DialogButton 711.25,511.00,46.75,26.00",
        "CheckBox 242.00,425.00,16.00,16.00"],
    "stop 12 permissions": ["field 401.00,311.00,287.00,26.00",
        "DialogButton 564.94,509.00,61.13,26.00",
        "DialogButton 634.06,509.00,53.94,26.00",
        "CheckBox 434.00,203.00,16.00,16.00",
        "CheckBox 533.00,203.00,16.00,16.00",
        "CheckBox 631.00,203.00,16.00,16.00",
        "CheckBox 434.00,237.00,16.00,16.00",
        "CheckBox 533.00,237.00,16.00,16.00",
        "CheckBox 631.00,237.00,16.00,16.00",
        "CheckBox 434.00,271.00,16.00,16.00",
        "CheckBox 533.00,271.00,16.00,16.00",
        "CheckBox 631.00,271.00,16.00,16.00"],
    "stop 12 save picker": ["field 12.00,34.00,816.00,26.00"],
    "stop 12 convert": ["DialogButton 519.56,515.00,61.13,26.00",
        "DialogButton 588.69,515.00,68.31,26.00",
        "CheckBox 343.00,403.00,16.00,16.00"],
    "stop 12 network": ["field 303.00,147.00,394.00,26.00",
        "field 303.00,203.00,192.00,26.00",
        "field 505.00,203.00,192.00,26.00",
        "field 303.00,259.00,394.00,26.00",
        "field 303.00,315.00,394.00,26.00",
        "field 303.00,371.00,394.00,26.00",
        "field 303.00,427.00,394.00,26.00",
        "DialogButton 494.88,561.00,61.13,26.00",
        "DialogButton 564.00,561.00,133.00,26.00"],
    "stop 12 trash confirm": ["DialogButton 531.75,345.00,61.13,26.00",
        "DialogButton 600.88,345.00,61.13,26.00"],
    "stop 14 new file": ["field 238.00,324.00,524.00,30.00",
        "DialogButton 612.31,363.00,70.34,30.00",
        "DialogButton 691.66,363.00,70.34,30.00"],
    "stop 14 open with": ["field 199.00,127.00,602.00,30.00",
        "DialogButton 668.09,540.00,70.34,30.00",
        "DialogButton 747.44,540.00,53.56,30.00",
        "CheckBox 199.00,442.00,18.00,18.00"],
    "stop 14 permissions": ["field 382.00,309.00,341.00,30.00",
        "DialogButton 581.70,541.00,70.34,30.00",
        "DialogButton 661.05,541.00,61.95,30.00",
        "CheckBox 422.00,182.00,18.00,18.00",
        "CheckBox 539.00,182.00,18.00,18.00",
        "CheckBox 656.00,182.00,18.00,18.00",
        "CheckBox 422.00,222.00,18.00,18.00",
        "CheckBox 539.00,222.00,18.00,18.00",
        "CheckBox 656.00,222.00,18.00,18.00",
        "CheckBox 422.00,262.00,18.00,18.00",
        "CheckBox 539.00,262.00,18.00,18.00",
        "CheckBox 656.00,262.00,18.00,18.00"],
    "stop 14 save picker": function(captionHeight) {
        return ["field 14.00," + (14 + captionHeight + 9).toFixed(2) + ",812.00,30.00"]
    },
    "stop 14 convert": ["DialogButton 524.92,543.00,70.34,30.00",
        "DialogButton 604.27,543.00,78.73,30.00",
        "CheckBox 317.00,416.50,18.00,18.00"],
    "stop 14 network": ["field 268.00,128.00,465.00,30.00",
        "field 268.00,190.00,227.50,30.00",
        "field 505.50,190.00,227.50,30.00",
        "field 268.00,252.00,465.00,30.00",
        "field 268.00,314.00,465.00,30.00",
        "field 268.00,376.00,465.00,30.00",
        "field 268.00,438.00,465.00,30.00",
        "DialogButton 499.41,585.00,70.34,30.00",
        "DialogButton 578.75,585.00,154.25,30.00"],
    "stop 14 trash confirm": ["DialogButton 542.31,349.00,70.34,30.00",
        "DialogButton 621.66,349.00,70.34,30.00"]
})
