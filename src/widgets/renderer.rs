use std::collections::HashMap;
use std::{cell::RefCell, collections::hash_map};
use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1Shadow,
    Common::{
        D2D_RECT_F, D2D_SIZE_F, D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_OVER,
        D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
    },
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL, D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_ROUND,
    D2D1_CAP_STYLE_SQUARE, D2D1_CAP_STYLE_TRIANGLE, D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS_NONE,
    D2D1_DASH_STYLE_CUSTOM, D2D1_DASH_STYLE_DASH, D2D1_DASH_STYLE_DASH_DOT,
    D2D1_DASH_STYLE_DASH_DOT_DOT, D2D1_DASH_STYLE_DOT, D2D1_DASH_STYLE_SOLID,
    D2D1_INTERPOLATION_MODE_LINEAR, D2D1_LINE_JOIN_BEVEL, D2D1_LINE_JOIN_MITER,
    D2D1_LINE_JOIN_MITER_OR_BEVEL, D2D1_LINE_JOIN_ROUND, D2D1_PROPERTY_TYPE_FLOAT,
    D2D1_PROPERTY_TYPE_VECTOR4, D2D1_ROUNDED_RECT, D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION,
    D2D1_SHADOW_PROP_COLOR, D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    ID2D1DeviceContext7, ID2D1Effect, ID2D1Factory, ID2D1GeometrySink, ID2D1Image,
    ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows_core::{IUnknown, Interface};
use windows_numerics::{Vector2, Vector4};

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
    pub render_target: &'a ID2D1DeviceContext7,
    pub brush: &'a ID2D1SolidColorBrush,
    shadow_cache: &'a RefCell<ShadowCache>,
}

impl Renderer<'_> {
    /// Create a new Renderer with a reference to the shadow cache
    pub fn new<'a>(
        factory: &'a ID2D1Factory,
        render_target: &'a ID2D1DeviceContext7,
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
                None => (D2D1_DASH_STYLE_SOLID, Vec::new(), 0.0f32),
                Some(StrokeDashStyle::Solid) => (D2D1_DASH_STYLE_SOLID, Vec::new(), 0.0),
                Some(StrokeDashStyle::Dash) => (D2D1_DASH_STYLE_DASH, Vec::new(), 0.0),
                Some(StrokeDashStyle::Dot) => (D2D1_DASH_STYLE_DOT, Vec::new(), 0.0),
                Some(StrokeDashStyle::DashDot) => (D2D1_DASH_STYLE_DASH_DOT, Vec::new(), 0.0),
                Some(StrokeDashStyle::DashDotDot) => {
                    (D2D1_DASH_STYLE_DASH_DOT_DOT, Vec::new(), 0.0)
                }
                Some(StrokeDashStyle::Custom { dashes, offset }) => {
                    (D2D1_DASH_STYLE_CUSTOM, dashes.clone(), *offset)
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
                Some(&dashes[..])
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
            } else if let Ok(path_geometry) = self.factory.CreatePathGeometry() {
                if let Ok(sink) = path_geometry.Open() {
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
                            if let Ok(path_geometry) = self.factory.CreatePathGeometry() {
                                if let Ok(sink) = path_geometry.Open() {
                                    self.create_rounded_rectangle_path(
                                        &sink,
                                        &shadow_rect_in_bitmap,
                                        border_radius,
                                    );
                                    let _ = sink.Close();
                                    bitmap_rt.FillGeometry(&path_geometry, &shadow_brush, None);
                                }
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
                if let Ok(path_geometry) = self.factory.CreatePathGeometry() {
                    if let Ok(sink) = path_geometry.Open() {
                        self.create_rounded_rectangle_path(&sink, rect, border_radius);
                        let _ = sink.Close();
                        self.render_target
                            .FillGeometry(&path_geometry, self.brush, None);
                    }
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
            let translation_transform = Matrix3x2::translation(rect.x, rect.y);
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
}
