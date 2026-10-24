//! The square fixture must occupy fitted pixels inside the wider media frame.
pub fn square_inside(frame: &str, picture: &str) {
    let parse = |value: &str| {
        value
            .split_whitespace()
            .map(|n| n.parse::<i32>().unwrap())
            .collect::<Vec<_>>()
    };
    let outer = parse(frame);
    let inner = parse(picture);
    assert_eq!(outer.len(), 4);
    assert_eq!(inner.len(), 4);
    assert!(inner[2] > 0 && inner[3] > 0);
    assert!(
        (inner[2] - inner[3]).abs() <= 1,
        "the 2x2 image stays square: {inner:?}"
    );
    assert!(
        inner[2] < outer[2],
        "letterbox pixels are not reported as image pixels"
    );
    assert!(inner[0] >= outer[0] && inner[1] >= outer[1]);
    assert!(inner[0] + inner[2] <= outer[0] + outer[2]);
    assert!(inner[1] + inner[3] <= outer[1] + outer[3]);
}
