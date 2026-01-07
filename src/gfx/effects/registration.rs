//! Effect registration with Direct2D.

use super::{EffectMetadata, PixelShaderEffect};
use windows::Win32::Graphics::Direct2D::{D2D1_PROPERTY_BINDING, ID2D1Factory1};
use windows::core::{GUID, HRESULT, HSTRING};
use windows_core::{IUnknown, OutRef};

/// Wrapper around `D2D1_PROPERTY_BINDING` that is `Sync`.
///
/// This is safe because the property bindings contain static string pointers
/// and function pointers that don't change after initialization.
#[repr(transparent)]
pub struct SyncPropertyBinding(pub D2D1_PROPERTY_BINDING);

// SAFETY: The D2D1_PROPERTY_BINDING struct contains:
// - propertyName: PCWSTR - a static string pointer that doesn't change
// - setFunction/getFunction: function pointers that are inherently thread-safe
// These are all read-only after creation, making it safe to share across threads.
unsafe impl Sync for SyncPropertyBinding {}
unsafe impl Send for SyncPropertyBinding {}

impl SyncPropertyBinding {
    /// Creates a new SyncPropertyBinding.
    pub const fn new(binding: D2D1_PROPERTY_BINDING) -> Self {
        Self(binding)
    }

    /// Returns the inner D2D1_PROPERTY_BINDING.
    pub const fn inner(&self) -> &D2D1_PROPERTY_BINDING {
        &self.0
    }
}

impl std::ops::Deref for SyncPropertyBinding {
    type Target = D2D1_PROPERTY_BINDING;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Registry for custom Direct2D effects.
///
/// Manages effect registration with the Direct2D factory and tracks
/// which effects have been registered to avoid duplicate registration.
#[derive(Default)]
pub struct EffectRegistry {
    registered: std::collections::HashSet<GUID>,
}

impl EffectRegistry {
    /// Creates a new empty effect registry.
    pub fn new() -> Self {
        Self {
            registered: std::collections::HashSet::new(),
        }
    }

    /// Registers a custom effect type with Direct2D.
    ///
    /// This must be called once per effect type before creating instances.
    /// Multiple calls with the same effect type are safely ignored.
    ///
    /// The effect type must have the `#[pixel_shader_effect]` attribute applied,
    /// which generates the necessary COM wrappers and factory function.
    ///
    /// # Arguments
    ///
    /// * `factory` - The Direct2D factory to register with
    ///
    /// # Returns
    ///
    /// Ok(()) if registration succeeded or effect was already registered
    ///
    /// # Example
    ///
    /// ```ignore
    /// registry.register::<GrayscaleEffect>(&factory)?;
    /// ```
    pub fn register<E: PixelShaderEffect + EffectFactory>(
        &mut self,
        factory: &ID2D1Factory1,
    ) -> windows::core::Result<()> {
        if self.registered.contains(&E::CLSID) {
            return Ok(());
        }

        let metadata = E::metadata();

        // Build the XML registration string
        let xml = build_effect_xml::<E>(&metadata);

        // Get property bindings from the effect
        let sync_bindings = E::property_bindings();

        // SAFETY: SyncPropertyBinding is #[repr(transparent)] around D2D1_PROPERTY_BINDING,
        // so we can safely transmute the slice.
        let bindings: &[D2D1_PROPERTY_BINDING] = unsafe { std::mem::transmute(sync_bindings) };

        unsafe {
            factory.RegisterEffectFromString(
                &E::CLSID,
                &HSTRING::from(&xml),
                Some(bindings),
                Some(E::effect_factory()),
            )?;
        }

        self.registered.insert(E::CLSID);
        Ok(())
    }

    /// Checks if an effect type has been registered.
    pub fn is_registered<E: PixelShaderEffect>(&self) -> bool {
        self.registered.contains(&E::CLSID)
    }

    /// Unregisters an effect type from Direct2D.
    ///
    /// Note: This should rarely be needed as effects are typically
    /// registered for the lifetime of the application.
    pub fn unregister<E: PixelShaderEffect>(
        &mut self,
        factory: &ID2D1Factory1,
    ) -> windows::core::Result<()> {
        if !self.registered.contains(&E::CLSID) {
            return Ok(());
        }

        unsafe {
            factory.UnregisterEffect(&E::CLSID)?;
        }

        self.registered.remove(&E::CLSID);
        Ok(())
    }

    /// Clears the registry without unregistering effects.
    ///
    /// This is useful when the factory is being destroyed.
    pub fn clear(&mut self) {
        self.registered.clear();
    }
}

/// Trait for effects that have a generated factory function.
///
/// This trait is automatically implemented by the `#[pixel_shader_effect]` macro.
/// You should not implement this trait manually.
pub trait EffectFactory {
    /// Returns the factory function pointer for registering this effect.
    fn effect_factory() -> unsafe extern "system" fn(OutRef<'_, IUnknown>) -> HRESULT;

    /// Returns the property bindings for this effect.
    fn property_bindings() -> &'static [SyncPropertyBinding];
}

/// Builds the XML registration string for a custom effect.
fn build_effect_xml<E: PixelShaderEffect>(metadata: &EffectMetadata) -> String {
    let inputs = (0..E::INPUT_COUNT)
        .map(|i| format!("<Input name='Source{}'/>", i))
        .collect::<Vec<_>>()
        .join("\n        ");

    let properties = metadata
        .properties
        .iter()
        .map(build_property_xml)
        .collect::<Vec<_>>()
        .join("\n    ");

    format!(
        r#"<?xml version='1.0'?>
<Effect>
    <Property name='DisplayName' type='string' value='{name}'/>
    <Property name='Author' type='string' value='{author}'/>
    <Property name='Category' type='string' value='{category}'/>
    <Property name='Description' type='string' value='{description}'/>
    <Inputs>
        {inputs}
    </Inputs>
    {properties}
</Effect>"#,
        name = metadata.name,
        author = metadata.author,
        category = metadata.category,
        description = metadata.description,
        inputs = inputs,
        properties = properties,
    )
}

/// Builds XML for a single property definition.
fn build_property_xml(prop: &super::PropertyMetadata) -> String {
    let mut xml = format!(
        "<Property name='{}' type='{}'>",
        prop.name,
        prop.property_type.as_str()
    );

    xml.push_str(&format!(
        "\n        <Property name='DisplayName' type='string' value='{}'/>",
        prop.display_name
    ));

    xml.push_str(&format!(
        "\n        <Property name='Default' type='{}' value='{}'/>",
        prop.property_type.as_str(),
        prop.default.to_xml_value()
    ));

    if let Some(min) = &prop.min {
        xml.push_str(&format!(
            "\n        <Property name='Min' type='{}' value='{}'/>",
            prop.property_type.as_str(),
            min.to_xml_value()
        ));
    }

    if let Some(max) = &prop.max {
        xml.push_str(&format!(
            "\n        <Property name='Max' type='{}' value='{}'/>",
            prop.property_type.as_str(),
            max.to_xml_value()
        ));
    }

    xml.push_str("\n    </Property>");
    xml
}
