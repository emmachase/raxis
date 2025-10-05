use std::collections::HashMap;

use crate::{Animation, layout::model::Axis};

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
