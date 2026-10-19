//! Theme inks for a 3D view: the lit role, its darker shade in OKLCH, and the 10% tone steps between them.

/// One encoded sRGB colour, each channel 0 to 1.
pub type Rgb = [f32; 3];

// The dark shade is the same hue this much lower in OKLCH lightness, so it stays the darker in light themes too.
pub const SHADE_DROP: f32 = 0.30;
// The G-code layers under the current one ease both shades halfway to the card ground.
pub const DIM_MIX: f32 = 0.5;

/// A lit colour and its dark shade; a face picks a 10% step between them by its angle to the light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub lit: Rgb,
    pub shade: Rgb,
}

impl Ink {
    /// The role or material colour as the lit end and its OKLCH shade as the dark end.
    pub fn from_lit(lit: Rgb) -> Self {
        Self { lit, shade: shade_of(lit) }
    }

    /// Both ends eased halfway to the ground, the dimmer pair the layers below a G-code cursor draw in.
    pub fn dimmed(self, ground: Rgb) -> Self {
        Self { lit: mix(self.lit, ground, DIM_MIX), shade: mix(self.shade, ground, DIM_MIX) }
    }

    /// The tone for a face whose normal makes `light` with the light, `color-mix(lit p%, shade)` in 10% steps.
    pub fn tone(&self, light: f32) -> Rgb {
        mix(self.shade, self.lit, tone_step(light))
    }
}

/// The share of the lit end for a face lit by `light` (a dot product), rounded to a 10% step.
pub fn tone_step(light: f32) -> f32 {
    let t = ((light + 0.3) / 1.25).clamp(0., 1.);
    (t * 10.).round() / 10.
}

/// The step `tone_step` rounds to, 0 to 10 tenths, found without its libm call; `None` for a NaN light, whose tone is NaN.
pub fn tone_index(light: f32) -> Option<usize> {
    let x = ((light + 0.3) / 1.25).clamp(0., 1.) * 10.;
    if x.is_nan() {
        return None;
    }
    let whole = x as usize;
    Some(whole + usize::from(x - whole as f32 >= 0.5))
}

/// `color-mix(in srgb, a (1-t), b t)`: a straight mix of the encoded channels.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// The colour `SHADE_DROP` lower in OKLCH lightness, hue and chroma kept, clipped into sRGB.
pub fn shade_of(color: Rgb) -> Rgb {
    let [l, a, b] = oklab(color);
    let lower = (l - SHADE_DROP).max(0.);
    from_oklab([lower, a, b])
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_encoded(c: f32) -> f32 {
    let c = c.clamp(0., 1.);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1. / 2.4) - 0.055 }
}

/// Björn Ottosson's OKLab from encoded sRGB.
pub fn oklab(color: Rgb) -> [f32; 3] {
    let [r, g, b] = color.map(to_linear);
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// Encoded sRGB from OKLab, each channel clipped to 0 to 1.
pub fn from_oklab([l, a, b]: [f32; 3]) -> Rgb {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_,
        -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_,
    ]
    .map(to_encoded)
}

/// A byte colour as encoded channels, the way a file's material colour arrives.
pub fn from_bytes(color: [u8; 3]) -> Rgb {
    color.map(|c| f32::from(c) / 255.)
}

/// An encoded colour as bytes, rounded.
pub fn to_bytes(color: Rgb) -> [u8; 3] {
    color.map(|c| (c.clamp(0., 1.) * 255.).round() as u8)
}

/// The roles a 3D view draws with, read from the live theme by the caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The card or column ground the view sits on.
    pub ground: Rgb,
    pub foreground: Rgb,
    /// The plate's 10 mm hairlines.
    pub muted: Rgb,
    /// The plate's edge.
    pub frame: Rgb,
}

impl Palette {
    /// The ground's luminance is below the foreground's: a dark theme.
    pub fn dark(&self) -> bool {
        luminance(self.ground) < luminance(self.foreground)
    }
}

fn luminance(c: Rgb) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}
