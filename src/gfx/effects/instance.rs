//! Effect instance creation and management.

use super::{EffectProperty, PixelShaderEffect};
use std::marker::PhantomData;
use windows::Win32::Graphics::Direct2D::{
    D2D1_PROPERTY_TYPE_BOOL, D2D1_PROPERTY_TYPE_FLOAT, D2D1_PROPERTY_TYPE_INT32,
    D2D1_PROPERTY_TYPE_UINT32, D2D1_PROPERTY_TYPE_VECTOR2, D2D1_PROPERTY_TYPE_VECTOR3,
    D2D1_PROPERTY_TYPE_VECTOR4, ID2D1DeviceContext, ID2D1Effect, ID2D1Image,
};

/// A created instance of a custom Direct2D effect.
///
/// This wrapper provides type-safe access to effect creation and property updates.
/// The effect must have been registered via [`EffectRegistry::register`] before
/// creating instances.
///
/// # Example
///
/// ```ignore
/// let instance = EffectInstance::<MyEffect>::create(&device_context)?;
/// instance.set_input(0, &input_image);
/// instance.update(&MyEffect { intensity: 0.5 })?;
/// let output = instance.output()?;
/// device_context.DrawImage(&output, ...);
/// ```
pub struct EffectInstance<E: PixelShaderEffect> {
    effect: ID2D1Effect,
    _marker: PhantomData<E>,
}

impl<E: PixelShaderEffect> EffectInstance<E> {
    /// Creates a new instance of the effect.
    ///
    /// The effect type must have been registered with the factory via
    /// [`EffectRegistry::register`] before calling this.
    ///
    /// # Arguments
    ///
    /// * `context` - The Direct2D device context to create the effect on
    ///
    /// # Returns
    ///
    /// A new effect instance ready for use
    pub fn create(context: &ID2D1DeviceContext) -> windows::core::Result<Self> {
        let effect = unsafe { context.CreateEffect(&E::CLSID)? };

        Ok(Self {
            effect,
            _marker: PhantomData,
        })
    }

    /// Sets an input image for the effect.
    ///
    /// # Arguments
    ///
    /// * `index` - The input index (0 for most single-input effects)
    /// * `image` - The input image to process
    pub fn set_input(&self, index: u32, image: &ID2D1Image) {
        unsafe {
            self.effect.SetInput(index, Some(image), false);
        }
    }

    /// Sets the first input image (convenience method for single-input effects).
    ///
    /// # Arguments
    ///
    /// * `image` - The input image to process
    pub fn set_input_image(&self, image: &ID2D1Image) {
        self.set_input(0, image);
    }

    /// Updates the effect with new property values.
    ///
    /// This reads the properties from the effect struct and applies them
    /// to the Direct2D effect.
    ///
    /// # Arguments
    ///
    /// * `effect_data` - The effect struct containing current property values
    pub fn update(&self, effect_data: &E) -> windows::core::Result<()> {
        for prop in effect_data.properties() {
            self.set_property(prop).unwrap();
        }
        Ok(())
    }

    /// Sets a single property value on the effect.
    fn set_property(&self, prop: EffectProperty) -> windows::core::Result<()> {
        unsafe {
            match prop {
                EffectProperty::Float { index, value } => {
                    self.effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_FLOAT,
                        std::slice::from_raw_parts(
                            &value as *const f32 as *const u8,
                            std::mem::size_of::<f32>(),
                        ),
                    )?;
                }
                EffectProperty::Float2 { index, value } => {
                    self.effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_VECTOR2,
                        std::slice::from_raw_parts(
                            value.as_ptr() as *const u8,
                            std::mem::size_of::<[f32; 2]>(),
                        ),
                    )?;
                }
                EffectProperty::Float3 { index, value } => {
                    self.effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_VECTOR3,
                        std::slice::from_raw_parts(
                            value.as_ptr() as *const u8,
                            std::mem::size_of::<[f32; 3]>(),
                        ),
                    )?;
                }
                EffectProperty::Float4 { index, value } => {
                    self.effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_VECTOR4,
                        std::slice::from_raw_parts(
                            value.as_ptr() as *const u8,
                            std::mem::size_of::<[f32; 4]>(),
                        ),
                    )?;
                }
                EffectProperty::Int { index, value } => {
                    self.effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_INT32,
                        std::slice::from_raw_parts(
                            &value as *const i32 as *const u8,
                            std::mem::size_of::<i32>(),
                        ),
                    )?;
                }
                EffectProperty::UInt { index, value } => {
                    self.effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_UINT32,
                        std::slice::from_raw_parts(
                            &value as *const u32 as *const u8,
                            std::mem::size_of::<u32>(),
                        ),
                    )?;
                }
                EffectProperty::Bool { index, value } => {
                    let bool_val: i32 = if value { 1 } else { 0 };
                    self.effect.SetValue(
                        index,
                        D2D1_PROPERTY_TYPE_BOOL,
                        std::slice::from_raw_parts(
                            &bool_val as *const i32 as *const u8,
                            std::mem::size_of::<i32>(),
                        ),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Gets the output image from the effect.
    ///
    /// This can be used as input to another effect or drawn directly.
    pub fn output(&self) -> windows::core::Result<ID2D1Image> {
        unsafe { self.effect.GetOutput() }
    }

    /// Returns a reference to the underlying Direct2D effect.
    ///
    /// This can be used for advanced operations not covered by this wrapper.
    pub fn as_raw(&self) -> &ID2D1Effect {
        &self.effect
    }

    /// Consumes the wrapper and returns the underlying Direct2D effect.
    pub fn into_raw(self) -> ID2D1Effect {
        self.effect
    }
}

impl<E: PixelShaderEffect> Clone for EffectInstance<E> {
    fn clone(&self) -> Self {
        Self {
            effect: self.effect.clone(),
            _marker: PhantomData,
        }
    }
}

impl<E: PixelShaderEffect> From<EffectInstance<E>> for ID2D1Effect {
    fn from(instance: EffectInstance<E>) -> Self {
        instance.effect
    }
}

impl<E: PixelShaderEffect> AsRef<ID2D1Effect> for EffectInstance<E> {
    fn as_ref(&self) -> &ID2D1Effect {
        &self.effect
    }
}
