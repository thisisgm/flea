//! Packed samples, one u32 of alpha, red, green and blue bytes, and the frame's bytes out of the raster; the conversions are exact, the same bytes `f32::round` and a division by 255 give.
use super::Raster;
use crate::ui_logic_pv4::color::Rgb;

// Each byte over 255, divided once at compile time with the same correctly rounded division the CPU does.
static UNIT: [f32; 256] = {
    let mut unit = [0.; 256];
    let mut i = 0;
    while i < 256 {
        unit[i] = i as f32 / 255.;
        i += 1;
    }
    unit
};

/// The byte nearest 255 times `v` clamped to 0..1, a half rounding up, which is what `f32::round` gives there without its libm call; NaN is 0.
pub(super) fn byte(v: f32) -> u32 {
    let x = v.clamp(0., 1.) * 255.;
    let whole = x as u32;
    whole + u32::from(x - whole as f32 >= 0.5)
}

pub(super) fn pack(c: Rgb, a: f32) -> u32 {
    (byte(a) << 24) | (byte(c[0]) << 16) | (byte(c[1]) << 8) | byte(c[2])
}

pub(super) fn unpack(p: u32) -> (Rgb, f32) {
    let f = |s: u32| UNIT[((p >> s) & 0xff) as usize];
    ([f(16), f(8), f(0)], f(24))
}

// The byte order a frame leaves in.
#[derive(Clone, Copy, PartialEq)]
enum Order {
    Rgb,
    Rgba,
    Bgra,
}

impl Raster {
    pub(super) fn get(&self, at: usize) -> u32 {
        let b = &self.color[4 * at..4 * at + 4];
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }

    pub(super) fn set(&mut self, at: usize, p: u32) {
        self.color[4 * at..4 * at + 4].copy_from_slice(&p.to_le_bytes());
    }

    // The sample an opaque line of `color` leaves under full cover, when the blend gives it whatever byte each channel held; checked once a colour a frame, at most `SOLIDS` colours.
    pub(super) fn solid(&mut self, color: Rgb) -> Option<u32> {
        let key = color.map(f32::to_bits);
        if let Some(&(_, p)) = self.solids.iter().find(|(k, _)| *k == key) {
            return p;
        }
        if self.solids.len() == super::SOLIDS {
            return None;
        }
        // The blend at alpha 1 is `mix(held, color, 1.)` a channel and alpha 255 for any held alpha.
        let p = color.iter().all(|&c| UNIT.iter().all(|&d| byte(d + (c - d) * 1.) == byte(c))).then(|| pack(color, 1.));
        self.solids.push((key, p));
        p
    }

    /// Takes `buffer` as the sample store the next frame draws into, so a frame at one sample a pixel can leave in it with no copy.
    pub fn lend(&mut self, buffer: Vec<u8>) {
        self.color = buffer;
    }

    /// The frame as opaque BGRA bytes, into `out`: one sample a pixel on an opaque ground hands the sample store over as it is, anything else resolves into `out`.
    pub fn frame_into(&mut self, out: &mut Vec<u8>) {
        if self.ss == 1 && self.opaque {
            *out = std::mem::take(&mut self.color);
        } else {
            self.bgra_into(out);
            self.stats.resolved = self.w * self.h;
        }
    }

    /// The frame as RGB bytes, `ss` by `ss` samples averaged into each pixel, into `out` (cleared first).
    pub fn rgb_into(&self, out: &mut Vec<u8>) {
        out.clear();
        self.resolve(out, Order::Rgb);
    }

    /// The frame as opaque BGRA bytes, the order GPUI's atlas keeps, into `out`; one sample a pixel overwrites a buffer already the frame's size in place.
    pub fn bgra_into(&self, out: &mut Vec<u8>) {
        let n = self.w * self.h;
        if self.ss == 1 {
            out.resize(n * 4, 0);
            for (o, p) in out.chunks_exact_mut(4).zip(self.color[..n * 4].chunks_exact(4)) {
                o.copy_from_slice(&[p[0], p[1], p[2], 255]);
            }
            return;
        }
        out.clear();
        self.resolve(out, Order::Bgra);
    }

    /// The frame as unpremultiplied RGBA bytes, for a thumbnail on a transparent ground.
    pub fn rgba_into(&self, out: &mut Vec<u8>) {
        out.clear();
        self.resolve(out, Order::Rgba);
    }

    // Appends the resolved pixels; one opaque sample a pixel is its own bytes, the same the averaging below rounds to.
    fn resolve(&self, out: &mut Vec<u8>, order: Order) {
        let alpha = order == Order::Rgba;
        if self.ss == 1 && order == Order::Rgb {
            out.extend(self.color[..self.w * self.h * 4].chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]));
            return;
        }
        let (ss, sw) = (self.ss, self.w * self.ss);
        let n = (ss * ss) as f32;
        for y in 0..self.h {
            for x in 0..self.w {
                let mut acc = [0f32; 4];
                for sy in 0..ss {
                    for sx in 0..ss {
                        let (c, a) = unpack(self.get((y * ss + sy) * sw + x * ss + sx));
                        let weight = if alpha { a } else { 1. };
                        for i in 0..3 {
                            acc[i] += c[i] * weight;
                        }
                        acc[3] += a;
                    }
                }
                let cover = if alpha { acc[3] } else { n };
                let rgb = if cover > 0. { [acc[0] / cover, acc[1] / cover, acc[2] / cover] } else { [0.; 3] };
                let [r, g, b] = rgb.map(|v| byte(v) as u8);
                match order {
                    Order::Rgb => out.extend([r, g, b]),
                    Order::Rgba => out.extend([r, g, b, byte(acc[3] / n) as u8]),
                    Order::Bgra => out.extend([b, g, r, 255]),
                }
            }
        }
    }
}
