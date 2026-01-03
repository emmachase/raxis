use std::collections::HashMap;
use std::{cell::RefCell, collections::hash_map, mem::ManuallyDrop};
use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1Shadow,
    Common::{
        D2D_RECT_F, D2D_SIZE_F, D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_OVER,
        D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
    },
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL, D2D1_CAP_STYLE_FLAT,
    D2D1_CAP_STYLE_ROUND, D2D1_CAP_STYLE_SQUARE, D2D1_CAP_STYLE_TRIANGLE,
    D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE, D2D1_DASH_STYLE_CUSTOM, D2D1_DASH_STYLE_DASH,
    D2D1_DASH_STYLE_DASH_DOT, D2D1_DASH_STYLE_DASH_DOT_DOT, D2D1_DASH_STYLE_DOT,
    D2D1_DASH_STYLE_SOLID, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_LAYER_PARAMETERS1,
    D2D1_LINE_JOIN_BEVEL, D2D1_LINE_JOIN_MITER, D2D1_LINE_JOIN_MITER_OR_BEVEL,
    D2D1_LINE_JOIN_ROUND, D2D1_PROPERTY_TYPE_FLOAT, D2D1_PROPERTY_TYPE_VECTOR4, D2D1_ROUNDED_RECT,
    D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION, D2D1_SHADOW_PROP_COLOR, D2D1_STROKE_STYLE_PROPERTIES,
    D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, ID2D1DeviceContext6,
    ID2D1Effect, ID2D1Factory, ID2D1Geometry, ID2D1GeometrySink, ID2D1Image, ID2D1SolidColorBrush,
    ID2D1StrokeStyle,
};
use windows_core::Interface;
use windows_numerics::{Matrix3x2, Vector2, Vector4};

use crate::{
    gfx::RectDIP,
    layout::model::{
        Border, BorderPlacement, BorderRadius, Color, DropShadow, StrokeDashStyle, StrokeLineCap,
        StrokeLineJoin,
    },
};

/// Cache key for shadow bitmaps based on shadow parameters
#[derive(Debug, Clone, Copy, PartialEq, Hash)]
struct ShadowCacheKey {
    width: u32,
    height: u32,
    blur_radius: u32,                // Rounded to avoid precision issues
    spread_radius: u32,              // Rounded to avoid precision issues
    color: [u8; 4],                  // RGBA as bytes for exact comparison
    border_radius: Option<[u32; 4]>, // [tl, tr, br, bl] rounded
    text_hash: Option<u64>,          // Hash of text content for text shadows
    inset: bool,                     // Whether this is an inset shadow
}

impl ShadowCacheKey {
    fn new(
        width: f32,
        height: f32,
        shadow: &DropShadow,
        border_radius: Option<&BorderRadius>,
    ) -> Self {
        let color_bytes = [
            (shadow.color.r * 255.0) as u8,
            (shadow.color.g * 255.0) as u8,
            (shadow.color.b * 255.0) as u8,
            (shadow.color.a * 255.0) as u8,
        ];

        let border_radius_key = border_radius.map(|br| {
            [
                (br.top_left * 100.0) as u32,
                (br.top_right * 100.0) as u32,
                (br.bottom_right * 100.0) as u32,
                (br.bottom_left * 100.0) as u32,
            ]
        });

        Self {
            width: width as u32,
            height: height as u32,
            blur_radius: (shadow.blur_radius * 100.0) as u32, // Sub-pixel precision
            spread_radius: (shadow.spread_radius * 100.0) as u32,
            color: color_bytes,
            border_radius: border_radius_key,
            text_hash: None, // Box shadows don't have text content
            inset: shadow.inset,
        }
    }

    fn from_text_shadow(
        width: f32,
        height: f32,
        text_shadow: &crate::layout::model::TextShadow,
        text_hash: u64,
    ) -> Self {
        let color_bytes = [
            (text_shadow.color.r * 255.0) as u8,
            (text_shadow.color.g * 255.0) as u8,
            (text_shadow.color.b * 255.0) as u8,
            (text_shadow.color.a * 255.0) as u8,
        ];

        Self {
            width: width as u32,
            height: height as u32,
            blur_radius: (text_shadow.blur_radius * 100.0) as u32, // Sub-pixel precision
            spread_radius: 0,
            color: color_bytes,
            border_radius: None,
            text_hash: Some(text_hash), // Include text content hash
            inset: false,               // Text shadows are never inset
        }
    }
}

// impl std::hash::Hash for ShadowCacheKey {
//     fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
//         self.width.hash(state);
//         self.height.hash(state);
//         self.blur_radius.hash(state);
//         self.spread_radius.hash(state);
//         self.color.hash(state);
//         self.border_radius.hash(state);
//     }
// }

impl Eq for ShadowCacheKey {}

/// Cached shadow effect with usage tracking
struct CachedShadowEffect {
    effect: ID2D1Effect,
    last_frame_used: i8,
}

/// Shadow cache manager with frame-based eviction
#[derive(Default)]
pub struct ShadowCache {
    cache: HashMap<ShadowCacheKey, CachedShadowEffect>,
    current_frame: i8,
}

const MAX_CACHE_AGE: i8 = 10;
const MAX_CACHE_SIZE: i8 = 100;

impl ShadowCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            current_frame: 0,
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }

    fn start_frame(&mut self) {
        self.current_frame += 1;
        self.current_frame %= MAX_CACHE_SIZE;
    }

    fn get_or_create_shadow(
        &mut self,
        key: &ShadowCacheKey,
        create_fn: impl FnOnce() -> Option<ID2D1Effect>,
    ) -> Option<&ID2D1Effect> {
        let entry = self.cache.entry(*key);

        match entry {
            hash_map::Entry::Occupied(entry) => Some(&entry.into_mut().effect),
            hash_map::Entry::Vacant(entry) => create_fn().map(|effect| {
                &entry
                    .insert(CachedShadowEffect {
                        effect,
                        last_frame_used: self.current_frame,
                    })
                    .effect
            }),
        }
    }

    fn evict_unused(&mut self) {
        // Remove entries that weren't used in the current frame
        self.cache.retain(|_, cached| {
            let real_dist = self.current_frame - cached.last_frame_used + {
                if cached.last_frame_used > self.current_frame {
                    MAX_CACHE_SIZE
                } else {
                    0
                }
            };

            real_dist < MAX_CACHE_AGE
        });
    }
}

pub struct Renderer<'a> {
    pub factory: &'a ID2D1Factory,
    pub render_target: &'a ID2D1DeviceContext6,
    pub brush: &'a ID2D1SolidColorBrush,
    shadow_cache: &'a RefCell<ShadowCache>,
}

impl Renderer<'_> {
    /// Create a new Renderer with a reference to the shadow cache
    pub fn new<'a>(
        factory: &'a ID2D1Factory,
        render_target: &'a ID2D1DeviceContext6,
        brush: &'a ID2D1SolidColorBrush,
        shadow_cache: &'a RefCell<ShadowCache>,
    ) -> Renderer<'a> {
        Renderer {
            factory,
            render_target,
            brush,
            shadow_cache,
        }
    }

    /// Mark the start of a new frame for cache management
    pub fn start_frame(&self) {
        self.shadow_cache.borrow_mut().start_frame();
    }

    /// Evict unused cache entries to free memory
    pub fn evict_unused_cache_entries(&self) {
        self.shadow_cache.borrow_mut().evict_unused();
    }

    fn create_stroke_style(
        &self,
        dash_style: &Option<StrokeDashStyle>,
        dash_cap: StrokeLineCap,
        stroke_join: StrokeLineJoin,
    ) -> Option<ID2D1StrokeStyle> {
        unsafe {
            let (dash, dashes, dash_offset) = match dash_style {
                None => (D2D1_DASH_STYLE_SOLID, &[] as &[f32], 0.0f32),
                Some(StrokeDashStyle::Solid) => (D2D1_DASH_STYLE_SOLID, &[] as &[f32], 0.0),
                Some(StrokeDashStyle::Dash) => (D2D1_DASH_STYLE_DASH, &[] as &[f32], 0.0),
                Some(StrokeDashStyle::Dot) => (D2D1_DASH_STYLE_DOT, &[] as &[f32], 0.0),
                Some(StrokeDashStyle::DashDot) => (D2D1_DASH_STYLE_DASH_DOT, &[] as &[f32], 0.0),
                Some(StrokeDashStyle::DashDotDot) => {
                    (D2D1_DASH_STYLE_DASH_DOT_DOT, &[] as &[f32], 0.0)
                }
                Some(StrokeDashStyle::Custom { dashes, offset }) => {
                    (D2D1_DASH_STYLE_CUSTOM, *dashes, *offset)
                }
            };

            let dash_cap_style = match dash_cap {
                StrokeLineCap::Flat => D2D1_CAP_STYLE_FLAT,
                StrokeLineCap::Round => D2D1_CAP_STYLE_ROUND,
                StrokeLineCap::Square => D2D1_CAP_STYLE_SQUARE,
                StrokeLineCap::Triangle => D2D1_CAP_STYLE_TRIANGLE,
            };

            let stroke_join_style = match stroke_join {
                StrokeLineJoin::Miter => D2D1_LINE_JOIN_MITER,
                StrokeLineJoin::Bevel => D2D1_LINE_JOIN_BEVEL,
                StrokeLineJoin::Round => D2D1_LINE_JOIN_ROUND,
                StrokeLineJoin::MiterOrBevel => D2D1_LINE_JOIN_MITER_OR_BEVEL,
            };

            let props = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: dash_cap_style,
                endCap: dash_cap_style,
                dashCap: dash_cap_style,
                lineJoin: stroke_join_style,
                miterLimit: 10.0,
                dashStyle: dash,
                dashOffset: dash_offset,
            };

            let dashes_slice = if dashes.is_empty() {
                None
            } else {
                Some(dashes)
            };
            self.factory.CreateStrokeStyle(&props, dashes_slice).ok()
        }
    }

    pub fn draw_rounded_rectangle_stroked(
        &self,
        rect: &RectDIP,
        border_radius: &BorderRadius,
        stroke_width: f32,
        stroke: Option<&ID2D1StrokeStyle>,
    ) {
        unsafe {
            if border_radius.top_left == border_radius.top_right
                && border_radius.top_right == border_radius.bottom_right
                && border_radius.bottom_right == border_radius.bottom_left
            {
                let rounded_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: rect.x,
                        top: rect.y,
                        right: rect.x + rect.width,
                        bottom: rect.y + rect.height,
                    },
                    radiusX: border_radius.top_left,
                    radiusY: border_radius.top_left,
                };
                self.render_target.DrawRoundedRectangle(
                    &rounded_rect,
                    self.brush,
                    stroke_width,
                    stroke,
                );
            } else if let Ok(path_geometry) = self.factory.CreatePathGeometry()
                && let Ok(sink) = path_geometry.Open() {
                    self.create_rounded_rectangle_path(&sink, rect, border_radius);
                    let _ = sink.Close();
                    self.render_target.DrawGeometry(
                        &path_geometry,
                        self.brush,
                        stroke_width,
                        stroke,
                    );
                }
        }
    }

    pub fn draw_border(
        &self,
        rect: &RectDIP,
        border_radius: Option<&BorderRadius>,
        border: &Border,
    ) {
        unsafe {
            // Set brush color
            self.brush.SetColor(&D2D1_COLOR_F {
                r: border.color.r,
                g: border.color.g,
                b: border.color.b,
                a: border.color.a,
            });

            // Adjust rect and radius for placement
            let half = border.width * 0.5;
            let mut adjusted = RectDIP {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            };
            match border.placement {
                BorderPlacement::Center => {}
                BorderPlacement::Inset => {
                    adjusted.x += half;
                    adjusted.y += half;
                    adjusted.width = (adjusted.width - border.width).max(0.0);
                    adjusted.height = (adjusted.height - border.width).max(0.0);
                }
                BorderPlacement::Outset => {
                    adjusted.x -= half;
                    adjusted.y -= half;
                    adjusted.width += border.width;
                    adjusted.height += border.width;
                }
            }

            let adjusted_radius = border_radius.map(|r| {
                let mut rr = *r;
                match border.placement {
                    BorderPlacement::Center => {}
                    BorderPlacement::Inset => {
                        rr.top_left = (rr.top_left - half).max(0.0);
                        rr.top_right = (rr.top_right - half).max(0.0);
                        rr.bottom_right = (rr.bottom_right - half).max(0.0);
                        rr.bottom_left = (rr.bottom_left - half).max(0.0);
                    }
                    BorderPlacement::Outset => {
                        rr.top_left += half;
                        rr.top_right += half;
                        rr.bottom_right += half;
                        rr.bottom_left += half;
                    }
                }
                rr
            });

            // Create stroke style if needed
            let stroke_style =
                self.create_stroke_style(&border.dash_style, border.dash_cap, border.stroke_join);
            let stroke_opt = stroke_style.as_ref();

            if let Some(rr) = adjusted_radius.as_ref() {
                self.draw_rounded_rectangle_stroked(&adjusted, rr, border.width, stroke_opt);
            } else {
                self.render_target.DrawRectangle(
                    &D2D_RECT_F {
                        left: adjusted.x,
                        top: adjusted.y,
                        right: adjusted.x + adjusted.width,
                        bottom: adjusted.y + adjusted.height,
                    },
                    self.brush,
                    border.width,
                    stroke_opt,
                );
            }
        }
    }
    pub fn draw_blurred_shadow(
        &self,
        rect: &RectDIP,
        shadow: &DropShadow,
        border_radius: Option<&BorderRadius>,
    ) {
        if shadow.inset {
            self.draw_inset_shadow(rect, shadow, border_radius);
        } else {
            self.draw_outset_shadow(rect, shadow, border_radius);
        }
    }

    fn draw_outset_shadow(
        &self,
        rect: &RectDIP,
        shadow: &DropShadow,
        border_radius: Option<&BorderRadius>,
    ) {
        unsafe {
            if shadow.blur_radius <= 0.0 {
                // Simple shadow without blur
                let shadow_rect = RectDIP {
                    x: rect.x + shadow.offset_x - shadow.spread_radius,
                    y: rect.y + shadow.offset_y - shadow.spread_radius,
                    width: rect.width + shadow.spread_radius * 2.0,
                    height: rect.height + shadow.spread_radius * 2.0,
                };
                if let Some(border_radius) = border_radius {
                    self.fill_rounded_rectangle(&shadow_rect, border_radius, shadow.color);
                } else {
                    self.fill_rectangle(&shadow_rect, shadow.color);
                }
                return;
            }

            let expanded_width = rect.width + shadow.spread_radius * 2.0;
            let expanded_height = rect.height + shadow.spread_radius * 2.0;

            // Create cache key for this shadow
            let cache_key =
                ShadowCacheKey::new(expanded_width, expanded_height, shadow, border_radius);

            // Try to get cached shadow effect or create new one
            let mut shadow_cache = self.shadow_cache.borrow_mut();
            let cached_effect = shadow_cache.get_or_create_shadow(&cache_key, || {
                // Create the shadow effect if not cached
                self.create_shadow_effect(
                    rect,
                    shadow,
                    border_radius,
                    expanded_width,
                    expanded_height,
                )
            });

            // Draw the cached shadow effect
            if let Some(effect) = cached_effect {
                self.render_target.DrawImage(
                    &effect.cast::<ID2D1Image>().unwrap(),
                    Some(&Vector2::new(
                        rect.x + shadow.offset_x - shadow.spread_radius,
                        rect.y + shadow.offset_y - shadow.spread_radius,
                    )),
                    None,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    D2D1_COMPOSITE_MODE_SOURCE_OVER,
                );
            }
        }
    }

    fn draw_inset_shadow(
        &self,
        rect: &RectDIP,
        shadow: &DropShadow,
        border_radius: Option<&BorderRadius>,
    ) {
        unsafe {
            // Simplified case for solid inset shadow (no blur)
            if shadow.blur_radius <= 0.0 {
                // Push a clip to the element bounds
                if let Some(border_radius) = border_radius {
                    // Use layer with geometry for rounded clip
                    if border_radius.top_left == border_radius.top_right
                        && border_radius.top_right == border_radius.bottom_right
                        && border_radius.bottom_right == border_radius.bottom_left
                    {
                        // Simple rounded rectangle
                        let rounded_rect = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                            rect: D2D_RECT_F {
                                left: rect.x,
                                top: rect.y,
                                right: rect.x + rect.width,
                                bottom: rect.y + rect.height,
                            },
                            radiusX: border_radius.top_left,
                            radiusY: border_radius.top_left,
                        };
                        if let Ok(geometry) =
                            self.factory.CreateRoundedRectangleGeometry(&rounded_rect)
                        {
                            let mut layer_params = D2D1_LAYER_PARAMETERS1 {
                                contentBounds: D2D_RECT_F {
                                    left: rect.x,
                                    top: rect.y,
                                    right: rect.x + rect.width,
                                    bottom: rect.y + rect.height,
                                },
                                geometricMask: ManuallyDrop::new(Some(geometry.cast::<ID2D1Geometry>().unwrap())),
                                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                                maskTransform: Matrix3x2::identity(),
                                opacity: 1.0,
                                opacityBrush: ManuallyDrop::new(None),
                                layerOptions: windows::Win32::Graphics::Direct2D::D2D1_LAYER_OPTIONS1_INITIALIZE_FROM_BACKGROUND,
                            };
                            self.render_target.PushLayer(&layer_params, None);
                            // Manually drop the ManuallyDrop fields to prevent leaks
                            ManuallyDrop::drop(&mut layer_params.geometricMask);
                            ManuallyDrop::drop(&mut layer_params.opacityBrush);
                        }
                    } else {
                        // Complex rounded rectangle - use path geometry
                        if let Ok(path_geometry) = self.factory.CreatePathGeometry()
                            && let Ok(sink) = path_geometry.Open() {
                                self.create_rounded_rectangle_path(&sink, rect, border_radius);
                                let _ = sink.Close();

                                let mut layer_params = D2D1_LAYER_PARAMETERS1 {
                                    contentBounds: D2D_RECT_F {
                                        left: rect.x,
                                        top: rect.y,
                                        right: rect.x + rect.width,
                                        bottom: rect.y + rect.height,
                                    },
                                    geometricMask: ManuallyDrop::new(Some(path_geometry.cast::<ID2D1Geometry>().unwrap())),
                                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                                    maskTransform: Matrix3x2::identity(),
                                    opacity: 1.0,
                                    opacityBrush: ManuallyDrop::new(None),
                                    layerOptions: windows::Win32::Graphics::Direct2D::D2D1_LAYER_OPTIONS1_INITIALIZE_FROM_BACKGROUND,
                                };
                                self.render_target.PushLayer(&layer_params, None);
                                // Manually drop the ManuallyDrop fields to prevent leaks
                                ManuallyDrop::drop(&mut layer_params.geometricMask);
                                ManuallyDrop::drop(&mut layer_params.opacityBrush);
                            }
                    }
                } else {
                    // Use simple axis-aligned clip
                    let clip_rect = D2D_RECT_F {
                        left: rect.x,
                        top: rect.y,
                        right: rect.x + rect.width,
                        bottom: rect.y + rect.height,
                    };
                    self.render_target
                        .PushAxisAlignedClip(&clip_rect, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                }

                // Draw a frame (large rectangle with hole) for the solid inset shadow
                // The hole represents where the element is (with offset and spread applied)
                let element_rect_with_spread = RectDIP {
                    x: rect.x + shadow.offset_x - shadow.spread_radius,
                    y: rect.y + shadow.offset_y - shadow.spread_radius,
                    width: rect.width + shadow.spread_radius * 2.0,
                    height: rect.height + shadow.spread_radius * 2.0,
                };

                // Create a large outer rectangle that covers the entire clipped area
                let outer_rect = RectDIP {
                    x: rect.x - rect.width,
                    y: rect.y - rect.height,
                    width: rect.width * 3.0,
                    height: rect.height * 3.0,
                };

                // Set brush to shadow color
                self.brush.SetColor(&D2D1_COLOR_F {
                    r: shadow.color.r,
                    g: shadow.color.g,
                    b: shadow.color.b,
                    a: shadow.color.a,
                });

                // Create a frame geometry (outer rect with hole)
                if let Ok(frame_geometry) = self.factory.CreatePathGeometry()
                    && let Ok(sink) = frame_geometry.Open() {
                        // Start the outer rectangle
                        sink.BeginFigure(
                            Vector2::new(outer_rect.x, outer_rect.y),
                            D2D1_FIGURE_BEGIN_FILLED,
                        );
                        sink.AddLine(Vector2::new(outer_rect.x + outer_rect.width, outer_rect.y));
                        sink.AddLine(Vector2::new(
                            outer_rect.x + outer_rect.width,
                            outer_rect.y + outer_rect.height,
                        ));
                        sink.AddLine(Vector2::new(outer_rect.x, outer_rect.y + outer_rect.height));
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);

                        // Add the inner hole (reverse winding)
                        if let Some(border_radius) = border_radius {
                            // Add rounded rectangle hole
                            self.create_rounded_rectangle_path_reverse(
                                &sink,
                                &element_rect_with_spread,
                                border_radius,
                            );
                        } else {
                            // Add rectangular hole (reverse winding - counterclockwise)
                            sink.BeginFigure(
                                Vector2::new(
                                    element_rect_with_spread.x,
                                    element_rect_with_spread.y,
                                ),
                                D2D1_FIGURE_BEGIN_FILLED,
                            );
                            sink.AddLine(Vector2::new(
                                element_rect_with_spread.x,
                                element_rect_with_spread.y + element_rect_with_spread.height,
                            ));
                            sink.AddLine(Vector2::new(
                                element_rect_with_spread.x + element_rect_with_spread.width,
                                element_rect_with_spread.y + element_rect_with_spread.height,
                            ));
                            sink.AddLine(Vector2::new(
                                element_rect_with_spread.x + element_rect_with_spread.width,
                                element_rect_with_spread.y,
                            ));
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        }

                        let _ = sink.Close();
                        self.render_target
                            .FillGeometry(&frame_geometry, self.brush, None);
                    }

                // Pop the clip
                if border_radius.is_some() {
                    self.render_target.PopLayer();
                } else {
                    self.render_target.PopAxisAlignedClip();
                }

                return;
            }

            // For inset shadows, we need to clip to the element bounds and draw the shadow inside
            // We create a shadow from a large outer rectangle with a hole cut out for the element

            // Push a clip to the element bounds
            if let Some(border_radius) = border_radius {
                // Use layer with geometry for rounded clip
                if border_radius.top_left == border_radius.top_right
                    && border_radius.top_right == border_radius.bottom_right
                    && border_radius.bottom_right == border_radius.bottom_left
                {
                    // Simple rounded rectangle
                    let rounded_rect = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: rect.x,
                            top: rect.y,
                            right: rect.x + rect.width,
                            bottom: rect.y + rect.height,
                        },
                        radiusX: border_radius.top_left,
                        radiusY: border_radius.top_left,
                    };
                    if let Ok(geometry) = self.factory.CreateRoundedRectangleGeometry(&rounded_rect)
                    {
                        let mut layer_params = D2D1_LAYER_PARAMETERS1 {
                            contentBounds: D2D_RECT_F {
                                left: rect.x,
                                top: rect.y,
                                right: rect.x + rect.width,
                                bottom: rect.y + rect.height,
                            },
                            geometricMask: ManuallyDrop::new(Some(geometry.cast::<ID2D1Geometry>().unwrap())),
                            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                            maskTransform: Matrix3x2::identity(),
                            opacity: 1.0,
                            opacityBrush: ManuallyDrop::new(None),
                            layerOptions: windows::Win32::Graphics::Direct2D::D2D1_LAYER_OPTIONS1_INITIALIZE_FROM_BACKGROUND,
                        };
                        self.render_target.PushLayer(&layer_params, None);
                        // Manually drop the ManuallyDrop fields to prevent leaks
                        ManuallyDrop::drop(&mut layer_params.geometricMask);
                        ManuallyDrop::drop(&mut layer_params.opacityBrush);
                    }
                } else {
                    // Complex rounded rectangle - use path geometry
                    if let Ok(path_geometry) = self.factory.CreatePathGeometry()
                        && let Ok(sink) = path_geometry.Open() {
                            self.create_rounded_rectangle_path(&sink, rect, border_radius);
                            let _ = sink.Close();

                            let mut layer_params = D2D1_LAYER_PARAMETERS1 {
                                contentBounds: D2D_RECT_F {
                                    left: rect.x,
                                    top: rect.y,
                                    right: rect.x + rect.width,
                                    bottom: rect.y + rect.height,
                                },
                                geometricMask: ManuallyDrop::new(Some(path_geometry.cast::<ID2D1Geometry>().unwrap())),
                                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                                maskTransform: Matrix3x2::identity(),
                                opacity: 1.0,
                                opacityBrush: ManuallyDrop::new(None),
                                layerOptions: windows::Win32::Graphics::Direct2D::D2D1_LAYER_OPTIONS1_INITIALIZE_FROM_BACKGROUND,
                            };
                            self.render_target.PushLayer(&layer_params, None);
                            // Manually drop the ManuallyDrop fields to prevent leaks
                            ManuallyDrop::drop(&mut layer_params.geometricMask);
                            ManuallyDrop::drop(&mut layer_params.opacityBrush);
                        }
                }
            } else {
                // Use simple axis-aligned clip
                let clip_rect = D2D_RECT_F {
                    left: rect.x,
                    top: rect.y,
                    right: rect.x + rect.width,
                    bottom: rect.y + rect.height,
                };
                self.render_target
                    .PushAxisAlignedClip(&clip_rect, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            }

            // Create expanded dimensions for the shadow bitmap
            // For inset shadows, we need extra space for the blur AND the offset
            let padding = shadow.blur_radius * 3.0 + shadow.spread_radius;
            let offset_padding_x = shadow.offset_x.abs();
            let offset_padding_y = shadow.offset_y.abs();
            let expanded_width = rect.width + (padding + offset_padding_x) * 2.0;
            let expanded_height = rect.height + (padding + offset_padding_y) * 2.0;

            // Create cache key for this inset shadow
            let cache_key =
                ShadowCacheKey::new(expanded_width, expanded_height, shadow, border_radius);

            // Try to get cached shadow effect or create new one
            let mut shadow_cache = self.shadow_cache.borrow_mut();
            let cached_effect = shadow_cache.get_or_create_shadow(&cache_key, || {
                // Create the inset shadow effect
                self.create_inset_shadow_effect(
                    rect,
                    shadow,
                    border_radius,
                    expanded_width,
                    expanded_height,
                    padding,
                )
            });

            // Draw the cached shadow effect
            if let Some(effect) = cached_effect {
                // Position the bitmap: offset from rect by padding, adjusting for negative offsets
                let draw_x = rect.x - padding + shadow.offset_x;
                let draw_y = rect.y - padding + shadow.offset_y;

                self.render_target.DrawImage(
                    &effect.cast::<ID2D1Image>().unwrap(),
                    Some(&Vector2::new(draw_x, draw_y)),
                    None,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    D2D1_COMPOSITE_MODE_SOURCE_OVER,
                );
            }

            // Pop the clip
            if border_radius.is_some() {
                self.render_target.PopLayer();
            } else {
                self.render_target.PopAxisAlignedClip();
            }
        }
    }

    /// Create a shadow effect for caching
    fn create_shadow_effect(
        &self,
        rect: &RectDIP,
        shadow: &DropShadow,
        border_radius: Option<&BorderRadius>,
        expanded_width: f32,
        expanded_height: f32,
    ) -> Option<ID2D1Effect> {
        unsafe {
            // Create a bitmap render target for the shadow
            let expanded_size = D2D_SIZE_F {
                width: expanded_width,
                height: expanded_height,
            };

            if let Ok(bitmap_rt) = self.render_target.CreateCompatibleRenderTarget(
                Some(&expanded_size),
                None,
                None,
                D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE,
            ) {
                // Draw the shadow shape to the bitmap render target
                bitmap_rt.BeginDraw();
                bitmap_rt.Clear(Some(&D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0, // Transparent background
                }));

                // Get brush from bitmap render target for drawing the shadow shape
                if let Ok(shadow_brush) = bitmap_rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0, // Full opacity white for the shadow shape
                    },
                    None,
                ) {
                    let shadow_rect_in_bitmap = RectDIP {
                        x: 0.0,
                        y: 0.0,
                        width: rect.width + shadow.spread_radius * 2.0,
                        height: rect.height + shadow.spread_radius * 2.0,
                    };

                    if let Some(border_radius) = border_radius {
                        // Draw rounded rectangle shadow shape
                        if border_radius.top_left == border_radius.top_right
                            && border_radius.top_right == border_radius.bottom_right
                            && border_radius.bottom_right == border_radius.bottom_left
                        {
                            // Use simple rounded rectangle
                            let rounded_rect =
                                windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                                    rect: D2D_RECT_F {
                                        left: shadow_rect_in_bitmap.x,
                                        top: shadow_rect_in_bitmap.y,
                                        right: shadow_rect_in_bitmap.x
                                            + shadow_rect_in_bitmap.width,
                                        bottom: shadow_rect_in_bitmap.y
                                            + shadow_rect_in_bitmap.height,
                                    },
                                    radiusX: border_radius.top_left,
                                    radiusY: border_radius.top_left,
                                };
                            bitmap_rt.FillRoundedRectangle(&rounded_rect, &shadow_brush);
                        } else {
                            // Create path geometry for complex rounded rectangle
                            if let Ok(path_geometry) = self.factory.CreatePathGeometry()
                                && let Ok(sink) = path_geometry.Open() {
                                    self.create_rounded_rectangle_path(
                                        &sink,
                                        &shadow_rect_in_bitmap,
                                        border_radius,
                                    );
                                    let _ = sink.Close();
                                    bitmap_rt.FillGeometry(&path_geometry, &shadow_brush, None);
                                }
                        }
                    } else {
                        // Draw regular rectangle shadow shape
                        let shadow_rect_d2d = D2D_RECT_F {
                            left: shadow_rect_in_bitmap.x,
                            top: shadow_rect_in_bitmap.y,
                            right: shadow_rect_in_bitmap.x + shadow_rect_in_bitmap.width,
                            bottom: shadow_rect_in_bitmap.y + shadow_rect_in_bitmap.height,
                        };
                        bitmap_rt.FillRectangle(&shadow_rect_d2d, &shadow_brush);
                    }
                }

                let _ = bitmap_rt.EndDraw(None, None);

                // Get the bitmap from the render target
                if let Ok(bitmap) = bitmap_rt.GetBitmap() {
                    // Create shadow effect
                    if let Ok(shadow_effect) = self.render_target.CreateEffect(&CLSID_D2D1Shadow) {
                        shadow_effect.SetInput(0, &bitmap, true);
                        shadow_effect
                            .SetValue(
                                D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32,
                                D2D1_PROPERTY_TYPE_FLOAT,
                                // Docs say this should be 3.0 but it seems more accurate at 2.0
                                &(shadow.blur_radius / 2.0).to_le_bytes(),
                            )
                            .unwrap();

                        shadow_effect
                            .SetValue(
                                D2D1_SHADOW_PROP_COLOR.0 as u32,
                                D2D1_PROPERTY_TYPE_VECTOR4,
                                &(std::mem::transmute::<
                                    Vector4,
                                    [u8; std::mem::size_of::<Vector4>()],
                                >(Vector4 {
                                    X: shadow.color.r,
                                    Y: shadow.color.g,
                                    Z: shadow.color.b,
                                    W: shadow.color.a,
                                })),
                            )
                            .unwrap();

                        return Some(shadow_effect);
                    }
                }
            }
            None
        }
    }

    /// Create an inset shadow effect for caching
    fn create_inset_shadow_effect(
        &self,
        rect: &RectDIP,
        shadow: &DropShadow,
        border_radius: Option<&BorderRadius>,
        expanded_width: f32,
        expanded_height: f32,
        padding: f32,
    ) -> Option<ID2D1Effect> {
        unsafe {
            // Create a bitmap render target for the inset shadow
            let expanded_size = D2D_SIZE_F {
                width: expanded_width,
                height: expanded_height,
            };

            if let Ok(bitmap_rt) = self.render_target.CreateCompatibleRenderTarget(
                Some(&expanded_size),
                None,
                None,
                D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE,
            ) {
                bitmap_rt.BeginDraw();
                bitmap_rt.Clear(Some(&D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0, // Transparent background
                }));

                // For inset shadows, create a "frame" geometry (outer rect with inner hole)
                // The element rect in the bitmap needs to account for shadow offset
                // Positive offset means shadow goes right/down, so hole shifts right/down in bitmap
                let element_rect_in_bitmap = RectDIP {
                    x: padding - shadow.spread_radius,
                    y: padding - shadow.spread_radius,
                    width: rect.width + shadow.spread_radius * 2.0,
                    height: rect.height + shadow.spread_radius * 2.0,
                };

                if let Ok(white_brush) = bitmap_rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    },
                    None,
                ) {
                    // Create a path geometry that represents the frame (outer rect with hole)
                    if let Ok(frame_geometry) = self.factory.CreatePathGeometry()
                        && let Ok(sink) = frame_geometry.Open() {
                            // Start the outer rectangle
                            sink.BeginFigure(Vector2::new(0.0, 0.0), D2D1_FIGURE_BEGIN_FILLED);
                            sink.AddLine(Vector2::new(expanded_width, 0.0));
                            sink.AddLine(Vector2::new(expanded_width, expanded_height));
                            sink.AddLine(Vector2::new(0.0, expanded_height));
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);

                            // Now add the inner shape as a hole (reverse winding)
                            if let Some(border_radius) = border_radius {
                                // Add rounded rectangle hole
                                self.create_rounded_rectangle_path_reverse(
                                    &sink,
                                    &element_rect_in_bitmap,
                                    border_radius,
                                );
                            } else {
                                // Add rectangular hole (reverse winding - counterclockwise)
                                sink.BeginFigure(
                                    Vector2::new(
                                        element_rect_in_bitmap.x,
                                        element_rect_in_bitmap.y,
                                    ),
                                    D2D1_FIGURE_BEGIN_FILLED,
                                );
                                sink.AddLine(Vector2::new(
                                    element_rect_in_bitmap.x,
                                    element_rect_in_bitmap.y + element_rect_in_bitmap.height,
                                ));
                                sink.AddLine(Vector2::new(
                                    element_rect_in_bitmap.x + element_rect_in_bitmap.width,
                                    element_rect_in_bitmap.y + element_rect_in_bitmap.height,
                                ));
                                sink.AddLine(Vector2::new(
                                    element_rect_in_bitmap.x + element_rect_in_bitmap.width,
                                    element_rect_in_bitmap.y,
                                ));
                                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            }

                            let _ = sink.Close();
                            bitmap_rt.FillGeometry(&frame_geometry, &white_brush, None);
                        }
                }

                let _ = bitmap_rt.EndDraw(None, None);

                // Get the bitmap from the render target
                if let Ok(bitmap) = bitmap_rt.GetBitmap() {
                    // Create shadow effect
                    if let Ok(shadow_effect) = self.render_target.CreateEffect(&CLSID_D2D1Shadow) {
                        shadow_effect.SetInput(0, &bitmap, true);
                        shadow_effect
                            .SetValue(
                                D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32,
                                D2D1_PROPERTY_TYPE_FLOAT,
                                &(shadow.blur_radius / 2.0).to_le_bytes(),
                            )
                            .unwrap();

                        shadow_effect
                            .SetValue(
                                D2D1_SHADOW_PROP_COLOR.0 as u32,
                                D2D1_PROPERTY_TYPE_VECTOR4,
                                &(std::mem::transmute::<
                                    Vector4,
                                    [u8; std::mem::size_of::<Vector4>()],
                                >(Vector4 {
                                    X: shadow.color.r,
                                    Y: shadow.color.g,
                                    Z: shadow.color.b,
                                    W: shadow.color.a,
                                })),
                            )
                            .unwrap();

                        return Some(shadow_effect);
                    }
                }
            }
            None
        }
    }

    pub fn draw_rectangle<C: Into<Color>>(&self, rect: &RectDIP, color: C, stroke_width: f32) {
        unsafe {
            let color = color.into();
            self.brush.SetColor(&D2D1_COLOR_F {
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            });
            self.render_target.DrawRectangle(
                &D2D_RECT_F {
                    left: rect.x,
                    top: rect.y,
                    right: rect.x + rect.width,
                    bottom: rect.y + rect.height,
                },
                self.brush,
                stroke_width,
                None,
            );
        }
    }

    pub fn fill_rectangle<C: Into<Color>>(&self, rect: &RectDIP, color: C) {
        unsafe {
            let color: Color = color.into();
            self.brush.SetColor(&D2D1_COLOR_F {
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            });
            self.render_target.FillRectangle(
                &D2D_RECT_F {
                    left: rect.x,
                    top: rect.y,
                    right: rect.x + rect.width,
                    bottom: rect.y + rect.height,
                },
                self.brush,
            );
        }
    }

    pub fn fill_rounded_rectangle<C: Into<Color>>(
        &self,
        rect: &RectDIP,
        border_radius: &BorderRadius,
        color: C,
    ) {
        unsafe {
            let color: Color = color.into();
            self.brush.SetColor(&D2D1_COLOR_F {
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            });

            // Check if all corners have the same radius for simple case
            if border_radius.top_left == border_radius.top_right
                && border_radius.top_right == border_radius.bottom_right
                && border_radius.bottom_right == border_radius.bottom_left
            {
                // Use simple rounded rectangle
                let rounded_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: rect.x,
                        top: rect.y,
                        right: rect.x + rect.width,
                        bottom: rect.y + rect.height,
                    },
                    radiusX: border_radius.top_left,
                    radiusY: border_radius.top_left,
                };
                self.render_target
                    .FillRoundedRectangle(&rounded_rect, self.brush);
            } else {
                // Create path geometry for complex rounded rectangle with different corner radii
                if let Ok(path_geometry) = self.factory.CreatePathGeometry()
                    && let Ok(sink) = path_geometry.Open() {
                        self.create_rounded_rectangle_path(&sink, rect, border_radius);
                        let _ = sink.Close();
                        self.render_target
                            .FillGeometry(&path_geometry, self.brush, None);
                    }
            }
        }
    }

    pub fn create_rounded_rectangle_path(
        &self,
        sink: &ID2D1GeometrySink,
        rect: &RectDIP,
        border_radius: &BorderRadius,
    ) {
        unsafe {
            let left = rect.x;
            let top = rect.y;
            let right = rect.x + rect.width;
            let bottom = rect.y + rect.height;

            // Clamp radii to prevent overlapping
            let max_radius_x = rect.width / 2.0;
            let max_radius_y = rect.height / 2.0;

            let tl = border_radius.top_left.min(max_radius_x).min(max_radius_y);
            let tr = border_radius.top_right.min(max_radius_x).min(max_radius_y);
            let br = border_radius
                .bottom_right
                .min(max_radius_x)
                .min(max_radius_y);
            let bl = border_radius
                .bottom_left
                .min(max_radius_x)
                .min(max_radius_y);

            // Start from top-left corner (after the radius)
            sink.BeginFigure(
                Vector2 {
                    X: left + tl,
                    Y: top,
                },
                D2D1_FIGURE_BEGIN_FILLED,
            );

            // Top edge to top-right corner
            if tr > 0.0 {
                sink.AddLine(Vector2 {
                    X: right - tr,
                    Y: top,
                });
                // Top-right arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: right,
                        Y: top + tr,
                    },
                    size: D2D_SIZE_F {
                        width: tr,
                        height: tr,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 { X: right, Y: top });
            }

            // Right edge to bottom-right corner
            if br > 0.0 {
                sink.AddLine(Vector2 {
                    X: right,
                    Y: bottom - br,
                });
                // Bottom-right arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: right - br,
                        Y: bottom,
                    },
                    size: D2D_SIZE_F {
                        width: br,
                        height: br,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 {
                    X: right,
                    Y: bottom,
                });
            }

            // Bottom edge to bottom-left corner
            if bl > 0.0 {
                sink.AddLine(Vector2 {
                    X: left + bl,
                    Y: bottom,
                });
                // Bottom-left arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: left,
                        Y: bottom - bl,
                    },
                    size: D2D_SIZE_F {
                        width: bl,
                        height: bl,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 { X: left, Y: bottom });
            }

            // Left edge to top-left corner
            if tl > 0.0 {
                sink.AddLine(Vector2 {
                    X: left,
                    Y: top + tl,
                });
                // Top-left arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: left + tl,
                        Y: top,
                    },
                    size: D2D_SIZE_F {
                        width: tl,
                        height: tl,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 { X: left, Y: top });
            }

            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        }
    }

    /// Create a rounded rectangle path with reverse winding (for creating holes)
    fn create_rounded_rectangle_path_reverse(
        &self,
        sink: &ID2D1GeometrySink,
        rect: &RectDIP,
        border_radius: &BorderRadius,
    ) {
        unsafe {
            let left = rect.x;
            let top = rect.y;
            let right = rect.x + rect.width;
            let bottom = rect.y + rect.height;

            // Clamp radii to prevent overlapping
            let max_radius_x = rect.width / 2.0;
            let max_radius_y = rect.height / 2.0;

            let tl = border_radius.top_left.min(max_radius_x).min(max_radius_y);
            let tr = border_radius.top_right.min(max_radius_x).min(max_radius_y);
            let br = border_radius
                .bottom_right
                .min(max_radius_x)
                .min(max_radius_y);
            let bl = border_radius
                .bottom_left
                .min(max_radius_x)
                .min(max_radius_y);

            // Start from top-left corner (going counterclockwise)
            sink.BeginFigure(
                Vector2 {
                    X: left + tl,
                    Y: top,
                },
                D2D1_FIGURE_BEGIN_FILLED,
            );

            // Go counterclockwise: top-left arc
            if tl > 0.0 {
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: left,
                        Y: top + tl,
                    },
                    size: D2D_SIZE_F {
                        width: tl,
                        height: tl,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 { X: left, Y: top });
            }

            // Left edge to bottom-left corner
            if bl > 0.0 {
                sink.AddLine(Vector2 {
                    X: left,
                    Y: bottom - bl,
                });
                // Bottom-left arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: left + bl,
                        Y: bottom,
                    },
                    size: D2D_SIZE_F {
                        width: bl,
                        height: bl,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 { X: left, Y: bottom });
            }

            // Bottom edge to bottom-right corner
            if br > 0.0 {
                sink.AddLine(Vector2 {
                    X: right - br,
                    Y: bottom,
                });
                // Bottom-right arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: right,
                        Y: bottom - br,
                    },
                    size: D2D_SIZE_F {
                        width: br,
                        height: br,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 {
                    X: right,
                    Y: bottom,
                });
            }

            // Right edge to top-right corner
            if tr > 0.0 {
                sink.AddLine(Vector2 {
                    X: right,
                    Y: top + tr,
                });
                // Top-right arc
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: Vector2 {
                        X: right - tr,
                        Y: top,
                    },
                    size: D2D_SIZE_F {
                        width: tr,
                        height: tr,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
            } else {
                sink.AddLine(Vector2 { X: right, Y: top });
            }

            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        }
    }

    /// Draw an SVG document at the specified rectangle
    pub fn draw_svg(
        &self,
        rect: &RectDIP,
        svg_document: &windows::Win32::Graphics::Direct2D::ID2D1SvgDocument,
    ) {
        unsafe {
            use windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_F;
            use windows_numerics::Matrix3x2;

            // Save current transform
            let mut current_transform = Matrix3x2::default();
            self.render_target.GetTransform(&mut current_transform);

            // Set SVG viewport size first
            let _ = svg_document.SetViewportSize(D2D_SIZE_F {
                width: rect.width,
                height: rect.height,
            });

            // Apply translation to position the SVG at the target rect
            let translation_transform = Matrix3x2::translation(rect.x, rect.y);
            let combined_transform = translation_transform * current_transform;
            self.render_target.SetTransform(&combined_transform);

            // Draw the SVG document
            self.render_target.DrawSvgDocument(svg_document);

            // Restore original transform
            self.render_target.SetTransform(&current_transform);
        }
    }

    /// Fill a path geometry at the specified rectangle with transform
    pub fn fill_path_geometry(
        &self,
        rect: &RectDIP,
        path_geometry: &windows::Win32::Graphics::Direct2D::ID2D1PathGeometry,
        color: Color,
        scale_x: f32,
        scale_y: f32,
    ) {
        unsafe {
            use windows_numerics::Matrix3x2;

            // Save current transform
            let mut current_transform = Matrix3x2::default();
            self.render_target.GetTransform(&mut current_transform);

            // Apply translation and scale to position and size the geometry at the target rect
            let translation_transform = Matrix3x2::translation(rect.x, rect.y);
            let scale_transform = Matrix3x2::scale(scale_x, scale_y);
            let combined_transform = scale_transform * translation_transform * current_transform;
            self.render_target.SetTransform(&combined_transform);

            // Set brush color and fill geometry
            self.brush
                .SetColor(&windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
                    r: color.r,
                    g: color.g,
                    b: color.b,
                    a: color.a,
                });
            self.render_target
                .FillGeometry(path_geometry, self.brush, None);

            // Restore original transform
            self.render_target.SetTransform(&current_transform);
        }
    }

    /// Stroke a path geometry at the specified rectangle with transform
    pub fn stroke_path_geometry(
        &self,
        rect: &RectDIP,
        path_geometry: &windows::Win32::Graphics::Direct2D::ID2D1PathGeometry,
        color: Color,
        stroke_width: f32,
        scale_x: f32,
        scale_y: f32,
        stroke_cap: Option<StrokeLineCap>,
        stroke_join: Option<StrokeLineJoin>,
    ) {
        unsafe {
            use windows_numerics::Matrix3x2;

            // Save current transform
            let mut current_transform = Matrix3x2::default();
            self.render_target.GetTransform(&mut current_transform);

            // Apply translation and scale to position and size the geometry at the target rect
            let translation_transform = if stroke_width == 1.0 {
                Matrix3x2::translation(rect.x - 0.5, rect.y - 0.5) // For 1px stroke, offset by 0.5 to avoid antialiasing
            } else {
                Matrix3x2::translation(rect.x, rect.y)
            };
            let scale_transform = Matrix3x2::scale(scale_x, scale_y);
            let combined_transform = scale_transform * translation_transform * current_transform;
            self.render_target.SetTransform(&combined_transform);

            // Set brush color
            self.brush
                .SetColor(&windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
                    r: color.r,
                    g: color.g,
                    b: color.b,
                    a: color.a,
                });

            // Create stroke style if cap or line join are specified
            let stroke_style = if stroke_cap.is_some() || stroke_join.is_some() {
                self.create_stroke_style(
                    &None, // no dash style for path geometry
                    stroke_cap.unwrap_or(StrokeLineCap::Square),
                    stroke_join.unwrap_or(StrokeLineJoin::Miter),
                )
            } else {
                None
            };

            // Stroke geometry with optional stroke style
            self.render_target.DrawGeometry(
                path_geometry,
                self.brush,
                stroke_width,
                stroke_style.as_ref(),
            );

            // Restore original transform
            self.render_target.SetTransform(&current_transform);
        }
    }

    /// Draw a line with optional dash style and stroke cap
    pub fn draw_line(
        &self,
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
        color: Color,
        stroke_width: f32,
        dash_style: Option<&StrokeDashStyle>,
        stroke_cap: Option<StrokeLineCap>,
    ) {
        unsafe {
            // Set brush color
            self.brush.SetColor(&D2D1_COLOR_F {
                r: color.r,
                g: color.g,
                b: color.b,
                a: color.a,
            });

            let start_point = Vector2 {
                X: start_x,
                Y: start_y,
            };
            let end_point = Vector2 { X: end_x, Y: end_y };

            // Create stroke style if needed (for dash patterns or custom caps)
            let stroke_style = if dash_style.is_some() || stroke_cap.is_some() {
                // Convert Option<&StrokeDashStyle> to &Option<StrokeDashStyle>
                let owned_dash_style = dash_style.cloned();
                self.create_stroke_style(
                    &owned_dash_style,
                    stroke_cap.unwrap_or(StrokeLineCap::Square),
                    StrokeLineJoin::Miter, // Line join doesn't matter for single lines
                )
            } else {
                None
            };

            // Draw the line
            self.render_target.DrawLine(
                start_point,
                end_point,
                self.brush,
                stroke_width,
                stroke_style.as_ref(),
            );
        }
    }

    /// Draw a bitmap at the specified rectangle
    pub fn draw_bitmap(
        &self,
        rect: &RectDIP,
        bitmap: &windows::Win32::Graphics::Direct2D::ID2D1Bitmap,
        opacity: f32,
    ) {
        unsafe {
            use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;

            let dest_rect = D2D_RECT_F {
                left: rect.x,
                top: rect.y,
                right: rect.x + rect.width,
                bottom: rect.y + rect.height,
            };

            self.render_target.DrawBitmap(
                bitmap,
                Some(&dest_rect),
                opacity,
                D2D1_INTERPOLATION_MODE_LINEAR,
                None, // Draw entire source bitmap
                None, // No perspective transform
            );
        }
    }

    /// Draw text with a blurred shadow effect
    pub fn draw_text_with_blurred_shadow(
        &self,
        position: &Vector2,
        layout: &windows::Win32::Graphics::DirectWrite::IDWriteTextLayout,
        text_shadow: &crate::layout::model::TextShadow,
    ) {
        unsafe {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            // Get text metrics for sizing
            let mut metrics = windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut metrics).is_err() {
                return;
            }

            let expanded_width = metrics.layoutWidth.ceil() + text_shadow.blur_radius * 4.0;
            let expanded_height = metrics.layoutHeight.ceil() + text_shadow.blur_radius * 4.0;

            // Compute hash of text content from the layout pointer
            // Note: We use the layout pointer as a proxy for text content identity
            // This assumes layouts are recreated when text changes
            let mut hasher = DefaultHasher::new();
            let layout_ptr = layout as *const _ as usize;
            layout_ptr.hash(&mut hasher);
            let text_hash = hasher.finish();

            // Create cache key for this text shadow
            let cache_key = ShadowCacheKey::from_text_shadow(
                expanded_width,
                expanded_height,
                text_shadow,
                text_hash,
            );

            // Try to get cached shadow effect or create new one
            let mut shadow_cache = self.shadow_cache.borrow_mut();
            let cached_effect = shadow_cache.get_or_create_shadow(&cache_key, || {
                self.create_text_shadow_effect(layout, text_shadow, &metrics)
            });

            // Draw the cached shadow effect
            if let Some(effect) = cached_effect {
                self.render_target.DrawImage(
                    &effect.cast::<ID2D1Image>().unwrap(),
                    Some(&Vector2::new(
                        position.X - text_shadow.blur_radius * 2.0,
                        position.Y - text_shadow.blur_radius * 2.0,
                    )),
                    None,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    D2D1_COMPOSITE_MODE_SOURCE_OVER,
                );
            }
        }
    }

    /// Create a text shadow effect for caching
    fn create_text_shadow_effect(
        &self,
        layout: &windows::Win32::Graphics::DirectWrite::IDWriteTextLayout,
        text_shadow: &crate::layout::model::TextShadow,
        metrics: &windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS,
    ) -> Option<ID2D1Effect> {
        unsafe {
            use windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT;

            // Create shadow effect
            let shadow_effect = self.render_target.CreateEffect(&CLSID_D2D1Shadow).ok()?;

            // Set shadow properties
            let blur_value = text_shadow.blur_radius;
            shadow_effect
                .SetValue(
                    D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32,
                    D2D1_PROPERTY_TYPE_FLOAT,
                    std::slice::from_raw_parts(
                        &blur_value as *const f32 as *const u8,
                        std::mem::size_of::<f32>(),
                    ),
                )
                .ok();

            let shadow_color = Vector4::new(
                text_shadow.color.r,
                text_shadow.color.g,
                text_shadow.color.b,
                text_shadow.color.a,
            );
            shadow_effect
                .SetValue(
                    D2D1_SHADOW_PROP_COLOR.0 as u32,
                    D2D1_PROPERTY_TYPE_VECTOR4,
                    std::slice::from_raw_parts(
                        &shadow_color as *const Vector4 as *const u8,
                        std::mem::size_of::<Vector4>(),
                    ),
                )
                .ok();

            // Create a compatible render target for the text
            // Use layoutWidth/layoutHeight to account for text alignment within the layout box
            let size = D2D_SIZE_F {
                width: metrics.layoutWidth.ceil() + text_shadow.blur_radius * 4.0,
                height: metrics.layoutHeight.ceil() + text_shadow.blur_radius * 4.0,
            };

            let bitmap_rt = self
                .render_target
                .CreateCompatibleRenderTarget(
                    Some(&size),
                    None,
                    None,
                    D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE,
                )
                .ok()?;

            // Draw text to the bitmap render target
            bitmap_rt.BeginDraw();
            bitmap_rt.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            self.brush.SetColor(&D2D1_COLOR_F {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            });

            let text_pos =
                Vector2::new(text_shadow.blur_radius * 2.0, text_shadow.blur_radius * 2.0);

            bitmap_rt.DrawTextLayout(
                text_pos,
                layout,
                self.brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
            );

            bitmap_rt.EndDraw(None, None).ok();

            // Get the bitmap from the render target
            let bitmap = bitmap_rt.GetBitmap().ok()?;

            // Set the bitmap as input to the shadow effect
            shadow_effect.SetInput(0, Some(&bitmap.cast::<ID2D1Image>().unwrap()), true);

            Some(shadow_effect)
        }
    }
}
