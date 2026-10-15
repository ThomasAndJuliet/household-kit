//! sRGB and OKLab/OKLCH colours (Björn Ottosson's OKLab, the matrices of
//! his 2020 post), as plain `f32` channels.

/// A gamma-encoded sRGB colour, each channel 0 to 1
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Srgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

/// An sRGB colour with its alpha, 0 (clear) to 1 (opaque)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Srgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// An OKLab colour: lightness `l` 0 to 1, and the opponent axes `a`, `b`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Oklab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

/// OKLab in polar form: lightness, chroma, and hue in degrees, 0 to 360
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub chroma: f32,
    pub hue: f32,
}

impl Srgb {
    pub const WHITE: Srgb = Srgb::new(1.0, 1.0, 1.0);
    pub const BLACK: Srgb = Srgb::new(0.0, 0.0, 0.0);

    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub const fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    /// `0xRRGGBB`
    pub const fn hex(rgb: u32) -> Self {
        Self::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    /// `#rrggbb` or `rrggbb`
    pub fn parse(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if hex.len() != 6 {
            return None;
        }
        u32::from_str_radix(hex, 16).ok().map(Self::hex)
    }

    /// Each channel rounded to eight bits
    pub fn to_rgb8(self) -> [u8; 3] {
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        [byte(self.r), byte(self.g), byte(self.b)]
    }

    /// `#rrggbb`
    pub fn to_hex(self) -> String {
        let [r, g, b] = self.to_rgb8();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// Each channel held to 0 to 1
    pub fn clamp(self) -> Self {
        Self::new(self.r.clamp(0.0, 1.0), self.g.clamp(0.0, 1.0), self.b.clamp(0.0, 1.0))
    }

    pub fn with_alpha(self, a: f32) -> Srgba {
        Srgba { r: self.r, g: self.g, b: self.b, a }
    }

    pub fn to_oklab(self) -> Oklab {
        let [r, g, b] = [self.r, self.g, self.b].map(|c| decode(f64::from(c)));
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        Oklab {
            l: (0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s) as f32,
            a: (1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s) as f32,
            b: (0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s) as f32,
        }
    }

    pub fn to_oklch(self) -> Oklch {
        self.to_oklab().to_oklch()
    }

    /// Its OKLab lightness
    pub fn lightness(self) -> f32 {
        self.to_oklab().l
    }
}

impl Srgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn rgb(self) -> Srgb {
        Srgb::new(self.r, self.g, self.b)
    }
}

impl Oklab {
    /// In sRGB, channels possibly outside 0 to 1 (out of gamut)
    pub fn to_srgb_unclamped(self) -> Srgb {
        let (l, a, b) = (f64::from(self.l), f64::from(self.a), f64::from(self.b));
        let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
        let m_ = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
        let s_ = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
        let r = 4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_;
        let g = -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_;
        let b = -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_;
        Srgb::new(encode(r) as f32, encode(g) as f32, encode(b) as f32)
    }

    /// In sRGB, each channel held to 0 to 1
    pub fn to_srgb(self) -> Srgb {
        self.to_srgb_unclamped().clamp()
    }

    pub fn to_oklch(self) -> Oklch {
        let hue = f64::from(self.b).atan2(f64::from(self.a)).to_degrees();
        Oklch { l: self.l, chroma: self.a.hypot(self.b), hue: (if hue < 0.0 { hue + 360.0 } else { hue }) as f32 }
    }

    /// The Euclidean distance between two colours, which OKLab makes about
    /// how different they look
    pub fn distance(self, other: Oklab) -> f32 {
        ((self.l - other.l).powi(2) + (self.a - other.a).powi(2) + (self.b - other.b).powi(2)).sqrt()
    }
}

impl Oklch {
    pub const fn new(l: f32, chroma: f32, hue: f32) -> Self {
        Self { l, chroma, hue }
    }

    pub fn to_oklab(self) -> Oklab {
        let h = f64::from(self.hue).to_radians();
        let c = f64::from(self.chroma);
        Oklab { l: self.l, a: (c * h.cos()) as f32, b: (c * h.sin()) as f32 }
    }

    /// In sRGB, each channel held to 0 to 1
    pub fn to_srgb(self) -> Srgb {
        self.to_oklab().to_srgb()
    }
}

/// sRGB's transfer function, encoded to linear
pub fn decode(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Linear to encoded
pub fn encode(c: f64) -> f64 {
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

impl From<[f32; 3]> for Srgb {
    fn from([r, g, b]: [f32; 3]) -> Self {
        Self::new(r, g, b)
    }
}

impl From<Srgb> for [f32; 3] {
    fn from(c: Srgb) -> Self {
        [c.r, c.g, c.b]
    }
}

impl From<[f32; 4]> for Srgba {
    fn from([r, g, b, a]: [f32; 4]) -> Self {
        Self::new(r, g, b, a)
    }
}

impl From<Srgba> for [f32; 4] {
    fn from(c: Srgba) -> Self {
        [c.r, c.g, c.b, c.a]
    }
}

impl From<Srgb> for Srgba {
    fn from(c: Srgb) -> Self {
        c.with_alpha(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklab_matches_ottossons_reference_values() {
        // White and the sRGB primaries, as CSS Color 4 gives them
        let close = |c: Oklab, want: [f32; 3]| (c.l - want[0]).abs() < 1e-4 && (c.a - want[1]).abs() < 1e-4 && (c.b - want[2]).abs() < 1e-4;
        let white = Srgb::WHITE.to_oklab();
        assert!(close(white, [1.0, 0.0, 0.0]), "{white:?}");
        let red = Srgb::new(1.0, 0.0, 0.0).to_oklab();
        assert!(close(red, [0.62796, 0.22486, 0.12585]), "{red:?}");
        let green = Srgb::new(0.0, 1.0, 0.0).to_oklab();
        assert!(close(green, [0.86644, -0.23389, 0.17950]), "{green:?}");
        let blue = Srgb::new(0.0, 0.0, 1.0).to_oklab();
        assert!(close(blue, [0.45201, -0.03246, -0.31153]), "{blue:?}");
    }

    #[test]
    fn conversions_round_trip() {
        for hex in [0x222222, 0x7f2400, 0x5cd7b0, 0xebebeb, 0x0000ff, 0xffcb6b] {
            let c = Srgb::hex(hex);
            assert_eq!(c.to_oklch().to_srgb().to_rgb8(), c.to_rgb8(), "{hex:06x}");
            assert_eq!(Srgb::parse(&c.to_hex()), Some(c));
        }
        assert_eq!(Srgb::parse("#22222"), None);
        let lch = Srgb::hex(0x0000ff).to_oklch();
        assert!((0.0..360.0).contains(&lch.hue));
    }
}
