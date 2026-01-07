//! Trait definitions for custom Direct2D pixel shader effects.

use windows::core::GUID;

/// Metadata describing a custom effect for registration with Direct2D.
#[derive(Debug, Clone)]
pub struct EffectMetadata {
    /// Human-readable name of the effect
    pub name: &'static str,
    /// Author of the effect
    pub author: &'static str,
    /// Category for organization (e.g., "Color", "Blur", "Distortion")
    pub category: &'static str,
    /// Description of what the effect does
    pub description: &'static str,
    /// Compiled HLSL pixel shader bytecode (.cso file contents)
    pub shader_bytecode: &'static [u8],
    /// Property definitions for registration with Direct2D
    pub properties: &'static [PropertyMetadata],
}

/// Metadata describing a single effect property for XML registration.
#[derive(Debug, Clone, Copy)]
pub struct PropertyMetadata {
    /// Property name (must match what shader expects)
    pub name: &'static str,
    /// Human-readable display name
    pub display_name: &'static str,
    /// Property type for D2D
    pub property_type: PropertyType,
    /// Default value
    pub default: PropertyDefault,
    /// Optional minimum value
    pub min: Option<PropertyDefault>,
    /// Optional maximum value
    pub max: Option<PropertyDefault>,
}

/// Direct2D property types for XML registration.
#[derive(Debug, Clone, Copy)]
pub enum PropertyType {
    Float,
    Int,
    UInt,
    Bool,
    Vector2,
    Vector3,
    Vector4,
}

impl PropertyType {
    /// Returns the D2D XML type string.
    pub fn as_str(&self) -> &'static str {
        match self {
            PropertyType::Float => "float",
            PropertyType::Int => "int32",
            PropertyType::UInt => "uint32",
            PropertyType::Bool => "bool",
            PropertyType::Vector2 => "vector2",
            PropertyType::Vector3 => "vector3",
            PropertyType::Vector4 => "vector4",
        }
    }
}

/// Default/min/max values for property metadata.
#[derive(Debug, Clone, Copy)]
pub enum PropertyDefault {
    Float(f32),
    Int(i32),
    UInt(u32),
    Bool(bool),
    Vector2([f32; 2]),
    Vector3([f32; 3]),
    Vector4([f32; 4]),
}

impl PropertyDefault {
    /// Formats the value for D2D XML.
    pub fn to_xml_value(&self) -> String {
        match self {
            PropertyDefault::Float(v) => format!("{}", v),
            PropertyDefault::Int(v) => format!("{}", v),
            PropertyDefault::UInt(v) => format!("{}", v),
            PropertyDefault::Bool(v) => {
                if *v {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            PropertyDefault::Vector2([x, y]) => format!("({}, {})", x, y),
            PropertyDefault::Vector3([x, y, z]) => format!("({}, {}, {})", x, y, z),
            PropertyDefault::Vector4([x, y, z, w]) => format!("({}, {}, {}, {})", x, y, z, w),
        }
    }
}

/// A property value that can be passed to a Direct2D effect.
#[derive(Debug, Clone, Copy)]
pub enum EffectProperty {
    /// Single floating-point value
    Float { index: u32, value: f32 },
    /// Two-component float vector
    Float2 { index: u32, value: [f32; 2] },
    /// Three-component float vector
    Float3 { index: u32, value: [f32; 3] },
    /// Four-component float vector (e.g., color)
    Float4 { index: u32, value: [f32; 4] },
    /// Integer value
    Int { index: u32, value: i32 },
    /// Boolean value
    Bool { index: u32, value: bool },
    /// Unsigned integer value
    UInt { index: u32, value: u32 },
}

/// Object-safe trait for pixel shader effects that can be used with `dyn`.
///
/// This trait provides runtime access to effect properties and is automatically
/// implemented for all types implementing [`PixelShaderEffect`].
pub trait DynPixelShaderEffect: Send + Sync {
    /// Returns the CLSID for this effect.
    fn clsid(&self) -> GUID;

    /// Returns the current property values to apply to the effect.
    fn properties(&self) -> Vec<EffectProperty>;

    /// Returns the amount of padding (in pixels) needed around the input image.
    ///
    /// Effects that sample neighboring pixels (like blur) need a larger input
    /// image to avoid edge artifacts. This method returns how many extra pixels
    /// are needed on each side of the output region.
    ///
    /// For example, a box blur with radius 4 samples up to 4 pixels in each
    /// direction, so it needs 4 pixels of padding on each edge.
    fn input_padding(&self) -> f32;
}

/// Trait for defining custom Direct2D pixel shader effects.
///
/// Implement this trait to define a custom effect that can be registered
/// with Direct2D and used during rendering.
///
/// # Example
///
/// ```ignore
/// pub struct SepiaEffect {
///     pub intensity: f32,
/// }
///
/// impl PixelShaderEffect for SepiaEffect {
///     const CLSID: GUID = GUID::from_u128(0x12345678_1234_1234_1234_123456789abc);
///     const INPUT_COUNT: u32 = 1;
///
///     fn metadata() -> EffectMetadata {
///         EffectMetadata {
///             name: "SepiaEffect",
///             author: "raxis",
///             category: "Color",
///             description: "Applies sepia tone to an image",
///             shader_bytecode: include_bytes!("shaders/sepia.cso"),
///         }
///     }
///
///     fn properties(&self) -> Vec<EffectProperty> {
///         vec![EffectProperty::Float { index: 0, value: self.intensity }]
///     }
/// }
/// ```
pub trait PixelShaderEffect: Send + Sync + 'static {
    /// Unique identifier (CLSID) for this effect type.
    /// Must be globally unique - use a GUID generator to create this.
    const CLSID: GUID;

    /// Number of input images this effect accepts.
    /// Most effects use 1 input, but some (like blend/composite) may use 2+.
    const INPUT_COUNT: u32 = 1;

    /// Returns metadata describing this effect for registration.
    fn metadata() -> EffectMetadata
    where
        Self: Sized;

    /// Returns the current property values to apply to the effect.
    /// Property indices must match the constant buffer layout in the HLSL shader.
    fn properties(&self) -> Vec<EffectProperty>;

    /// Returns the amount of padding (in pixels) needed around the input image.
    ///
    /// Override this for effects that sample neighboring pixels (like blur).
    /// The default implementation returns 0.0 (no padding needed).
    fn input_padding(&self) -> f32 {
        0.0
    }
}

/// Blanket implementation of DynPixelShaderEffect for all PixelShaderEffect types.
impl<T: PixelShaderEffect> DynPixelShaderEffect for T {
    fn clsid(&self) -> GUID {
        T::CLSID
    }

    fn properties(&self) -> Vec<EffectProperty> {
        PixelShaderEffect::properties(self)
    }

    fn input_padding(&self) -> f32 {
        PixelShaderEffect::input_padding(self)
    }
}
