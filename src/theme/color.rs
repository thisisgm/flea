// Colors use unpremultiplied sRGB, as Qt.rgba and the QML contrast helpers do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);

    pub const fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    // The optional final byte follows Commons.Style.colorFromHex, not QML's ARGB string.
    pub fn parse(raw: &str) -> Option<Self> {
        let s = raw.trim().strip_prefix('#')?;
        if !s.is_ascii() || !matches!(s.len(), 3 | 6 | 8) {
            return None;
        }
        let expanded;
        let s = if s.len() == 3 {
            expanded = s.chars().flat_map(|c| [c, c]).collect::<String>();
            expanded.as_str()
        } else {
            s
        };
        let byte = |at| {
            u8::from_str_radix(&s[at..at + 2], 16)
                .ok()
                .map(|v| v as f64 / 255.0)
        };
        Some(Self {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: if s.len() == 8 { byte(6)? } else { 1.0 },
        })
    }

    pub fn alpha(self, a: f64) -> Self {
        Self {
            a: if a.is_finite() {
                a.clamp(0.0, 1.0)
            } else {
                0.0
            },
            ..self
        }
    }

    pub fn mix(self, toward: Self, t: f64) -> Self {
        Self::rgb(
            self.r + (toward.r - self.r) * t,
            self.g + (toward.g - self.g) * t,
            self.b + (toward.b - self.b) * t,
        )
    }

    pub fn over(self, ground: Self) -> Self {
        ground.mix(self, self.a)
    }

    pub fn saturation(self) -> f64 {
        let hi = self.r.max(self.g).max(self.b);
        let lo = self.r.min(self.g).min(self.b);
        if hi == 0.0 {
            0.0
        } else {
            (hi - lo) / hi
        }
    }

    // QColor converts 16-bit RGB to float HSV and exposes a quantized saturation as float.
    pub fn qcolor_saturation(self) -> f64 {
        let channel = |c: f64| (c.clamp(0.0, 1.0) * 65535.0).round() as f32 / 65535.0;
        let (r, g, b) = (channel(self.r), channel(self.g), channel(self.b));
        let hi = r.max(g).max(b);
        let lo = r.min(g).min(b);
        if hi == 0.0 {
            0.0
        } else {
            ((((hi - lo) / hi) * 65535.0).round() / 65535.0) as f64
        }
    }

    pub fn luminance(self) -> f64 {
        fn linear(c: f64) -> f64 {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    pub fn darker(self) -> Self {
        // QColor::darker(140) scales the HSV value, which scales all three RGB channels.
        Self::rgb(self.r / 1.4, self.g / 1.4, self.b / 1.4).quantized()
    }

    pub fn quantized(self) -> Self {
        let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() / 255.0;
        Self {
            r: q(self.r),
            g: q(self.g),
            b: q(self.b),
            a: q(self.a),
        }
    }

    // Golden colors always have eight bits per channel and an explicit leading alpha byte.
    pub fn argb(self) -> String {
        let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            q(self.a),
            q(self.r),
            q(self.g),
            q(self.b)
        )
    }

    pub fn hex(self) -> String {
        format!("#{}", &self.argb()[3..])
    }
}

pub fn ratio(a: Color, b: Color) -> f64 {
    let a = a.luminance();
    let b = b.luminance();
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

// This is Contrast.ensureRatio, including the repair after eight-bit rounding.
pub fn ensure_ratio(ink: Color, ground: Color, need: f64) -> Color {
    let need = if need > 0.0 { need } else { 4.5 };
    if ratio(ink, ground) >= need {
        return ink;
    }
    let toward = if ground.luminance() > 0.179 {
        Color::BLACK
    } else {
        Color::WHITE
    };
    let (mut lo, mut hi, mut best) = (0.0, 1.0, toward);
    for _ in 0..18 {
        let mid = (lo + hi) / 2.0;
        let candidate = ink.mix(toward, mid);
        if ratio(candidate, ground) >= need {
            best = candidate;
            hi = mid;
        } else {
            lo = mid;
        }
    }
    if ratio(best, ground) < need {
        best = toward;
    }
    best = best.quantized();
    let step = if toward.r > 0.5 {
        1.0 / 255.0
    } else {
        -1.0 / 255.0
    };
    for _ in 0..255 {
        if ratio(best, ground) >= need {
            break;
        }
        let next = Color::rgb(
            (best.r + step).clamp(0.0, 1.0),
            (best.g + step).clamp(0.0, 1.0),
            (best.b + step).clamp(0.0, 1.0),
        );
        if next.argb() == best.argb() {
            break;
        }
        best = next;
    }
    best.quantized()
}
