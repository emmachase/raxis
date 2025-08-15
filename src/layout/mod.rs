use slotmap::SlotMap;
use windows::Win32::Graphics::Direct2D::{
    Common::{D2D_RECT_F, D2D1_COLOR_F},
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
    ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows_numerics::Vector2;

use crate::{
    gfx::RectDIP,
    layout::{
        model::{Axis, UIElement, UIKey},
        positioning::position_elements,
        scroll_manager::ScrollStateManager,
    },
};

pub mod model;
pub mod scroll_manager;

mod float;
mod visitors;

mod fit_along_axis;
mod grow_and_shrink_along_axis;
mod positioning;

use fit_along_axis::fit_along_axis;
use grow_and_shrink_along_axis::grow_and_shrink_along_axis;

pub type OwnedUITree = SlotMap<UIKey, UIElement>;
pub type UITree<'a> = &'a mut OwnedUITree;

#[allow(dead_code)]
fn set_parent_references(slots: UITree<'_>, root: UIKey) {
    visitors::visit_bfs(slots, root, |slots, key, parent| {
        slots[key].parent = parent;
    });
}

#[allow(dead_code)]
fn propagate_inherited_properties(slots: UITree<'_>, root: UIKey) {
    visitors::visit_bfs(slots, root, |slots, key, parent| {
        if let Some(parent_key) = parent {
            if slots[key].color.is_none() && slots[parent_key].color.is_some() {
                slots[key].color = slots[parent_key].color;
            }
        }

        // TODO: propagate Font
    });
}

fn wrap_text(slots: UITree<'_>, root: UIKey) {
    visitors::visit_bfs(slots, root, |slots, key, _parent| {
        if slots[key].is_text_element() {
            let element = &slots[key];

            let available_width =
                element.computed_width - element.padding.left - element.padding.right;

            let content = slots[key].content.as_ref().unwrap();
            unsafe {
                content
                    .layout
                    .as_ref()
                    .unwrap()
                    .SetMaxWidth(available_width)
                    .unwrap();
            }
        }
    });
}

pub fn layout<SS: ScrollStateManager>(
    slots: UITree<'_>,
    root: UIKey,
    scroll_state_manager: &mut SS,
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

const DEFAULT_SCROLLBAR_TRACK_COLOR: u32 = 0x00000033;
const DEFAULT_SCROLLBAR_THUMB_COLOR: u32 = 0x00000055;
const DEFAULT_SCROLLBAR_SIZE: f32 = 16.0;
const DEFAULT_SCROLLBAR_MIN_THUMB_SIZE: f32 = 16.0;

pub fn paint<SS: ScrollStateManager>(
    rt: &ID2D1HwndRenderTarget,
    brush: &ID2D1SolidColorBrush,
    slots: UITree<'_>,
    root: UIKey,
    scroll_state_manager: &mut SS,
    offset_x: f32,
    offset_y: f32,
) {
    visitors::visit_dfs(
        slots,
        root,
        |slots, key, _parent| {
            let element = &slots[key];
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
                    rt.PushAxisAlignedClip(&clip_rect, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                }
            }

            if let Some(color) = element.background_color {
                unsafe {
                    brush.SetColor(&D2D1_COLOR_F {
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
                    rt.FillRectangle(&rect, brush);
                }
            }

            if let Some(layout) = element.content.as_ref().and_then(|c| c.layout.as_ref()) {
                let color = element.color.unwrap_or(0x000000FF);

                unsafe {
                    brush.SetColor(&D2D1_COLOR_F {
                        r: (0xFF & (color >> 24)) as f32 / 255.0,
                        g: (0xFF & (color >> 16)) as f32 / 255.0,
                        b: (0xFF & (color >> 8)) as f32 / 255.0,
                        a: (0xFF & color) as f32 / 255.0,
                    });
                    rt.DrawTextLayout(
                        Vector2 { X: x, Y: y },
                        layout,
                        brush,
                        D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                    );
                }
            }
        },
        Some(&mut |slots: UITree<'_>, key, _parent| {
            let element = &slots[key];
            let x = element.x + offset_x;
            let y = element.y + offset_y;
            let width = element.computed_width;
            let height = element.computed_height;

            let has_scroll_x = matches!(element.scroll.as_ref(), Some(s) if s.horizontal.is_some());
            let has_scroll_y = matches!(element.scroll.as_ref(), Some(s) if s.vertical.is_some());

            if has_scroll_x || has_scroll_y {
                // Draw Scrollbars
                // Get scroll position for scrollable elements
                let scroll_x = if has_scroll_x && let Some(id) = element.id {
                    scroll_state_manager.get_scroll_position(id).x
                } else {
                    0.0
                };
                let scroll_y = if has_scroll_y && let Some(id) = element.id {
                    scroll_state_manager.get_scroll_position(id).y
                } else {
                    0.0
                };

                let content_width = element.computed_content_width;
                let content_height = element.computed_content_height;

                let max_scroll_x = (content_width - element.computed_width).max(0.0);
                let max_scroll_y = (content_height - element.computed_height).max(0.0);

                // Get scrollbar appearance from element
                let scroll_config = element.scroll.as_ref().unwrap();
                let scrollbar_size = scroll_config
                    .scrollbar_size
                    .unwrap_or(DEFAULT_SCROLLBAR_SIZE);
                let scrollbar_track_color = scroll_config
                    .scrollbar_track_color
                    .unwrap_or(DEFAULT_SCROLLBAR_TRACK_COLOR);
                let scrollbar_thumb_color = scroll_config
                    .scrollbar_thumb_color
                    .unwrap_or(DEFAULT_SCROLLBAR_THUMB_COLOR);
                let scrollbar_min_thumb_size = scroll_config
                    .scrollbar_min_thumb_size
                    .unwrap_or(DEFAULT_SCROLLBAR_MIN_THUMB_SIZE);

                // Draw vertical scrollbar if needed
                if has_scroll_y && element.computed_content_height > element.computed_height {
                    // Calculate the visible portion ratio (viewport height / content height)
                    let visible_ratio = (element.computed_height / content_height).min(1.0);

                    // Calculate the scrollbar min thumb height
                    let thumb_height = (height * visible_ratio).max(scrollbar_min_thumb_size);

                    // Calculate thumb position, clamp to viewport
                    let effective_scroll_y = scroll_y.clamp(0.0, max_scroll_y);
                    let scroll_progress = if max_scroll_y > 0.0 {
                        effective_scroll_y / max_scroll_y
                    } else {
                        0.0
                    };
                    let thumb_y = y + (height - thumb_height) * scroll_progress;

                    unsafe {
                        // Draw track
                        brush.SetColor(&D2D1_COLOR_F {
                            r: (0xFF & (scrollbar_track_color >> 24)) as f32 / 255.0,
                            g: (0xFF & (scrollbar_track_color >> 16)) as f32 / 255.0,
                            b: (0xFF & (scrollbar_track_color >> 8)) as f32 / 255.0,
                            a: (0xFF & scrollbar_track_color) as f32 / 255.0,
                        });

                        rt.FillRectangle(
                            &D2D_RECT_F {
                                left: x + width - scrollbar_size,
                                top: y,
                                right: x + width,
                                bottom: y + height,
                            },
                            brush,
                        );

                        // Draw thumb
                        brush.SetColor(&D2D1_COLOR_F {
                            r: (0xFF & (scrollbar_thumb_color >> 24)) as f32 / 255.0,
                            g: (0xFF & (scrollbar_thumb_color >> 16)) as f32 / 255.0,
                            b: (0xFF & (scrollbar_thumb_color >> 8)) as f32 / 255.0,
                            a: (0xFF & scrollbar_thumb_color) as f32 / 255.0,
                        });

                        rt.FillRectangle(
                            &D2D_RECT_F {
                                left: x + width - scrollbar_size,
                                top: thumb_y,
                                right: x + width,
                                bottom: thumb_y + thumb_height,
                            },
                            brush,
                        );
                    }
                }

                // Horizontal scrollbar
                if has_scroll_x && element.computed_content_width > element.computed_width {
                    // Calculate the visible portion ratio (viewport width / content width)
                    let visible_ratio = (element.computed_width / content_width).min(1.0);

                    // Calculate the scrollbar min thumb width
                    let thumb_width = (width * visible_ratio).max(scrollbar_min_thumb_size);

                    // Calculate thumb position, clamp to viewport
                    let effective_scroll_x = scroll_x.clamp(0.0, max_scroll_x);
                    let scroll_progress = if max_scroll_x > 0.0 {
                        effective_scroll_x / max_scroll_x
                    } else {
                        0.0
                    };
                    let thumb_x = x + (width - thumb_width) * scroll_progress;

                    unsafe {
                        // Draw track
                        brush.SetColor(&D2D1_COLOR_F {
                            r: (0xFF & (scrollbar_track_color >> 24)) as f32 / 255.0,
                            g: (0xFF & (scrollbar_track_color >> 16)) as f32 / 255.0,
                            b: (0xFF & (scrollbar_track_color >> 8)) as f32 / 255.0,
                            a: (0xFF & scrollbar_track_color) as f32 / 255.0,
                        });

                        rt.FillRectangle(
                            &D2D_RECT_F {
                                left: x,
                                top: y + height - scrollbar_size,
                                right: x + width,
                                bottom: y + height,
                            },
                            brush,
                        );

                        // Draw thumb
                        brush.SetColor(&D2D1_COLOR_F {
                            r: (0xFF & (scrollbar_thumb_color >> 24)) as f32 / 255.0,
                            g: (0xFF & (scrollbar_thumb_color >> 16)) as f32 / 255.0,
                            b: (0xFF & (scrollbar_thumb_color >> 8)) as f32 / 255.0,
                            a: (0xFF & scrollbar_thumb_color) as f32 / 255.0,
                        });

                        rt.FillRectangle(
                            &D2D_RECT_F {
                                left: thumb_x,
                                top: y + height - scrollbar_size,
                                right: thumb_x + thumb_width,
                                bottom: y + height,
                            },
                            brush,
                        );
                    }
                }

                unsafe {
                    rt.PopAxisAlignedClip();
                }
            }
        }),
    );
}
