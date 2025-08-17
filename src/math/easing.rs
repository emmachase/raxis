// Phase easing for grow/shrink interpolation.
#[derive(Copy, Clone, Debug, Default)]
pub enum Easing {
    Linear,
    #[default]
    EaseInOut, // cosine-based smoothstep
    EaseIn,
    EaseOut,
    EaseInOutQuad,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutCubic,
    EaseInCubic,
    EaseOutCubic,
}

impl Easing {
    pub fn apply(self, p: f32) -> f32 {
        match self {
            Easing::Linear => p,
            // 0.5 - 0.5*cos(pi*p): smooth in/out
            Easing::EaseInOut => 0.5 - 0.5 * (core::f32::consts::PI * p).cos(),
            // quadratic ease-in/out variants
            Easing::EaseIn => p * p,
            Easing::EaseOut => 1.0 - (1.0 - p) * (1.0 - p),
            // quadratic ease-in/out variants
            Easing::EaseInOutQuad => p * p * (3.0 - 2.0 * p),
            Easing::EaseInQuad => p * p,
            Easing::EaseOutQuad => 1.0 - (1.0 - p) * (1.0 - p),
            // cubic ease-in/out variants
            Easing::EaseInOutCubic => p * p * (3.0 - 2.0 * p),
            Easing::EaseInCubic => p * p * p,
            Easing::EaseOutCubic => 1.0 - (1.0 - p) * (1.0 - p) * (1.0 - p),
        }
    }
}
