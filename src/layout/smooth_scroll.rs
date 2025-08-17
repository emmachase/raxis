use std::collections::HashMap;
use std::time::Instant;

use super::scroll_manager::ScrollPosition;
use crate::math::easing::Easing;

#[derive(Clone, Copy, Debug)]
pub struct SmoothScrollAnimation {
    pub start_position: ScrollPosition,
    pub target_position: ScrollPosition,
    pub start_time: Instant,
    pub duration_ms: u32,
    pub easing: Easing,
}

pub const SMOOTH_SCROLL_DURATION_MS: u32 = 250;

impl SmoothScrollAnimation {
    pub fn new(
        start: ScrollPosition,
        target: ScrollPosition,
        duration_ms: u32,
        easing: Easing,
    ) -> Self {
        Self {
            start_position: start,
            target_position: target,
            start_time: Instant::now(),
            duration_ms,
            easing,
        }
    }

    pub fn current_position(&self, now: Instant) -> ScrollPosition {
        let elapsed = now.duration_since(self.start_time).as_millis() as f32;
        let progress = (elapsed / self.duration_ms as f32).clamp(0.0, 1.0);
        let eased_progress = self.easing.apply(progress);

        ScrollPosition {
            x: self.start_position.x
                + (self.target_position.x - self.start_position.x) * eased_progress,
            y: self.start_position.y
                + (self.target_position.y - self.start_position.y) * eased_progress,
        }
    }

    pub fn is_complete(&self, now: Instant) -> bool {
        now.duration_since(self.start_time).as_millis() >= self.duration_ms as u128
    }

    pub fn accumulate_target(&mut self, delta: ScrollPosition, now: Instant) {
        let current_pos = self.current_position(now);

        // Calculate current direction of animation
        let current_direction_x = (self.target_position.x - current_pos.x).signum();
        let current_direction_y = (self.target_position.y - current_pos.y).signum();

        // Calculate new direction from delta
        let new_direction_x = delta.x.signum();
        let new_direction_y = delta.y.signum();

        // Check if direction changed (and we're not at zero velocity)
        let direction_changed_x = delta.x.abs() > 0.1
            && current_direction_x != 0.0
            && current_direction_x != new_direction_x;
        let direction_changed_y = delta.y.abs() > 0.1
            && current_direction_y != 0.0
            && current_direction_y != new_direction_y;

        if direction_changed_x || direction_changed_y {
            // Direction changed, restart animation from current position
            self.start_position = current_pos;
            self.target_position = ScrollPosition {
                x: current_pos.x + delta.x,
                y: current_pos.y + delta.y,
            };
            self.start_time = now;
        } else {
            // Same direction or starting from rest, accumulate delta
            self.start_position = current_pos;
            self.target_position.x += delta.x;
            self.target_position.y += delta.y;
            self.start_time = now;
        }
    }
}

#[derive(Clone, Default)]
pub struct SmoothScrollManager {
    animations: HashMap<u64, SmoothScrollAnimation>,
    default_duration_ms: u32,
    default_easing: Easing,
}

impl SmoothScrollManager {
    pub fn new() -> Self {
        Self {
            animations: HashMap::new(),
            default_duration_ms: SMOOTH_SCROLL_DURATION_MS,
            default_easing: Easing::EaseOutCubic,
        }
    }

    pub fn with_defaults(duration_ms: u32, easing: Easing) -> Self {
        Self {
            animations: HashMap::new(),
            default_duration_ms: duration_ms,
            default_easing: easing,
        }
    }

    pub fn accumulate_scroll_delta(
        &mut self,
        element_id: u64,
        current_pos: ScrollPosition,
        delta: ScrollPosition,
    ) {
        let now = Instant::now();

        if let Some(existing) = self.animations.get_mut(&element_id) {
            // Accumulate delta to existing animation
            existing.accumulate_target(delta, now);
        } else {
            // Create new animation with delta
            let target_pos = ScrollPosition {
                x: current_pos.x + delta.x,
                y: current_pos.y + delta.y,
            };
            let animation = SmoothScrollAnimation::new(
                current_pos,
                target_pos,
                self.default_duration_ms,
                self.default_easing,
            );
            self.animations.insert(element_id, animation);
        }
    }

    pub fn get_current_position(
        &self,
        element_id: u64,
        fallback: ScrollPosition,
    ) -> ScrollPosition {
        let now = Instant::now();
        if let Some(animation) = self.animations.get(&element_id) {
            animation.current_position(now)
        } else {
            fallback
        }
    }

    pub fn update_animations(&mut self) -> Vec<u64> {
        let now = Instant::now();
        let mut completed_animations = Vec::new();

        // Remove completed animations and collect their element IDs
        self.animations.retain(|&element_id, animation| {
            if animation.is_complete(now) {
                completed_animations.push(element_id);
                false
            } else {
                true
            }
        });

        completed_animations
    }

    pub fn has_active_animation(&self, element_id: u64) -> bool {
        self.animations.contains_key(&element_id)
    }

    pub fn stop_animation(&mut self, element_id: u64) {
        self.animations.remove(&element_id);
    }

    pub fn has_any_active_animations(&self) -> bool {
        !self.animations.is_empty()
    }

    pub fn get_active_animation_count(&self) -> usize {
        self.animations.len()
    }

    pub fn clear_all_animations(&mut self) {
        self.animations.clear();
    }

    pub fn set_default_duration(&mut self, duration_ms: u32) {
        self.default_duration_ms = duration_ms;
    }

    pub fn set_default_easing(&mut self, easing: Easing) {
        self.default_easing = easing;
    }

    pub fn get_active_animations(&self) -> impl Iterator<Item = (&u64, &SmoothScrollAnimation)> {
        self.animations.iter()
    }
}
