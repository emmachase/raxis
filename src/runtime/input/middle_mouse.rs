/// State for middle mouse button scrolling (pan mode)
#[derive(Clone, Copy, Debug)]
pub struct MiddleMouseScrollState {
    pub element_id: u64,
    /// Origin position where middle mouse was clicked, in DIPs
    pub origin_x: f32,
    pub origin_y: f32,
    /// Current mouse position, in DIPs
    pub current_x: f32,
    pub current_y: f32,
}

impl MiddleMouseScrollState {
    /// Calculates velocity based on distance from origin using quadratic scaling
    pub fn calculate_velocity(&self, dt: f64) -> (f32, f32) {
        let delta_x = self.current_x - self.origin_x;
        let delta_y = self.current_y - self.origin_y;

        // Use quadratic scaling for better control: slow near origin, faster further away
        const BASE_SPEED: f32 = 30.0; // pixels per second at 100px distance
        let velocity_x = delta_x * delta_x.abs() * BASE_SPEED * dt as f32 / 100.0;
        let velocity_y = delta_y * delta_y.abs() * BASE_SPEED * dt as f32 / 100.0;

        (velocity_x, velocity_y)
    }
}
