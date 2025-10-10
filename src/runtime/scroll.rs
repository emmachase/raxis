use std::{collections::HashMap, time::Instant};

use crate::{
    Animation, Shell,
    gfx::{RectDIP, color::Color},
    layout::model::{Axis, BorderRadius, ScrollBarSize, UIElement},
};

pub const DEFAULT_SCROLLBAR_TRACK_COLOR: Color = Color::from_hex(0x00000033);
pub const DEFAULT_SCROLLBAR_THUMB_COLOR: Color = Color::from_hex(0x00000055);
pub const DEFAULT_SCROLLBAR_SIZE: ScrollBarSize = ScrollBarSize::ThinThick(8.0, 16.0);
pub const DEFAULT_SCROLLBAR_MIN_THUMB_SIZE: f32 = 16.0;
pub const DEFAULT_SCROLLBAR_TRACK_RADIUS: BorderRadius = BorderRadius::all(0.0);
pub const DEFAULT_SCROLLBAR_THUMB_RADIUS: BorderRadius = BorderRadius::all(0.0);

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ScrollPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ScrollMetadata {
    pub position: ScrollPosition,
    /// Whether the scroll-bar is hovered or actively being dragged
    pub animation: (Animation<bool>, Animation<bool>),
    max_scroll: ScrollPosition,
    was_at_bottom: bool,
    was_at_right: bool,
    previous_content_dimensions: (f32, f32),
    container_dimensions: (f32, f32),
}

#[derive(Clone, Default)]
pub struct ScrollStateManager {
    scroll_metadata: HashMap<u64, ScrollMetadata>,
    pub active_scrollbar: Option<(u64, Axis)>,
}

/// The threshold in pixels (dips) for considering a scroll position to be at the bottom or right of the scrollable area.
pub const SCROLL_SNAP_THRESHOLD: f32 = 5.0;

impl ScrollStateManager {
    pub fn get_scroll_position(&self, element_id: u64) -> ScrollPosition {
        self.scroll_metadata
            .get(&element_id)
            .cloned()
            .unwrap_or_default()
            .position
    }

    pub fn get_scroll_metadata(&self, element_id: u64) -> ScrollMetadata {
        self.scroll_metadata
            .get(&element_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_scroll_position(&mut self, element_id: u64, position: ScrollPosition) {
        self.scroll_metadata
            .entry(element_id)
            .and_modify(|metadata| {
                metadata.position.x = position.x.clamp(0.0, metadata.max_scroll.x);
                metadata.position.y = position.y.clamp(0.0, metadata.max_scroll.y);
            })
            .or_insert(ScrollMetadata {
                position,

                ..Default::default()
            });
    }

    pub fn set_active(&mut self, element_id: u64, axis: Axis) {
        if let Some((active_element_id, active_axis)) = self.active_scrollbar
            && (active_element_id != element_id || active_axis != axis)
        {
            self.set_inactive();
        }

        self.scroll_metadata
            .entry(element_id)
            .and_modify(|metadata| {
                match axis {
                    Axis::X => metadata.animation.0.update(true),
                    Axis::Y => metadata.animation.1.update(true),
                }

                self.active_scrollbar = Some((element_id, axis));
            });
    }

    pub fn set_inactive(&mut self) -> bool {
        if let Some((element_id, _axis)) = self.active_scrollbar {
            self.scroll_metadata
                .entry(element_id)
                .and_modify(|metadata| {
                    metadata.animation.0.update(false);
                    metadata.animation.1.update(false);
                });

            self.active_scrollbar = None;
            return true;
        }

        false
    }

    pub fn update_scroll_position(
        &mut self,
        element_id: u64,
        delta_x: f32,
        delta_y: f32,
    ) -> ScrollPosition {
        self.scroll_metadata
            .entry(element_id)
            .and_modify(|metadata| {
                metadata.position.x += delta_x;
                metadata.position.y += delta_y;
            })
            .or_insert(ScrollMetadata {
                position: ScrollPosition {
                    x: delta_x,
                    y: delta_y,
                },

                ..Default::default()
            })
            .position
    }

    pub fn apply_scroll_limits(
        &mut self,
        element_id: u64,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    ) {
        self.scroll_metadata
            .entry(element_id)
            .and_modify(|metadata| {
                metadata.position.x = metadata.position.x.clamp(min_x, max_x);
                metadata.position.y = metadata.position.y.clamp(min_y, max_y);
            });
    }

    pub fn update_scroll_metadata(
        &mut self,
        element_id: u64,
        current_position: ScrollPosition,
        max_scroll_x: f32,
        max_scroll_y: f32,
        content_width: f32,
        content_height: f32,
        container_width: f32,
        container_height: f32,
    ) {
        let was_at_bottom = (current_position.y - max_scroll_y).abs() <= SCROLL_SNAP_THRESHOLD;
        let was_at_right = (current_position.x - max_scroll_x).abs() <= SCROLL_SNAP_THRESHOLD;

        self.scroll_metadata
            .entry(element_id)
            .and_modify(|metadata| {
                metadata.position = current_position;
                metadata.max_scroll = ScrollPosition {
                    x: max_scroll_x,
                    y: max_scroll_y,
                };
                metadata.was_at_bottom = was_at_bottom;
                metadata.was_at_right = was_at_right;
                metadata.previous_content_dimensions = (content_width, content_height);
                metadata.container_dimensions = (container_width, container_height);
            })
            .or_insert(ScrollMetadata {
                position: current_position,
                max_scroll: ScrollPosition {
                    x: max_scroll_x,
                    y: max_scroll_y,
                },
                was_at_bottom,
                was_at_right,
                previous_content_dimensions: (content_width, content_height),
                container_dimensions: (container_width, container_height),
                animation: (Animation::new(false), Animation::new(false)),
            });
    }

    pub fn was_at_bottom(&self, element_id: u64) -> bool {
        self.scroll_metadata
            .get(&element_id)
            .map(|metadata| metadata.was_at_bottom)
            .unwrap_or(false)
    }

    pub fn was_at_right(&self, element_id: u64) -> bool {
        self.scroll_metadata
            .get(&element_id)
            .map(|metadata| metadata.was_at_right)
            .unwrap_or(false)
    }

    pub fn get_previous_content_dimensions(&self, element_id: u64) -> (f32, f32) {
        self.scroll_metadata
            .get(&element_id)
            .map(|metadata| metadata.previous_content_dimensions)
            .unwrap_or((0.0, 0.0))
    }

    pub fn get_container_dimensions(&self, element_id: u64) -> (f32, f32) {
        self.scroll_metadata
            .get(&element_id)
            .map(|metadata| metadata.container_dimensions)
            .unwrap_or((0.0, 0.0))
    }
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

pub fn can_scroll_further<Message>(
    element: &UIElement<Message>,
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

pub fn compute_scrollbar_geom<Message>(
    shell: &mut Shell<Message>,
    element: &UIElement<Message>,
    axis: Axis,
) -> Option<ScrollbarGeom> {
    let has_scroll_x = matches!(element.scroll.as_ref(), Some(s) if s.horizontal.is_some());
    let has_scroll_y = matches!(element.scroll.as_ref(), Some(s) if s.vertical.is_some());
    let id = element.id?;
    let sc = element.scroll.as_ref()?;

    let scroll_metadata = shell.scroll_state_manager.get_scroll_metadata(id);

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
            let scrollbar_size = scroll_metadata.animation.1.interpolate(
                shell,
                scrollbar_size.thin(),
                scrollbar_size.thick(),
                Instant::now(),
            );

            let scrollbar_min_thumb_size = sc
                .scrollbar_min_thumb_size
                .unwrap_or(DEFAULT_SCROLLBAR_MIN_THUMB_SIZE);

            let scroll_y = scroll_metadata.position.y;
            let effective_scroll_y = scroll_y.clamp(0.0, max_scroll_y);
            let progress = if max_scroll_y > 0.0 {
                effective_scroll_y / max_scroll_y
            } else {
                0.0
            };
            let visible_ratio = (height / content_height).min(1.0);

            let safe_area_padding = sc.safe_area_padding.unwrap_or_default();
            let x = element.x + safe_area_padding.left;
            let y = element.y + safe_area_padding.top;
            let safe_width = width - safe_area_padding.left - safe_area_padding.right;
            let safe_height = height - safe_area_padding.top - safe_area_padding.bottom;

            let thumb_len = (safe_height * visible_ratio).max(scrollbar_min_thumb_size);
            let range = (safe_height - thumb_len).max(0.0);

            let track_rect = RectDIP {
                x: x + safe_width - scrollbar_size,
                y,
                width: scrollbar_size,
                height: safe_height,
            };
            let thumb_rect = RectDIP {
                x: x + safe_width - scrollbar_size,
                y: y + range * progress,
                width: scrollbar_size,
                height: thumb_len,
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
            let scrollbar_size = scroll_metadata.animation.0.interpolate(
                shell,
                scrollbar_size.thin(),
                scrollbar_size.thick(),
                Instant::now(),
            );

            let scrollbar_min_thumb_size = sc
                .scrollbar_min_thumb_size
                .unwrap_or(DEFAULT_SCROLLBAR_MIN_THUMB_SIZE);

            let scroll_x = scroll_metadata.position.x;
            let effective_scroll_x = scroll_x.clamp(0.0, max_scroll_x);
            let progress = if max_scroll_x > 0.0 {
                effective_scroll_x / max_scroll_x
            } else {
                0.0
            };
            let visible_ratio = (width / content_width).min(1.0);

            let safe_area_padding = sc.safe_area_padding.unwrap_or_default();
            let x = element.x + safe_area_padding.left;
            let y = element.y + safe_area_padding.top;
            let safe_width = width - safe_area_padding.left - safe_area_padding.right;
            let safe_height = height - safe_area_padding.top - safe_area_padding.bottom;

            let thumb_len = (safe_width * visible_ratio).max(scrollbar_min_thumb_size);
            let range = (safe_width - thumb_len).max(0.0);

            let track_rect = RectDIP {
                x,
                y: y + safe_height - scrollbar_size,
                width: safe_width,
                height: scrollbar_size,
            };
            let thumb_rect = RectDIP {
                x: x + range * progress,
                y: y + safe_height - scrollbar_size,
                width: thumb_len,
                height: scrollbar_size,
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
