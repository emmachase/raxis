use crate::{
    Shell,
    gfx::command_recorder::CommandRecorder,
    layout::{
        UIArenas,
        model::{Element, ElementStyle},
    },
    runtime::DeviceResources,
    util::str::StableString,
    widgets::{Bounds, Cursor, Event, Instance, State, Widget, limit_response, widget},
    with_state,
};
use std::{any::Any, time::Instant};
use windows::Win32::{
    Foundation::{GENERIC_READ, HWND},
    Graphics::{
        Direct2D::{ID2D1Bitmap, ID2D1DeviceContext6},
        Imaging::{
            CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICFormatConverter,
            IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
            WICDecodeMetadataCacheOnDemand,
        },
    },
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
};

/// Image widget for displaying bitmap images using WIC
#[derive(Debug)]
pub struct Image {
    /// Path to the image file
    image_path: StableString,
    /// Width for layout calculations
    width: Option<f32>,
    /// Height for layout calculations  
    height: Option<f32>,
    /// Opacity for rendering (0.0 - 1.0)
    opacity: f32,
}

/// State for Image widget that caches the WIC bitmap and D2D bitmap
struct ImageWidgetState {
    /// Device context for creating D2D bitmap
    device_context: ID2D1DeviceContext6,
    /// Cached D2D bitmap
    d2d_bitmap: Option<ID2D1Bitmap>,
    /// Cached image path to detect changes
    cached_image_path: String,
    /// Intrinsic width from image
    intrinsic_width: f32,
    /// Intrinsic height from image
    intrinsic_height: f32,
}

impl ImageWidgetState {
    pub fn new(device_context: ID2D1DeviceContext6) -> Self {
        Self {
            device_context,
            d2d_bitmap: None,
            cached_image_path: String::new(),
            intrinsic_width: 0.0,
            intrinsic_height: 0.0,
        }
    }

    pub fn into_any(self) -> Box<dyn Any> {
        Box::new(self)
    }

    /// Load image from file and create D2D bitmap
    fn load_image(&mut self, image_path: &str) -> windows::core::Result<bool> {
        // Only reload if path changed
        if self.cached_image_path == image_path && self.d2d_bitmap.is_some() {
            return Ok(false);
        }

        unsafe {
            // Create WIC factory
            let wic_factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;

            // Convert path to wide string
            let path_wide: Vec<u16> = image_path
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            // Create decoder from filename
            let decoder = wic_factory.CreateDecoderFromFilename(
                windows::core::PCWSTR(path_wide.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )?;

            // Get first frame
            let frame = decoder.GetFrame(0)?;

            // Get image dimensions
            let mut width = 0u32;
            let mut height = 0u32;
            frame.GetSize(&mut width, &mut height)?;
            self.intrinsic_width = width as f32;
            self.intrinsic_height = height as f32;

            // Convert to 32bppPBGRA format
            let converter: IWICFormatConverter = wic_factory.CreateFormatConverter()?;
            converter.Initialize(
                &frame,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )?;

            // Create D2D bitmap from WIC bitmap
            let d2d_bitmap = self
                .device_context
                .CreateBitmapFromWicBitmap(&converter, None)?;

            self.d2d_bitmap = Some(d2d_bitmap.into());
            self.cached_image_path = image_path.to_string();

            Ok(true)
        }
    }
}

impl Image {
    /// Create a new Image widget from a file path
    pub fn new(image_path: impl Into<StableString>) -> Self {
        Self {
            image_path: image_path.into(),
            width: None,
            height: None,
            opacity: 1.0,
        }
    }

    /// Set explicit width for layout
    pub fn with_width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Set explicit height for layout
    pub fn with_height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Set both width and height for layout
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    /// Set opacity for rendering (0.0 - 1.0)
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn as_element<Message>(self, id: u64) -> Element<Message> {
        Element {
            id: Some(id),
            content: widget(self),
            ..Default::default()
        }
    }
}

impl Default for Image {
    fn default() -> Self {
        Self::new(StableString::Static(""))
    }
}

impl<Message> Widget<Message> for Image {
    fn limits_x(&self, arenas: &UIArenas, instance: &mut Instance) -> limit_response::SizingForX {
        let state = with_state!(mut instance as ImageWidgetState);
        if let Some(image_path) = self.image_path.resolve(arenas) {
            // Load image if needed
            state.load_image(image_path).ok();
        }

        // Use explicit width if set, otherwise use intrinsic width
        if let Some(width) = self.width {
            limit_response::SizingForX {
                min_width: width,
                preferred_width: width,
            }
        } else {
            limit_response::SizingForX {
                min_width: 0.0,
                preferred_width: state.intrinsic_width,
            }
        }
    }

    fn limits_y(
        &self,
        _arenas: &UIArenas,
        instance: &mut Instance,
        _border_width: f32,
        content_width: f32,
    ) -> limit_response::SizingForY {
        // Use explicit height if set
        if let Some(height) = self.height {
            limit_response::SizingForY {
                min_height: height,
                preferred_height: height,
            }
        } else {
            // Get intrinsic dimensions from state
            let state = with_state!(instance as ImageWidgetState);
            if state.intrinsic_width > 0.0 && state.intrinsic_height > 0.0 {
                // Maintain aspect ratio based on content width
                let aspect_ratio = state.intrinsic_height / state.intrinsic_width;
                let preferred_height = content_width * aspect_ratio;
                limit_response::SizingForY {
                    min_height: 0.0,
                    preferred_height,
                }
            } else {
                // No intrinsic size yet
                limit_response::SizingForY {
                    min_height: 0.0,
                    preferred_height: 0.0,
                }
            }
        }
    }

    fn state(&self, _arenas: &UIArenas, device_resources: &DeviceResources) -> State {
        Some(ImageWidgetState::new(device_resources.d2d_device_context.clone()).into_any())
    }

    fn paint(
        &mut self,
        arenas: &UIArenas,
        instance: &mut Instance,
        _shell: &Shell<Message>,
        recorder: &mut CommandRecorder,
        _style: ElementStyle,
        bounds: Bounds,
        _now: Instant,
    ) {
        if let Some(image_path) = self.image_path.resolve(arenas) {
            let state = with_state!(mut instance as ImageWidgetState);

            // Load image if needed
            if state.load_image(image_path).is_ok() {
                if let Some(ref bitmap) = state.d2d_bitmap {
                    recorder.draw_bitmap(&bounds.content_box, bitmap, self.opacity);
                }
            }
        }
    }

    fn update(
        &mut self,
        _arenas: &mut UIArenas,
        _instance: &mut Instance,
        _hwnd: HWND,
        _shell: &mut Shell<Message>,
        _event: &Event,
        _bounds: Bounds,
    ) {
        // Image widgets don't handle events by default
    }

    fn cursor(
        &self,
        _arenas: &UIArenas,
        _instance: &Instance,
        _point: crate::gfx::PointDIP,
        _bounds: Bounds,
    ) -> Option<Cursor> {
        None
    }
}
