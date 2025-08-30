use std::{any::Any, time::Instant};

use windows_numerics::Vector2;

use crate::{
    Shell,
    math::easing::Easing,
    widgets::{Bounds, Instance, Widget},
    with_state,
};

// A simple spinner widget built on top of CircleArc. It animates the arc fill
// between 10% and 90% while the start angle rotates slowly. It encapsulates its
// own animation state to avoid leaking logic into callers.
#[derive(Debug)]
pub struct Spinner {
    // layout
    center: Vector2,
    radius: f32,
    stroke: f32,

    // config
    base_speed_dps: f32,
    grow_period_s: f32, // full grow+shrink cycle duration
    extent: f32,        // min fill fraction (0..0.5); max is 1 - extent
    easing: Easing,     // easing for the phase interpolation
}

impl Default for Spinner {
    fn default() -> Self {
        Self {
            center: Vector2 { X: 0.0, Y: 0.0 },
            radius: 50.0,
            stroke: 2.0,

            base_speed_dps: 300.0,
            grow_period_s: 1.6,
            extent: 0.05,
            easing: Easing::EaseInOut,
        }
    }
}

#[derive(Debug)]
struct WidgetState {
    anchor_deg: f32,    // the anchored endpoint angle (deg)
    phase_elapsed: f32, // time within current half-cycle [0, half)
    is_growing: bool,   // true: growing (10%->90%), false: shrinking (90%->10%)
    last_update: Instant,
}

impl WidgetState {
    pub fn new() -> Self {
        Self {
            anchor_deg: 0.0,
            phase_elapsed: 0.0,
            is_growing: true,
            last_update: Instant::now(),
        }
    }

    pub fn into_any(self) -> Box<dyn Any> {
        Box::new(self)
    }
}

impl Default for WidgetState {
    fn default() -> Self {
        Self::new()
    }
}

impl<Message> Widget<Message> for Spinner {
    fn state(&self, _device_resources: &crate::runtime::DeviceResources) -> super::State {
        Some(WidgetState::default().into_any())
    }

    fn limits_x(&self, _instance: &mut Instance) -> super::limit_response::SizingForX {
        super::limit_response::SizingForX {
            min_width: self.radius * 2.0,
            preferred_width: self.radius * 2.0,
        }
    }

    fn limits_y(&self, _instance: &mut Instance, _width: f32) -> super::limit_response::SizingForY {
        super::limit_response::SizingForY {
            min_height: self.radius * 2.0,
            preferred_height: self.radius * 2.0,
        }
    }

    fn paint(
        &mut self,
        instance: &mut Instance,
        _shell: &Shell<Message>,
        recorder: &mut crate::gfx::command_recorder::CommandRecorder,
        bounds: Bounds,
        now: Instant,
    ) {
        let state = with_state!(mut instance as WidgetState);
        let center = Vector2 {
            X: bounds.content_box.x_dip + bounds.content_box.width_dip * 0.5,
            Y: bounds.content_box.y_dip + bounds.content_box.height_dip * 0.5,
        };
        // let radius = bounds.width_dip.min(bounds.height_dip) * 0.5;
        // self.set_layout(center, radius);
        self.center = center;
        state.update(self, now);

        // Draw spinner using command recorder
        let half = self.grow_period_s * 0.5;
        let p = (state.phase_elapsed / half).clamp(0.0, 1.0);
        let p = self.easing.apply(p);
        let min = self.extent;
        let max = 1.0 - self.extent;
        let span = (max - min).max(0.0);
        let fill_frac = if state.is_growing {
            min + span * p
        } else {
            max - span * p
        };

        let (begin_deg, end_deg) = if state.is_growing {
            let begin = state.anchor_deg;
            (begin, begin + 360.0 * fill_frac)
        } else {
            let end = state.anchor_deg;
            (end - 360.0 * fill_frac, end)
        };

        // Record circle arc drawing command
        recorder.draw_circle_arc(
            center,
            self.radius - self.stroke * 0.5,
            begin_deg,
            end_deg,
            self.stroke,
            crate::widgets::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        );
    }

    fn update(
        &mut self,
        _instance: &mut Instance,
        hwnd: windows::Win32::Foundation::HWND,
        shell: &mut Shell<Message>,
        event: &super::Event,
        _bounds: Bounds,
    ) {
        if matches!(event, super::Event::Redraw { .. }) {
            shell.request_redraw(hwnd, crate::RedrawRequest::Immediate);
        }
    }
}

impl Spinner {
    pub fn new(stroke: f32, base_speed_dps: f32, grow_period_s: f32, radius: f32) -> Self {
        Self {
            center: Vector2 { X: 0.0, Y: 0.0 },
            radius,
            stroke,
            base_speed_dps,
            grow_period_s,
            extent: 0.05,
            easing: Easing::EaseInOut,
        }
    }

    // Symmetric configuration via a single value: max = 1 - extent.
    // extent is clamped to [0, 0.5] so min <= max.
    pub fn set_extent(&mut self, min_frac: f32) {
        self.extent = min_frac.clamp(0.0, 0.5);
    }

    pub fn set_easing(&mut self, easing: Easing) {
        self.easing = easing;
    }
}

impl WidgetState {
    pub fn update(&mut self, config: &Spinner, now: Instant) {
        let dt_seconds = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        let half = config.grow_period_s * 0.5;
        // advance slow rotation
        self.anchor_deg += config.base_speed_dps * dt_seconds;
        // advance phase and handle boundaries
        self.phase_elapsed += dt_seconds;
        while self.phase_elapsed >= half {
            self.phase_elapsed -= half;
            if self.is_growing {
                // Grow finished at max: end becomes the new anchor
                self.anchor_deg += 360.0 * (1.0 - config.extent);
            } else {
                // Shrink finished at min: begin becomes the new anchor
                self.anchor_deg -= 360.0 * config.extent;
            }
            self.is_growing = !self.is_growing;
        }
        // normalize
        while self.anchor_deg >= 360.0 {
            self.anchor_deg -= 360.0;
        }
        while self.anchor_deg < 0.0 {
            self.anchor_deg += 360.0;
        }
    }
}
