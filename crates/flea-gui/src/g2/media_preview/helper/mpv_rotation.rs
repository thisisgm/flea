//! Metadata rotation after the software VO has rendered an unrotated BGRx buffer.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Rotation {
    #[default]
    None,
    Clockwise,
    Half,
    Counterclockwise,
}

impl Rotation {
    pub(super) fn from_degrees(degrees: i64) -> Self {
        match degrees.rem_euclid(360) {
            90 => Self::Clockwise,
            180 => Self::Half,
            270 => Self::Counterclockwise,
            _ => Self::None,
        }
    }

    /// Swapping twice recovers the render size from the upright frame size.
    pub(super) fn dimensions(self, (width, height): (u32, u32)) -> (u32, u32) {
        match self {
            Self::Clockwise | Self::Counterclockwise => (height, width),
            Self::None | Self::Half => (width, height),
        }
    }

    /// One reused destination, with no allocation or copy for an unrotated frame.
    pub(super) fn apply<'a>(
        self,
        width: u32,
        height: u32,
        pixels: &'a [u8],
        rotated: &'a mut Vec<u8>,
    ) -> &'a [u8] {
        if self == Self::None {
            return pixels;
        }
        rotated.resize(pixels.len(), 0);
        let (output_width, _) = self.dimensions((width, height));
        for y in 0..height {
            for x in 0..width {
                let (dx, dy) = match self {
                    Self::Clockwise => (height - 1 - y, x),
                    Self::Half => (width - 1 - x, height - 1 - y),
                    Self::Counterclockwise => (y, width - 1 - x),
                    Self::None => unreachable!(),
                };
                let source = (y as usize * width as usize + x as usize) * 4;
                let destination = (dy as usize * output_width as usize + dx as usize) * 4;
                rotated[destination..destination + 4].copy_from_slice(&pixels[source..source + 4]);
            }
        }
        rotated
    }
}

#[cfg(test)]
#[path = "mpv_rotation_tests.rs"]
mod tests;
