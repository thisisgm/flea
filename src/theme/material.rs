use super::{ratio, Color, ThemeColors};

// Material.js's contrast envelope over white and black backdrops at Omarchy's blur brightness.
#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub background: Color,
    pub surface: Color,
    pub foreground: Color,
    pub muted: Color,
    pub metadata: Color,
    pub blur_brightness: f64,
}

impl Material {
    pub fn from_colors(c: &ThemeColors, glassy: bool) -> Self {
        Self {
            background: c.background,
            surface: c.surface,
            foreground: c.foreground,
            muted: c.opaqueMuted,
            metadata: c.opaqueMetadata,
            blur_brightness: if glassy { 0.75 } else { 1.0 },
        }
    }

    /// The OEM blur transforms the white endpoint before the app composites its fills.
    pub fn backdrops(&self) -> [Color; 2] {
        [
            Color::rgb(
                self.blur_brightness,
                self.blur_brightness,
                self.blur_brightness,
            )
            .quantized(),
            Color::BLACK,
        ]
    }

    pub fn worst(&self, ink: Color, alpha: u8) -> f64 {
        self.worst_over_washes(ink, alpha, &[])
    }

    /// Contrast on the actual translucent plane, including its existing selection washes.
    fn worst_over_washes(&self, ink: Color, alpha: u8, washes: &[Color]) -> f64 {
        [self.background, self.surface]
            .into_iter()
            .flat_map(|plane| {
                self.backdrops().into_iter().flat_map(move |backdrop| {
                    let ground = plane.alpha(alpha as f64 / 255.).over(backdrop).quantized();
                    std::iter::once(ratio(ink, ground)).chain(
                        washes
                            .iter()
                            .map(move |wash| ratio(ink, wash.over(ground).quantized())),
                    )
                })
            })
            .fold(f64::INFINITY, f64::min)
    }

    pub fn lift_over_washes(&self, ink: Color, alpha: u8, need: f64, washes: &[Color]) -> Color {
        self.search(ink, |candidate| {
            self.worst_over_washes(candidate, alpha, washes) >= need
        })
    }

    fn toward(&self) -> Color {
        if self.foreground.luminance() > self.background.luminance() {
            Color::WHITE
        } else {
            Color::BLACK
        }
    }

    fn search(&self, from: Color, holds: impl Fn(Color) -> bool) -> Color {
        if holds(from) {
            return from;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..20 {
            let mid = (lo + hi) / 2.0;
            if holds(from.mix(self.toward(), mid).quantized()) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        let mut t = hi;
        while t <= 1.0 {
            let candidate = from.mix(self.toward(), t).quantized();
            if holds(candidate) {
                return candidate;
            }
            t += 1.0 / 512.0;
        }
        self.toward()
    }

    pub fn lift_ink(&self, ink: Color, alpha: u8, need: f64, quiet: Option<Color>) -> Color {
        self.search(ink, |c| {
            self.worst(c, alpha) >= need
                && quiet
                    .map(|q| {
                        if self.toward() == Color::WHITE {
                            c.luminance() >= q.luminance()
                        } else {
                            c.luminance() <= q.luminance()
                        }
                    })
                    .unwrap_or(true)
        })
    }

    fn quieter(&self, ink: Color, primary: Color) -> bool {
        let less = if self.toward() == Color::WHITE {
            ink.luminance() < primary.luminance()
        } else {
            ink.luminance() > primary.luminance()
        };
        less && ratio(ink, primary) >= 1.25
    }

    fn primary_holds(
        &self,
        ink: Color,
        alpha: u8,
        need: f64,
        muted: Color,
        metadata: Color,
    ) -> bool {
        self.worst(ink, alpha) >= need && self.quieter(muted, ink) && self.quieter(metadata, ink)
    }

    pub fn inks(&self, alpha: u8, need: f64) -> (Color, Color, Color) {
        let muted = self.lift_ink(self.muted, alpha, need, None);
        let mut metadata = self.lift_ink(self.metadata, alpha, need, None);
        let beyond = if self.toward() == Color::WHITE {
            metadata.luminance() >= self.foreground.luminance()
        } else {
            metadata.luminance() <= self.foreground.luminance()
        };
        if beyond {
            metadata = muted;
        }
        let primary = self.search(self.foreground, |c| {
            self.primary_holds(c, alpha, need, muted, metadata)
        });
        (primary, muted, metadata)
    }

    pub fn floor(&self, need: f64) -> u8 {
        for a in 192..255 {
            let (primary, muted, metadata) = self.inks(a, need);
            if self.primary_holds(primary, a, need, muted, metadata)
                && self.worst(muted, a) >= need
                && self.worst(metadata, a) >= need
            {
                return a;
            }
        }
        255
    }

    // The caller chooses this path only when Glass.on && Glass.usesDefault.
    pub fn apply(&self, colors: &mut ThemeColors, alpha: u8) {
        let (fg, muted, metadata) = self.inks(alpha, 4.5);
        colors.foreground = fg;
        colors.muted = muted;
        colors.metadata = metadata;
        colors.foregroundBright = if ratio(colors.opaqueForegroundBright, colors.background)
            > ratio(fg, colors.background)
        {
            colors.opaqueForegroundBright
        } else {
            fg
        };
        colors.accent = self.lift_ink(colors.accent, alpha, 3.0, None);
        colors.mutedFrame = self.lift_ink(colors.opaqueMutedFrame, alpha, 3.0, None);
        colors.metadataFrame = self.lift_ink(colors.opaqueMetadataFrame, alpha, 3.0, None);
        colors.accentFrame = self.lift_ink(colors.opaqueAccentFrame, alpha, 3.0, None);
        colors.symlink = self.lift_ink(colors.opaqueSymlink, alpha, 4.5, None);
        colors.executable = self.lift_ink(colors.opaqueExecutable, alpha, 4.5, None);
        colors.error = self.lift_ink(colors.opaqueError, alpha, 4.5, Some(muted));
        colors.errorOnSurface = self.lift_ink(colors.opaqueErrorOnSurface, alpha, 4.5, Some(muted));
    }
}
