//! Built-in example effects demonstrating the custom effects system.
//!
//! These effects serve as examples and can be used directly in applications.
//! To use them, first register the effect and then create instances.
//!
//! # Example
//!
//! ```ignore
//! // Register at startup (done automatically by Application)
//! device_resources.register_effect::<GrayscaleEffect>()?;
//!
//! // Use during rendering via BackdropFilter
//! Element {
//!     backdrop_filter: Some(BackdropFilter::grayscale(0.8)),
//!     ..Default::default()
//! }
//! ```
//!
//! # Compiling Shaders
//!
//! The HLSL source files for these effects are in `src/gfx/effects/shaders/`.
//! They are automatically compiled by the build script when changed.

use raxis_proc_macro::pixel_shader_effect;

/// Applies a box blur effect to an image.
///
/// This is a custom blur implementation that demonstrates multi-sample
/// shader techniques. For production use, consider the built-in D2D
/// Gaussian blur effect which is hardware-optimized.
///
/// # Properties
///
/// * `radius` - Blur radius in pixels (1-8, higher = more blur)
/// * `intensity` - Blend factor between original (0.0) and blurred (1.0)
///
/// # Shader
///
/// Uses a box blur kernel that samples a square grid of pixels and
/// averages them together. The kernel size is (2*radius+1) x (2*radius+1).
#[pixel_shader_effect(
    clsid = "1793FB86-6351-43D8-B857-C8DC02C4EB7A",
    name = "BoxBlur",
    author = "raxis",
    category = "Blur",
    description = "Custom box blur effect with adjustable radius and intensity",
    shader = "shaders/box_blur.cso",
    input_padding = "self.radius"
)]
#[derive(Debug, Clone, Copy)]
pub struct BoxBlurEffect {
    /// Blur radius in pixels: 1-8, higher values create a stronger blur
    #[property(min = 0.0, max = 8.0, default = 3.0)]
    pub radius: f32,
    /// Blend factor: 0.0 = original image, 1.0 = full blur effect
    #[property(min = 0.0, max = 1.0, default = 1.0)]
    pub intensity: f32,
}

impl Default for BoxBlurEffect {
    fn default() -> Self {
        Self {
            radius: 3.0,
            intensity: 1.0,
        }
    }
}

/// Applies a liquid glass distortion effect.
///
/// Creates a glass-like warping effect using a superellipse shape with
/// configurable refraction, glow, and noise. Ported from https://github.com/OverShifted/LiquidGlass
///
/// # Properties
///
/// * `refraction` - Refraction power controlling distortion strength (1.0-5.0)
/// * `glow` - Glow intensity around the distortion (0.0-1.0)
/// * `noise` - Amount of noise/grain to add (0.0-0.2)
/// * `size` - Size of the effect as fraction of element (0.1-1.0)
///
/// # Shader
///
/// Uses a superellipse signed distance field to create smooth distortion
/// with an angular glow effect.
#[pixel_shader_effect(
    clsid = "2A8C1690-CC5C-4A26-BB5A-94FA9D8C5C51",
    name = "LiquidGlass",
    author = "raxis",
    category = "Distortion",
    description = "Liquid glass distortion effect with glow",
    shader = "shaders/liquid_glass.cso"
)]
#[derive(Debug, Clone, Copy)]
pub struct LiquidGlassEffect {
    /// Refraction power: 1.0 = subtle, 5.0 = extreme distortion
    #[property(min = 1.0, max = 5.0, default = 3.0)]
    pub refraction: f32,
    /// Glow intensity: 0.0 = no glow, 1.0 = strong glow
    #[property(min = 0.0, max = 1.0, default = 0.35)]
    pub glow: f32,
    /// Noise amount: 0.0 = clean, 0.2 = grainy
    #[property(min = 0.0, max = 0.2, default = 0.06)]
    pub noise: f32,
    /// Effect size as fraction of element: 0.1 = small, 1.0 = fills element
    #[property(min = 0.1, max = 1.0, default = 1.0)]
    pub size: f32,
}

impl Default for LiquidGlassEffect {
    fn default() -> Self {
        Self {
            refraction: 3.0,
            glow: 0.35,
            noise: 0.06,
            size: 0.5,
        }
    }
}
