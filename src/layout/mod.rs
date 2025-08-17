use std::time::Instant;

use slotmap::SlotMap;
use windows::Win32::Graphics::Direct2D::{
    Common::{D2D_RECT_F, D2D1_COLOR_F},
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
};
use windows_numerics::Vector2;

use crate::{
    Shell,
    gfx::RectDIP,
    layout::{
        model::{Axis, ElementContent, UIElement, UIKey},
        positioning::position_elements,
        scroll_manager::ScrollStateManager,
    },
    widgets::Renderer,
};

pub mod model;
pub mod scroll_manager;

mod float;
pub mod visitors;

mod fit_along_axis;
mod grow_and_shrink_along_axis;
mod positioning;

use fit_along_axis::fit_along_axis;
use grow_and_shrink_along_axis::grow_and_shrink_along_axis;

pub type OwnedUITree = SlotMap<UIKey, UIElement>;
pub type BorrowedUITree<'a> = &'a mut OwnedUITree;

#[allow(dead_code)]
fn set_parent_references(slots: BorrowedUITree<'_>, root: UIKey) {
    visitors::visit_bfs(slots, root, |slots, key, parent| {
        slots[key].parent = parent;
    });
}

#[allow(dead_code)]
fn propagate_inherited_properties(slots: BorrowedUITree<'_>, root: UIKey) {
    visitors::visit_bfs(slots, root, |slots, key, parent| {
        if let Some(parent_key) = parent {
            if slots[key].color.is_none() && slots[parent_key].color.is_some() {
                slots[key].color = slots[parent_key].color;
            }
        }

        // TODO: propagate Font
    });
}

fn wrap_text(slots: BorrowedUITree<'_>, root: UIKey) {
    visitors::visit_bfs(slots, root, |slots, key, _parent| {
        if let Some(ElementContent::Text { layout, .. }) = slots[key].content.as_ref() {
            let element = &slots[key];

            let available_width =
                element.computed_width - element.padding.left - element.padding.right;

            unsafe {
                layout
                    .as_ref()
                    .unwrap()
                    .SetMaxWidth(available_width)
                    .unwrap();
            }
        }
    });
}

pub fn layout(
    slots: BorrowedUITree<'_>,
    root: UIKey,
    scroll_state_manager: &mut ScrollStateManager,
) {
    set_parent_references(slots, root);
    propagate_inherited_properties(slots, root);

    fit_along_axis(slots, root, Axis::X);
    grow_and_shrink_along_axis(slots, root, Axis::X);

    wrap_text(slots, root);

    fit_along_axis(slots, root, Axis::Y);
    grow_and_shrink_along_axis(slots, root, Axis::Y);

    position_elements(slots, root, scroll_state_manager);
}

pub const DEFAULT_SCROLLBAR_TRACK_COLOR: u32 = 0x00000033;
pub const DEFAULT_SCROLLBAR_THUMB_COLOR: u32 = 0x00000055;
pub const DEFAULT_SCROLLBAR_SIZE: f32 = 16.0;
pub const DEFAULT_SCROLLBAR_MIN_THUMB_SIZE: f32 = 16.0;

pub fn paint(
    // rt: &ID2D1HwndRenderTarget,
    // brush: &ID2D1SolidColorBrush,
    shell: &Shell,
    renderer: &Renderer,
    slots: BorrowedUITree<'_>,
    root: UIKey,
    scroll_state_manager: &mut ScrollStateManager,
    offset_x: f32,
    offset_y: f32,
) {
    let now = Instant::now();

    visitors::visit_dfs(
        slots,
        root,
        |slots, key, _parent| {
            let element = &mut slots[key];
            let x = element.x + offset_x;
            let y = element.y + offset_y;
            let width = element.computed_width;
            let height = element.computed_height;

            let has_scroll_x = matches!(element.scroll.as_ref(), Some(s) if s.horizontal.is_some());
            let has_scroll_y = matches!(element.scroll.as_ref(), Some(s) if s.vertical.is_some());

            if has_scroll_x || has_scroll_y {
                let clip_rect = D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + width,
                    bottom: y + height,
                };

                unsafe {
                    renderer
                        .render_target
                        .PushAxisAlignedClip(&clip_rect, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                }
            }

            if let Some(color) = element.background_color {
                unsafe {
                    renderer.brush.SetColor(&D2D1_COLOR_F {
                        r: (0xFF & (color >> 24)) as f32 / 255.0,
                        g: (0xFF & (color >> 16)) as f32 / 255.0,
                        b: (0xFF & (color >> 8)) as f32 / 255.0,
                        a: (0xFF & color) as f32 / 255.0,
                    });

                    let rect = D2D_RECT_F {
                        left: x,
                        top: y,
                        right: x + width,
                        bottom: y + height,
                    };
                    renderer.render_target.FillRectangle(&rect, renderer.brush);
                }
            }

            // if let Some(layout) = element.content.as_ref().and_then(|c| c.layout.as_ref()) {
            let bounds = element.bounds();
            if let Some(content) = element.content.as_mut() {
                match content {
                    ElementContent::Text { layout, .. } => {
                        let color = element.color.unwrap_or(0x000000FF);

                        unsafe {
                            renderer.brush.SetColor(&D2D1_COLOR_F {
                                r: (0xFF & (color >> 24)) as f32 / 255.0,
                                g: (0xFF & (color >> 16)) as f32 / 255.0,
                                b: (0xFF & (color >> 8)) as f32 / 255.0,
                                a: (0xFF & color) as f32 / 255.0,
                            });
                            renderer.render_target.DrawTextLayout(
                                Vector2 { X: x, Y: y },
                                layout.as_ref().unwrap(),
                                renderer.brush,
                                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                            );
                        }
                    }
                    ElementContent::Widget(widget) => {
                        widget.paint(
                            element.id, key, shell, renderer, bounds,
                            now,
                            // 1.0 / 240.0, /* TODO: dt */
                        );
                    }
                }
            }
        },
        Some(&mut |slots: BorrowedUITree<'_>, key, _parent| {
            let element = &slots[key];

            let has_scroll_x = matches!(element.scroll.as_ref(), Some(s) if s.horizontal.is_some());
            let has_scroll_y = matches!(element.scroll.as_ref(), Some(s) if s.vertical.is_some());

            if has_scroll_x || has_scroll_y {
                // Draw Scrollbars

                // Get scrollbar appearance from element
                let scroll_config = element.scroll.as_ref().unwrap();
                let scrollbar_track_color = scroll_config
                    .scrollbar_track_color
                    .unwrap_or(DEFAULT_SCROLLBAR_TRACK_COLOR);
                let scrollbar_thumb_color = scroll_config
                    .scrollbar_thumb_color
                    .unwrap_or(DEFAULT_SCROLLBAR_THUMB_COLOR);

                if let Some(ScrollbarGeom {
                    track_rect,
                    thumb_rect,
                    ..
                }) = compute_scrollbar_geom(element, Axis::X, scroll_state_manager)
                {
                    unsafe {
                        // Draw track
                        renderer.brush.SetColor(&D2D1_COLOR_F {
                            r: (0xFF & (scrollbar_track_color >> 24)) as f32 / 255.0,
                            g: (0xFF & (scrollbar_track_color >> 16)) as f32 / 255.0,
                            b: (0xFF & (scrollbar_track_color >> 8)) as f32 / 255.0,
                            a: (0xFF & scrollbar_track_color) as f32 / 255.0,
                        });

                        renderer.render_target.FillRectangle(
                            &D2D_RECT_F {
                                left: track_rect.x_dip,
                                top: track_rect.y_dip,
                                right: track_rect.x_dip + track_rect.width_dip,
                                bottom: track_rect.y_dip + track_rect.height_dip,
                            },
                            renderer.brush,
                        );

                        // Draw thumb
                        renderer.brush.SetColor(&D2D1_COLOR_F {
                            r: (0xFF & (scrollbar_thumb_color >> 24)) as f32 / 255.0,
                            g: (0xFF & (scrollbar_thumb_color >> 16)) as f32 / 255.0,
                            b: (0xFF & (scrollbar_thumb_color >> 8)) as f32 / 255.0,
                            a: (0xFF & scrollbar_thumb_color) as f32 / 255.0,
                        });

                        renderer.render_target.FillRectangle(
                            &D2D_RECT_F {
                                left: thumb_rect.x_dip,
                                top: thumb_rect.y_dip,
                                right: thumb_rect.x_dip + thumb_rect.width_dip,
                                bottom: thumb_rect.y_dip + thumb_rect.height_dip,
                            },
                            renderer.brush,
                        );
                    }
                }

                if let Some(ScrollbarGeom {
                    track_rect,
                    thumb_rect,
                    ..
                }) = compute_scrollbar_geom(element, Axis::Y, scroll_state_manager)
                {
                    unsafe {
                        // Draw track
                        renderer.brush.SetColor(&D2D1_COLOR_F {
                            r: (0xFF & (scrollbar_track_color >> 24)) as f32 / 255.0,
                            g: (0xFF & (scrollbar_track_color >> 16)) as f32 / 255.0,
                            b: (0xFF & (scrollbar_track_color >> 8)) as f32 / 255.0,
                            a: (0xFF & scrollbar_track_color) as f32 / 255.0,
                        });

                        renderer.render_target.FillRectangle(
                            &D2D_RECT_F {
                                left: track_rect.x_dip,
                                top: track_rect.y_dip,
                                right: track_rect.x_dip + track_rect.width_dip,
                                bottom: track_rect.y_dip + track_rect.height_dip,
                            },
                            renderer.brush,
                        );

                        // Draw thumb
                        renderer.brush.SetColor(&D2D1_COLOR_F {
                            r: (0xFF & (scrollbar_thumb_color >> 24)) as f32 / 255.0,
                            g: (0xFF & (scrollbar_thumb_color >> 16)) as f32 / 255.0,
                            b: (0xFF & (scrollbar_thumb_color >> 8)) as f32 / 255.0,
                            a: (0xFF & scrollbar_thumb_color) as f32 / 255.0,
                        });

                        renderer.render_target.FillRectangle(
                            &D2D_RECT_F {
                                left: thumb_rect.x_dip,
                                top: thumb_rect.y_dip,
                                right: thumb_rect.x_dip + thumb_rect.width_dip,
                                bottom: thumb_rect.y_dip + thumb_rect.height_dip,
                            },
                            renderer.brush,
                        );
                    }
                }

                unsafe {
                    renderer.render_target.PopAxisAlignedClip();
                }
            }
        }),
    );
}

// ===== Reusable scrollbar geometry helpers =====

#[derive(Clone, Copy, Debug)]
pub struct ScrollbarGeom {
    pub axis: Axis,
    pub track_rect: RectDIP,
    pub thumb_rect: RectDIP,
    // Run length along the scroll axis that the thumb can travel
    pub range: f32,
    // Track start coordinate along the scroll axis (x for X, y for Y)
    pub track_start: f32,
    // Maximum scroll value for this axis (content - viewport)
    pub max_scroll: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum ScrollDirection {
    Positive,
    Negative,
}

pub fn can_scroll_further(
    element: &UIElement,
    axis: Axis,
    direction: ScrollDirection,
    scroll_state_manager: &ScrollStateManager,
) -> bool {
    let id = match element.id {
        Some(id) => id,
        None => return false,
    };

    let scroll_position = scroll_state_manager.get_scroll_position(id);
    let (max_scroll_x, max_scroll_y) = (
        element.computed_content_width - element.computed_width,
        element.computed_content_height - element.computed_height,
    );

    match (axis, direction) {
        (Axis::X, ScrollDirection::Positive) => scroll_position.x < max_scroll_x,
        (Axis::X, ScrollDirection::Negative) => scroll_position.x > 0.0,
        (Axis::Y, ScrollDirection::Positive) => scroll_position.y < max_scroll_y,
        (Axis::Y, ScrollDirection::Negative) => scroll_position.y > 0.0,
    }
}

pub fn compute_scrollbar_geom(
    element: &UIElement,
    axis: Axis,
    scroll_state_manager: &ScrollStateManager,
) -> Option<ScrollbarGeom> {
    let has_scroll_x = matches!(element.scroll.as_ref(), Some(s) if s.horizontal.is_some());
    let has_scroll_y = matches!(element.scroll.as_ref(), Some(s) if s.vertical.is_some());
    let id = element.id?;
    let sc = element.scroll.as_ref()?;

    match axis {
        Axis::Y if has_scroll_y => {
            let width = element.computed_width;
            let height = element.computed_height;
            let content_height = element.computed_content_height;
            let max_scroll_y = (content_height - height).max(0.0);
            if content_height <= height {
                return None;
            }

            let scrollbar_size = sc.scrollbar_size.unwrap_or(DEFAULT_SCROLLBAR_SIZE);
            let scrollbar_min_thumb_size = sc
                .scrollbar_min_thumb_size
                .unwrap_or(DEFAULT_SCROLLBAR_MIN_THUMB_SIZE);

            let visible_ratio = (height / content_height).min(1.0);
            let thumb_len = (height * visible_ratio).max(scrollbar_min_thumb_size);
            let range = (height - thumb_len).max(0.0);

            let scroll_y = scroll_state_manager.get_scroll_position(id).y;
            let effective_scroll_y = scroll_y.clamp(0.0, max_scroll_y);
            let progress = if max_scroll_y > 0.0 {
                effective_scroll_y / max_scroll_y
            } else {
                0.0
            };

            let x = element.x;
            let y = element.y;
            let track_rect = RectDIP {
                x_dip: x + width - scrollbar_size,
                y_dip: y,
                width_dip: scrollbar_size,
                height_dip: height,
            };
            let thumb_rect = RectDIP {
                x_dip: x + width - scrollbar_size,
                y_dip: y + range * progress,
                width_dip: scrollbar_size,
                height_dip: thumb_len,
            };

            Some(ScrollbarGeom {
                axis,
                track_rect,
                thumb_rect,
                range,
                track_start: y,
                max_scroll: max_scroll_y,
            })
        }
        Axis::X if has_scroll_x => {
            let width = element.computed_width;
            let height = element.computed_height;
            let content_width = element.computed_content_width;
            let max_scroll_x = (content_width - width).max(0.0);
            if content_width <= width {
                return None;
            }

            let scrollbar_size = sc.scrollbar_size.unwrap_or(DEFAULT_SCROLLBAR_SIZE);
            let scrollbar_min_thumb_size = sc
                .scrollbar_min_thumb_size
                .unwrap_or(DEFAULT_SCROLLBAR_MIN_THUMB_SIZE);

            let visible_ratio = (width / content_width).min(1.0);
            let thumb_len = (width * visible_ratio).max(scrollbar_min_thumb_size);
            let range = (width - thumb_len).max(0.0);

            let scroll_x = scroll_state_manager.get_scroll_position(id).x;
            let effective_scroll_x = scroll_x.clamp(0.0, max_scroll_x);
            let progress = if max_scroll_x > 0.0 {
                effective_scroll_x / max_scroll_x
            } else {
                0.0
            };

            let x = element.x;
            let y = element.y;
            let track_rect = RectDIP {
                x_dip: x,
                y_dip: y + height - scrollbar_size,
                width_dip: width,
                height_dip: scrollbar_size,
            };
            let thumb_rect = RectDIP {
                x_dip: x + range * progress,
                y_dip: y + height - scrollbar_size,
                width_dip: thumb_len,
                height_dip: scrollbar_size,
            };

            Some(ScrollbarGeom {
                axis,
                track_rect,
                thumb_rect,
                range,
                track_start: x,
                max_scroll: max_scroll_x,
            })
        }
        _ => None,
    }
}
