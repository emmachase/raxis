use std::path::Path;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_FAMILY_MODEL_TYPOGRAPHIC, DWRITE_LINE_SPACING,
    DWRITE_LINE_SPACING_METHOD_PROPORTIONAL, IDWriteFactory6, IDWriteFontCollection,
    IDWriteFontSetBuilder1, IDWriteInMemoryFontFileLoader, IDWriteTextFormat3,
    DWRITE_FONT_AXIS_VALUE, DWRITE_FONT_AXIS_TAG_WEIGHT, DWRITE_FONT_AXIS_TAG_WIDTH,
    DWRITE_FONT_AXIS_TAG_ITALIC, DWRITE_FONT_AXIS_TAG_SLANT,
};
use windows::core::{PCWSTR, Result};

/// Identifies a font that can be either a system font or a custom loaded font
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FontIdentifier {
    /// System font identified by family name (e.g., "Segoe UI", "Arial")
    System(String),
    /// Custom font identified by a unique name
    Custom(String),
}

impl FontIdentifier {
    /// Create a system font identifier
    pub fn system(family_name: impl Into<String>) -> Self {
        Self::System(family_name.into())
    }

    /// Create a custom font identifier
    pub fn custom(name: impl Into<String>) -> Self {
        Self::Custom(name.into())
    }

    /// Get the family name for font creation
    pub fn family_name(&self) -> &str {
        match self {
            FontIdentifier::System(name) => name,
            FontIdentifier::Custom(name) => name,
        }
    }
}

impl From<&str> for FontIdentifier {
    fn from(family_name: &str) -> Self {
        Self::System(family_name.to_string())
    }
}

impl From<String> for FontIdentifier {
    fn from(family_name: String) -> Self {
        Self::System(family_name)
    }
}

/// Manages custom font loading and provides unified font access
pub struct FontManager {
    dwrite_factory: IDWriteFactory6,
    custom_font_collection: Option<IDWriteFontCollection>,
    font_set_builder: Option<IDWriteFontSetBuilder1>,
    memory_font_loader: Option<IDWriteInMemoryFontFileLoader>,
}

/// Font weight values (100-900, where 400 is normal, 700 is bold)
#[derive(Debug, Clone, Copy, PartialEq)]
#[derive(Default)]
pub enum FontWeight {
    Thin,        // 100
    ExtraLight,  // 200
    Light,       // 300
    #[default]
    Normal,      // 400
    Medium,      // 500
    SemiBold,    // 600
    Bold,        // 700
    ExtraBold,   // 800
    Black,       // 900
    Custom(f32), // Custom weight value
}

impl FontWeight {
    pub fn value(&self) -> f32 {
        match self {
            FontWeight::Thin => 100.0,
            FontWeight::ExtraLight => 200.0,
            FontWeight::Light => 300.0,
            FontWeight::Normal => 400.0,
            FontWeight::Medium => 500.0,
            FontWeight::SemiBold => 600.0,
            FontWeight::Bold => 700.0,
            FontWeight::ExtraBold => 800.0,
            FontWeight::Black => 900.0,
            FontWeight::Custom(value) => *value,
        }
    }
}


/// Font style (italic/oblique)
#[derive(Debug, Clone, Copy, PartialEq)]
#[derive(Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique(f32), // Custom oblique angle
}

impl FontStyle {
    pub fn italic_value(&self) -> f32 {
        match self {
            FontStyle::Normal => 0.0,
            FontStyle::Italic => 1.0,
            FontStyle::Oblique(_) => 1.0,
        }
    }
    
    pub fn slant_value(&self) -> f32 {
        match self {
            FontStyle::Normal => 0.0,
            FontStyle::Italic => -20.0, // Standard italic slant
            FontStyle::Oblique(angle) => *angle,
        }
    }
}


/// Font width/stretch values
#[derive(Debug, Clone, Copy, PartialEq)]
#[derive(Default)]
pub enum FontWidth {
    UltraCondensed, // 50%
    ExtraCondensed, // 62.5%
    Condensed,      // 75%
    SemiCondensed,  // 87.5%
    #[default]
    Normal,         // 100%
    SemiExpanded,   // 112.5%
    Expanded,       // 125%
    ExtraExpanded,  // 150%
    UltraExpanded,  // 200%
    Custom(f32),    // Custom width percentage
}

impl FontWidth {
    pub fn value(&self) -> f32 {
        match self {
            FontWidth::UltraCondensed => 50.0,
            FontWidth::ExtraCondensed => 62.5,
            FontWidth::Condensed => 75.0,
            FontWidth::SemiCondensed => 87.5,
            FontWidth::Normal => 100.0,
            FontWidth::SemiExpanded => 112.5,
            FontWidth::Expanded => 125.0,
            FontWidth::ExtraExpanded => 150.0,
            FontWidth::UltraExpanded => 200.0,
            FontWidth::Custom(value) => *value,
        }
    }
}


/// Collection of font axes for variable font support
#[derive(Debug, Clone, Copy, PartialEq)]
#[derive(Default)]
pub struct FontAxes {
    pub weight: FontWeight,
    pub style: FontStyle,
    pub width: FontWidth,
}


impl FontAxes {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }
    
    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }
    
    pub fn with_width(mut self, width: FontWidth) -> Self {
        self.width = width;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineSpacing {
    pub height: f32,
    pub baseline: f32,
}

impl LineSpacing {
    pub fn new(height: f32, baseline: f32) -> Self {
        Self { height, baseline }
    }

    pub fn of_height(height: f32) -> Self {
        Self {
            height,
            baseline: 1.0,
        }
    }

    pub fn of_baseline(baseline: f32) -> Self {
        Self {
            height: 1.0,
            baseline,
        }
    }
}

impl Default for LineSpacing {
    fn default() -> Self {
        Self {
            height: 1.0,
            baseline: 1.0,
        }
    }
}

impl FontManager {
    /// Create a new FontManager
    pub fn new(dwrite_factory: IDWriteFactory6) -> Result<Self> {
        let font_set_builder = unsafe { dwrite_factory.CreateFontSetBuilder()?.into() };

        // Create the in-memory font file loader
        let memory_font_loader = unsafe { dwrite_factory.CreateInMemoryFontFileLoader()? };
        unsafe { dwrite_factory.RegisterFontFileLoader(&memory_font_loader)? };

        Ok(Self {
            dwrite_factory,
            custom_font_collection: None,
            font_set_builder: Some(font_set_builder),
            memory_font_loader: Some(memory_font_loader),
        })
    }

    /// Load a custom font from a file path
    pub fn load_font_from_file<P: AsRef<Path>>(&mut self, font_path: P) -> Result<()> {
        let path = font_path.as_ref();

        // Convert path to wide string
        let path_str = path.to_string_lossy();
        let path_wide: Vec<u16> = path_str.encode_utf16().chain(Some(0)).collect();

        unsafe {
            // Create font file reference
            let font_file = self
                .dwrite_factory
                .CreateFontFileReference(PCWSTR(path_wide.as_ptr()), None)?;

            // Add to font set builder if available
            if let Some(builder) = &self.font_set_builder {
                builder.AddFontFile(&font_file)?;
            }

            // Rebuild custom font collection
            self.rebuild_font_collection()?;

            Ok(())
        }
    }

    /// Load a custom font from in-memory data
    pub fn load_font_from_memory(&mut self, font_data: &'static [u8]) -> Result<()> {
        if let Some(memory_loader) = &self.memory_font_loader {
            unsafe {
                // Create font file reference from memory
                let font_file = memory_loader.CreateInMemoryFontFileReference(
                    &self.dwrite_factory,
                    font_data.as_ptr() as *const _,
                    font_data.len() as u32,
                    None, // No owner object, loader will copy the data
                )?;

                // Add to font set builder
                if let Some(builder) = &self.font_set_builder {
                    builder.AddFontFile(&font_file)?;
                }

                // Rebuild custom font collection
                self.rebuild_font_collection()?;

                Ok(())
            }
        } else {
            Err(windows::core::Error::from_win32())
        }
    }

    /// Load multiple fonts from a directory
    pub fn load_fonts_from_directory<P: AsRef<Path>>(&mut self, directory: P) -> Result<()> {
        let dir = directory.as_ref();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(extension) = path.extension() {
                    let ext = extension.to_string_lossy().to_lowercase();
                    if matches!(ext.as_str(), "ttf" | "otf" | "woff" | "woff2") {
                        self.load_font_from_file(&path)
                            .expect("Failed to load font");
                    }
                }
            }
        }

        Ok(())
    }

    /// Create a text format using either system or custom fonts
    pub fn create_text_format(
        &self,
        font_id: &FontIdentifier,
        font_size: f32,
        font_axes: FontAxes,
        line_spacing: Option<LineSpacing>,
        locale: &str,
    ) -> Result<IDWriteTextFormat3> {
        let family_name = font_id.family_name();
        let family_name_wide: Vec<u16> = family_name.encode_utf16().chain(Some(0)).collect();
        let locale_wide: Vec<u16> = locale.encode_utf16().chain(Some(0)).collect();

        unsafe {
            let font_collection = match font_id {
                FontIdentifier::Custom(_) => self.custom_font_collection.as_ref(),
                FontIdentifier::System(_) => None, // Use system collection (default)
            };

            let font_axis_values = [
                DWRITE_FONT_AXIS_VALUE {
                    axisTag: DWRITE_FONT_AXIS_TAG_WEIGHT,
                    value: font_axes.weight.value(),
                },
                DWRITE_FONT_AXIS_VALUE {
                    axisTag: DWRITE_FONT_AXIS_TAG_WIDTH,
                    value: font_axes.width.value(),
                },
                DWRITE_FONT_AXIS_VALUE {
                    axisTag: DWRITE_FONT_AXIS_TAG_ITALIC,
                    value: font_axes.style.italic_value(),
                },
                DWRITE_FONT_AXIS_VALUE {
                    axisTag: DWRITE_FONT_AXIS_TAG_SLANT,
                    value: font_axes.style.slant_value(),
                },
            ];

            let format = self.dwrite_factory.CreateTextFormat(
                PCWSTR(family_name_wide.as_ptr()),
                font_collection,
                &font_axis_values,
                font_size,
                PCWSTR(locale_wide.as_ptr()),
            )?;

            if let Some(line_spacing) = line_spacing {
                let line_spacing = DWRITE_LINE_SPACING {
                    method: DWRITE_LINE_SPACING_METHOD_PROPORTIONAL,
                    height: line_spacing.height,
                    baseline: line_spacing.baseline,
                    ..Default::default()
                };
                format.SetLineSpacing(&line_spacing)?;
            }

            Ok(format)
        }
    }

    /// Rebuild the custom font collection from loaded fonts
    fn rebuild_font_collection(&mut self) -> Result<()> {
        if let Some(builder) = &self.font_set_builder {
            unsafe {
                let font_set = builder.CreateFontSet()?;
                self.custom_font_collection = Some(
                    self.dwrite_factory
                        .CreateFontCollectionFromFontSet(
                            &font_set,
                            // DWRITE_FONT_FAMILY_MODEL_WEIGHT_STRETCH_STYLE,
                            DWRITE_FONT_FAMILY_MODEL_TYPOGRAPHIC,
                        )?
                        .into(),
                );
                println!("Rebuilt custom font collection");
            }
        }
        Ok(())
    }
}

/// Global font manager instance access
pub struct GlobalFontManager;

impl GlobalFontManager {
    /// Initialize the global font manager
    pub fn initialize(dwrite_factory: IDWriteFactory6) -> Result<()> {
        let font_manager = FontManager::new(dwrite_factory)?;
        FONT_MANAGER
            .set(std::sync::Mutex::new(font_manager))
            .map_err(|_| windows::core::Error::from_win32())?;
        Ok(())
    }

    /// Access the global font manager
    pub fn with<F, R>(f: F) -> R
    where
        F: FnOnce(&mut FontManager) -> R,
    {
        if let Some(manager) = FONT_MANAGER.get() {
            let mut guard = manager.lock().unwrap();
            f(&mut guard)
        } else {
            panic!("FontManager not initialized. Call GlobalFontManager::initialize() first.");
        }
    }

    /// Load a font and return its identifier
    pub fn load_font_from_file<P: AsRef<Path>>(font_path: P) -> Result<()> {
        Self::with(|manager| manager.load_font_from_file(font_path))
    }

    /// Load a font from in-memory data and return its identifier
    pub fn load_font_from_memory(font_data: &'static [u8]) -> Result<()> {
        Self::with(|manager| manager.load_font_from_memory(font_data))
    }

    /// Create a text format using the global font manager
    pub fn create_text_format(
        font_id: &FontIdentifier,
        font_size: f32,
        font_axes: FontAxes,
        line_spacing: Option<LineSpacing>,
        locale: &str,
    ) -> Result<IDWriteTextFormat3> {
        Self::with(|manager| manager.create_text_format(font_id, font_size, font_axes, line_spacing, locale))
    }
}

static FONT_MANAGER: std::sync::OnceLock<std::sync::Mutex<FontManager>> =
    std::sync::OnceLock::new();
