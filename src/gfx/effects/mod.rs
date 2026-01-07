//! Custom Direct2D pixel shader effects system.
//!
//! This module provides a Rust-idiomatic interface for creating and using
//! custom Direct2D effects with arbitrary HLSL pixel shaders.
//!
//! # Overview
//!
//! To create a custom effect:
//!
//! 1. Write an HLSL pixel shader and compile it to a `.cso` file
//! 2. Define a struct implementing [`PixelShaderEffect`]
//! 3. Register the effect with [`EffectRegistry::register`]
//! 4. Create instances with [`EffectInstance::create`]
//!
//! # Example
//!
//! ```ignore
//! // Define the effect
//! pub struct GrayscaleEffect {
//!     pub intensity: f32,
//! }
//!
//! impl PixelShaderEffect for GrayscaleEffect {
//!     const CLSID: GUID = GUID::from_u128(0x...);
//!     const INPUT_COUNT: u32 = 1;
//!
//!     fn metadata() -> EffectMetadata {
//!         EffectMetadata {
//!             name: "GrayscaleEffect",
//!             author: "raxis",
//!             category: "Color",
//!             description: "Converts image to grayscale",
//!             shader_bytecode: include_bytes!("grayscale.cso"),
//!         }
//!     }
//!
//!     fn properties(&self) -> Vec<EffectProperty> {
//!         vec![EffectProperty::Float { index: 0, value: self.intensity }]
//!     }
//! }
//!
//! // Register once at startup
//! effect_registry.register::<GrayscaleEffect>(&factory)?;
//!
//! // Use during rendering
//! let instance = EffectInstance::<GrayscaleEffect>::create(&device_context)?;
//! instance.set_input(&input_image);
//! instance.update(&GrayscaleEffect { intensity: 1.0 })?;
//! device_context.DrawImage(&instance.output(), ...);
//! ```

pub mod builtins;
mod instance;
mod registration;
mod traits;

pub use instance::EffectInstance;
pub use registration::{EffectFactory, EffectRegistry, SyncPropertyBinding};
pub use traits::{
    DynPixelShaderEffect, EffectMetadata, EffectProperty, PixelShaderEffect, PropertyDefault,
    PropertyMetadata, PropertyType,
};
