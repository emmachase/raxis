#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollPosition {
    pub x: f32,
    pub y: f32,
}

pub trait ScrollStateManager {
    fn get_scroll_position(&self, element_id: u64) -> ScrollPosition;
    fn set_scroll_position(&mut self, element_id: u64, position: ScrollPosition);
    fn update_scroll_position(
        &mut self,
        element_id: u64,
        delta_x: f32,
        delta_y: f32,
    ) -> ScrollPosition;
    fn apply_scroll_limits(
        &mut self,
        element_id: u64,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    );
    fn update_scroll_metadata(
        &mut self,
        element_id: u64,
        current_position: ScrollPosition,
        max_scroll_x: f32,
        max_scroll_y: f32,
        content_width: f32,
        content_height: f32,
    );
    fn was_at_bottom(&self, element_id: u64) -> bool;
    fn was_at_right(&self, element_id: u64) -> bool;
    fn get_previous_content_dimensions(&self, element_id: u64) -> (f32, f32);
}

#[derive(Clone, Default)]
pub struct NoScrollStateManager {}

impl ScrollStateManager for NoScrollStateManager {
    fn get_scroll_position(&self, _element_id: u64) -> ScrollPosition {
        ScrollPosition { x: 0.0, y: 0.0 }
    }

    fn set_scroll_position(&mut self, _element_id: u64, _position: ScrollPosition) {
        // Do nothing
    }

    fn update_scroll_position(
        &mut self,
        _element_id: u64,
        _delta_x: f32,
        _delta_y: f32,
    ) -> ScrollPosition {
        ScrollPosition { x: 0.0, y: 0.0 }
    }

    fn apply_scroll_limits(
        &mut self,
        _element_id: u64,
        _min_x: f32,
        _min_y: f32,
        _max_x: f32,
        _max_y: f32,
    ) {
        // Do nothing
    }

    fn update_scroll_metadata(
        &mut self,
        _element_id: u64,
        _current_position: ScrollPosition,
        _max_scroll_x: f32,
        _max_scroll_y: f32,
        _content_width: f32,
        _content_height: f32,
    ) {
        // Do nothing
    }

    fn was_at_bottom(&self, _element_id: u64) -> bool {
        false
    }

    fn was_at_right(&self, _element_id: u64) -> bool {
        false
    }

    fn get_previous_content_dimensions(&self, _element_id: u64) -> (f32, f32) {
        (0.0, 0.0)
    }
}
