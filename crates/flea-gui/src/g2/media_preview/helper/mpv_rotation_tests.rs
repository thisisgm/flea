use super::Rotation;

#[test]
fn metadata_rotation_normalizes_signed_angles_and_full_turns() {
    for (degrees, expected) in [
        (0, Rotation::None),
        (360, Rotation::None),
        (90, Rotation::Clockwise),
        (450, Rotation::Clockwise),
        (-270, Rotation::Clockwise),
        (180, Rotation::Half),
        (-180, Rotation::Half),
        (270, Rotation::Counterclockwise),
        (-90, Rotation::Counterclockwise),
    ] {
        assert_eq!(Rotation::from_degrees(degrees), expected);
    }
}

#[test]
fn rotation_swaps_buffer_dimensions_only_for_quarter_turns() {
    for degrees in [0, 90, 180, 270] {
        let rotation = Rotation::from_degrees(degrees);
        let output = if degrees == 90 || degrees == 270 {
            (2, 3)
        } else {
            (3, 2)
        };
        assert_eq!(rotation.dimensions((3, 2)), output);
        assert_eq!(rotation.dimensions(output), (3, 2));
    }
}

#[test]
fn each_draw_transform_preserves_every_channel_and_places_all_corners() {
    // A non-square frame distinguishes both quarter turns and exposes stride errors.
    let pixels: Vec<_> = (1..=6).flat_map(|n| [n, n + 10, n + 20, 255]).collect();
    let mut rotated = Vec::new();
    for (degrees, expected) in [
        (0, [1, 2, 3, 4, 5, 6]),
        (90, [4, 1, 5, 2, 6, 3]),
        (180, [6, 5, 4, 3, 2, 1]),
        (270, [3, 6, 2, 5, 1, 4]),
    ] {
        let output = Rotation::from_degrees(degrees).apply(3, 2, &pixels, &mut rotated);
        let expected: Vec<_> = expected
            .into_iter()
            .flat_map(|n| [n, n + 10, n + 20, 255])
            .collect();
        assert_eq!(output, expected, "rotation {degrees}");
    }
}

#[test]
fn unrotated_frame_borrows_the_render_buffer_and_rotated_frames_reuse_storage() {
    let pixels = [255; 24];
    let mut rotated = Vec::new();
    assert!(std::ptr::eq(
        Rotation::None.apply(3, 2, &pixels, &mut rotated).as_ptr(),
        pixels.as_ptr()
    ));
    assert_eq!(rotated.capacity(), 0);
    Rotation::Clockwise.apply(3, 2, &pixels, &mut rotated);
    let allocation = rotated.as_ptr();
    Rotation::Counterclockwise.apply(3, 2, &pixels, &mut rotated);
    assert_eq!(rotated.as_ptr(), allocation);
}
