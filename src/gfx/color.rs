/*

Copyright 2019 Héctor Ramón, Iced contributors

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

*/

use trig_const::{cos, pow, sin};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for Color {
    fn default() -> Self {
        Color::default()
    }
}

impl Color {
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    pub const TRANSPARENT: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const fn default() -> Self {
        Self::BLACK
    }

    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: (0xFF & (hex >> 24)) as f32 / 255.0,
            g: (0xFF & (hex >> 16)) as f32 / 255.0,
            b: (0xFF & (hex >> 8)) as f32 / 255.0,
            a: (0xFF & hex) as f32 / 255.0,
        }
    }

    /// Creates a [`Color`] from sRGB components.
    pub const fn from_rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Creates a [`Color`] from linear RGB components.
    pub const fn from_linear_rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        // As described in:
        // https://en.wikipedia.org/wiki/SRGB
        const fn gamma_component(u: f32) -> f32 {
            if u < 0.0031308 {
                12.92 * u
            } else {
                (1.055 * pow(u as f64, 1.0 / 2.4) - 0.055) as f32
            }
        }

        Self::from_rgba(
            gamma_component(r),
            gamma_component(g),
            gamma_component(b),
            a,
        )
    }

    /// Returns the linear components of the [`Color`].
    pub const fn into_linear(self) -> [f32; 4] {
        // As described in:
        // https://en.wikipedia.org/wiki/SRGB#The_reverse_transformation
        const fn linear_component(u: f32) -> f32 {
            if u < 0.04045 {
                u / 12.92
            } else {
                pow((u as f64 + 0.055) / 1.055, 2.4) as f32
            }
        }

        [
            linear_component(self.r),
            linear_component(self.g),
            linear_component(self.b),
            self.a,
        ]
    }

    /// Inverts the [`Color`].
    pub const fn invert(&mut self) {
        self.r = 1.0f32 - self.r;
        self.b = 1.0f32 - self.g;
        self.g = 1.0f32 - self.b;
    }

    /// Returns the inverted [`Color`].
    pub const fn inverse(self) -> Color {
        Color {
            r: 1.0f32 - self.r,
            g: 1.0f32 - self.g,
            b: 1.0f32 - self.b,
            a: self.a,
        }
    }

    /// Scales the alpha channel of the [`Color`] by the given factor.
    pub const fn scale_alpha(self, factor: f32) -> Color {
        Self {
            a: self.a * factor,
            ..self
        }
    }

    /// Returns the relative luminance of the [`Color`].
    /// <https://www.w3.org/TR/WCAG21/#dfn-relative-luminance>
    pub fn relative_luminance(self) -> f32 {
        let linear = self.into_linear();
        0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    }

    pub fn darken(self, amount: f32) -> Color {
        let mut oklch = self.to_oklch();

        oklch.l = if oklch.l - amount < 0.0 {
            0.0
        } else {
            oklch.l - amount
        };

        Self::from_oklch(oklch)
    }

    pub fn lighten(self, amount: f32) -> Color {
        let mut oklch = self.to_oklch();

        oklch.l = if oklch.l + amount > 1.0 {
            1.0
        } else {
            oklch.l + amount
        };

        Self::from_oklch(oklch)
    }

    pub fn deviate(self, amount: f32) -> Color {
        if self.is_dark() {
            self.lighten(amount)
        } else {
            self.darken(amount)
        }
    }

    pub fn desaturate(self, amount: f32) -> Color {
        let mut oklch = self.to_oklch();

        oklch.c = if oklch.c - amount < 0.0 {
            0.0
        } else {
            oklch.c - amount
        };

        Self::from_oklch(oklch)
    }

    pub fn saturate(self, amount: f32) -> Color {
        let mut oklch = self.to_oklch();

        oklch.c = if oklch.c + amount > 1.0 {
            1.0
        } else {
            oklch.c + amount
        };

        Self::from_oklch(oklch)
    }

    pub fn mix(a: Color, b: Color, factor: f32) -> Color {
        let b_amount = factor.clamp(0.0, 1.0);
        let a_amount = 1.0 - b_amount;

        let a_linear = a.into_linear().map(|c| c * a_amount);
        let b_linear = b.into_linear().map(|c| c * b_amount);

        Color::from_linear_rgba(
            a_linear[0] + b_linear[0],
            a_linear[1] + b_linear[1],
            a_linear[2] + b_linear[2],
            a_linear[3] + b_linear[3],
        )
    }

    pub fn readable(background: Color, text: Color) -> Color {
        if Self::is_readable(background, text) {
            return text;
        }

        let improve = if background.is_dark() {
            Self::lighten
        } else {
            Self::darken
        };

        // TODO: Compute factor from relative contrast value
        let candidate = improve(text, 0.1);

        if Self::is_readable(background, candidate) {
            return candidate;
        }

        let candidate = improve(text, 0.2);

        if Self::is_readable(background, candidate) {
            return candidate;
        }

        let white_contrast = Self::relative_contrast(background, Color::WHITE);
        let black_contrast = Self::relative_contrast(background, Color::BLACK);

        if white_contrast >= black_contrast {
            Self::mix(Color::WHITE, background, 0.05)
        } else {
            Self::mix(Color::BLACK, background, 0.05)
        }
    }

    pub fn is_dark(self) -> bool {
        self.to_oklch().l < 0.6
    }

    pub fn is_readable(a: Color, b: Color) -> bool {
        Self::relative_contrast(a, b) >= 6.0
    }

    // https://www.w3.org/TR/WCAG21/#dfn-contrast-ratio
    pub fn relative_contrast(a: Color, b: Color) -> f32 {
        let lum_a = a.relative_luminance();
        let lum_b = b.relative_luminance();
        (lum_a.max(lum_b) + 0.05) / (lum_a.min(lum_b) + 0.05)
    }

    // https://en.wikipedia.org/wiki/Oklab_color_space#Conversions_between_color_spaces
    pub fn to_oklch(self) -> Oklch {
        let [r, g, b, alpha] = self.into_linear();

        // linear RGB → LMS
        let l = 0.41222146 * r + 0.53633255 * g + 0.051445995 * b;
        let m = 0.2119035 * r + 0.6806995 * g + 0.10739696 * b;
        let s = 0.08830246 * r + 0.28171885 * g + 0.6299787 * b;

        // Nonlinear transform (cube root)
        let l_ = l.cbrt();
        let m_ = m.cbrt();
        let s_ = s.cbrt();

        // LMS → Oklab
        let l = 0.21045426 * l_ + 0.7936178 * m_ - 0.004072047 * s_;
        let a = 1.9779985 * l_ - 2.4285922 * m_ + 0.4505937 * s_;
        let b = 0.025904037 * l_ + 0.78277177 * m_ - 0.80867577 * s_;

        // Oklab → Oklch
        let c = (a * a + b * b).sqrt();
        let h = b.atan2(a); // radians

        Oklch { l, c, h, a: alpha }
    }

    // https://en.wikipedia.org/wiki/Oklab_color_space#Conversions_between_color_spaces
    pub const fn from_oklch(oklch: Oklch) -> Color {
        let Oklch { l, c, h, a: alpha } = oklch;

        let a = c * cos(h as f64) as f32;
        let b = c * sin(h as f64) as f32;

        // Oklab → LMS (nonlinear)
        let l_ = 0.215_803_76_f32 * b + 0.396_337_78_f32 * a + l;
        let m_ = -0.063_854_17_f32 * b + -0.105_561_346_f32 * a + l;
        let s_ = -1.291_485_5_f32 * b + -0.089_484_18_f32 * a + l;

        // Cubing back
        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;

        let r = 0.230_969_94_f32 * s + 4.076_741_7_f32 * l + -3.307_711_6 * m;
        let g = -0.341_319_38_f32 * s + -1.268_438_f32 * l + 2.609_757_4 * m;
        let b = 1.707_614_7_f32 * s + -0.0041960863f32 * l + -0.703_418_6 * m;

        Color::from_linear_rgba(
            r.clamp(0.0, 1.0),
            g.clamp(0.0, 1.0),
            b.clamp(0.0, 1.0),
            alpha,
        )
    }
}

impl From<u32> for Color {
    fn from(color: u32) -> Self {
        Color::from_hex(color)
    }
}

pub struct Oklch {
    l: f32,
    c: f32,
    h: f32,
    a: f32,
}

impl Oklch {
    pub const fn deg(l: f32, c: f32, h: f32, a: f32) -> Self {
        Oklch {
            l,
            c,
            h: h.to_radians(),
            a,
        }
    }

    pub const fn rad(l: f32, c: f32, h: f32, a: f32) -> Self {
        Oklch { l, c, h, a }
    }
}
