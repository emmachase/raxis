use std::any::Any;
use std::fmt::Debug;
use std::time::Instant;

use windows::Win32::Foundation::HWND;

use crate::gfx::PointDIP;
use crate::gfx::command_recorder::CommandRecorder;
use crate::layout::UIArenas;
use crate::layout::model::{Color, Element};
use crate::widgets::{Bounds, Cursor, widget};
use crate::widgets::{Instance, Widget};
use crate::{RedrawRequest, Shell, with_state};

/// Slider states for visual feedback
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SliderState {
    Normal,
    Hover,
    Dragging,
    Disabled,
}

/// Style configuration for slider in a specific state
#[derive(Debug, Clone)]
pub struct SliderStyle {
    pub track_color: Color,
    pub filled_track_color: Color,
    pub thumb_color: Color,
    pub thumb_border_color: Option<Color>,
}

impl Default for SliderStyle {
    fn default() -> Self {
        Self {
            track_color: Color::from(0xE2E8F0FF),        // Neutral-200
            filled_track_color: Color::from(0x0F172AFF), // Neutral-900
            thumb_color: Color::WHITE,
            thumb_border_color: Some(Color::from(0x0F172AFF)), // Neutral-900
        }
    }
}

/// Complete style set for all slider states
#[derive(Debug, Clone)]
pub struct SliderStyleSet {
    pub normal: SliderStyle,
    pub hover: SliderStyle,
    pub dragging: SliderStyle,
    pub disabled: SliderStyle,
}

impl Default for SliderStyleSet {
    fn default() -> Self {
        Self {
            normal: SliderStyle::default(),
            hover: SliderStyle {
                track_color: Color::from(0xE2E8F0FF),
                filled_track_color: Color::from(0x0F172AFF),
                thumb_color: Color::WHITE,
                thumb_border_color: Some(Color::from(0x0F172AFF)),
            },
            dragging: SliderStyle {
                track_color: Color::from(0xE2E8F0FF),
                filled_track_color: Color::from(0x0F172AFF),
                thumb_color: Color::WHITE,
                thumb_border_color: Some(Color::from(0x0F172AFF)),
            },
            disabled: SliderStyle {
                track_color: Color::from(0xF1F5F9FF),        // Lighter neutral
                filled_track_color: Color::from(0x94A3B8FF), // Neutral-400
                thumb_color: Color::from(0xF8FAFCFF),        // Neutral-50
                thumb_border_color: Some(Color::from(0xCBD5E1FF)), // Neutral-300
            },
        }
    }
}

pub type OnValueChangeFn<Message> = dyn Fn(f32, &mut UIArenas, &mut Shell<Message>);
pub type OnDragFn<Message> = dyn Fn(bool, &mut UIArenas, &mut Shell<Message>);

/// Slider widget with value tracking and change handling
pub struct Slider<Message> {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    pub step: Option<f32>,
    pub enabled: bool,
    pub on_value_change: Option<Box<OnValueChangeFn<Message>>>,
    pub on_drag: Option<Box<OnDragFn<Message>>>,
    pub styles: SliderStyleSet,
    pub track_height: f32,
    pub thumb_size: f32,
    pub cursor: Option<Cursor>,
}

impl<Message> Debug for Slider<Message> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slider")
            .field("min", &self.min)
            .field("max", &self.max)
            .field("value", &self.value)
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl<Message: 'static + Send> Slider<Message> {
    pub fn new(min: f32, max: f32, value: f32) -> Self {
        Self {
            min,
            max,
            value: value.clamp(min, max),
            step: None,
            enabled: true,
            on_value_change: None,
            on_drag: None,
            styles: SliderStyleSet::default(),
            track_height: 4.0,
            thumb_size: 16.0,
            cursor: Some(Cursor::Pointer),
        }
    }

    pub fn with_step(mut self, step: f32) -> Self {
        self.step = Some(step);
        self
    }

    pub fn with_value_change_handler(
        mut self,
        handler: impl Fn(f32, &mut UIArenas, &mut Shell<Message>) + 'static,
    ) -> Self {
        self.on_value_change = Some(Box::new(handler));
        self
    }

    pub fn with_drag_handler(
        mut self,
        handler: impl Fn(bool, &mut UIArenas, &mut Shell<Message>) + 'static,
    ) -> Self {
        self.on_drag = Some(Box::new(handler));
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_styles(mut self, styles: SliderStyleSet) -> Self {
        self.styles = styles;
        self
    }

    pub fn with_track_height(mut self, height: f32) -> Self {
        self.track_height = height;
        self
    }

    pub fn with_thumb_size(mut self, size: f32) -> Self {
        self.thumb_size = size;
        self
    }

    pub fn with_cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn with_track_color(mut self, color: Color) -> Self {
        self.styles.normal.track_color = color;
        self.styles.hover.track_color = color;
        self.styles.dragging.track_color = color;
        self
    }

    pub fn with_filled_track_color(mut self, color: Color) -> Self {
        self.styles.normal.filled_track_color = color;
        self.styles.hover.filled_track_color = color;
        self.styles.dragging.filled_track_color = color;
        self
    }

    pub fn with_thumb_color(mut self, color: Color) -> Self {
        self.styles.normal.thumb_color = color;
        self.styles.hover.thumb_color = color;
        self.styles.dragging.thumb_color = color;
        self
    }

    pub fn with_thumb_border_color(mut self, color: Color) -> Self {
        self.styles.normal.thumb_border_color = Some(color);
        self.styles.hover.thumb_border_color = Some(color);
        self.styles.dragging.thumb_border_color = Some(color);
        self
    }

    pub fn as_element(self, id: u64) -> Element<Message> {
        Element {
            id: Some(id),
            content: widget(self),
            ..Default::default()
        }
    }
}

impl<Message: 'static + Send> Default for Slider<Message> {
    fn default() -> Self {
        Self::new(0.0, 100.0, 0.0)
    }
}

// Helper methods that don't require Message bounds
impl<Message> Slider<Message> {
    fn snap_to_step(&self, value: f32) -> f32 {
        if let Some(step) = self.step {
            let steps = ((value - self.min) / step).round();
            (self.min + steps * step).clamp(self.min, self.max)
        } else {
            value
        }
    }

    fn value_from_x(&self, x: f32, bounds: Bounds) -> f32 {
        let track_left = bounds.content_box.x + self.thumb_size / 2.0;
        let track_right = bounds.content_box.x + bounds.content_box.width - self.thumb_size / 2.0;
        let track_width = track_right - track_left;

        if track_width <= 0.0 {
            return self.min;
        }

        let ratio = ((x - track_left) / track_width).clamp(0.0, 1.0);
        let value = self.min + ratio * (self.max - self.min);
        self.snap_to_step(value)
    }
}

struct SliderWidgetState {
    state: SliderState,
    is_dragging: bool,
    is_hover: bool,
}

impl SliderWidgetState {
    pub fn new() -> Self {
        Self {
            state: SliderState::Normal,
            is_dragging: false,
            is_hover: false,
        }
    }

    pub fn into_any(self) -> Box<dyn Any> {
        Box::new(self)
    }

    fn update_state(&mut self, enabled: bool) {
        self.state = if !enabled {
            SliderState::Disabled
        } else if self.is_dragging {
            SliderState::Dragging
        } else if self.is_hover {
            SliderState::Hover
        } else {
            SliderState::Normal
        };
    }

    fn get_current_style<'a>(&self, styles: &'a SliderStyleSet) -> &'a SliderStyle {
        match self.state {
            SliderState::Normal => &styles.normal,
            SliderState::Hover => &styles.hover,
            SliderState::Dragging => &styles.dragging,
            SliderState::Disabled => &styles.disabled,
        }
    }
}

impl<Message> Widget<Message> for Slider<Message> {
    fn state(
        &self,
        _arenas: &UIArenas,
        _device_resources: &crate::runtime::DeviceResources,
    ) -> super::State {
        Some(SliderWidgetState::new().into_any())
    }

    fn limits_x(
        &self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
    ) -> super::limit_response::SizingForX {
        super::limit_response::SizingForX {
            min_width: self.thumb_size * 3.0,
            preferred_width: 200.0,
        }
    }

    fn limits_y(
        &self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
        _border_width: f32,
        _content_width: f32,
    ) -> super::limit_response::SizingForY {
        super::limit_response::SizingForY {
            min_height: self.thumb_size,
            preferred_height: self.thumb_size,
        }
    }

    fn update(
        &mut self,
        arenas: &mut UIArenas,
        instance: &mut Instance,
        hwnd: HWND,
        shell: &mut Shell<Message>,
        event: &super::Event,
        bounds: Bounds,
    ) {
        let state = with_state!(mut instance as SliderWidgetState);

        match event {
            super::Event::MouseButtonDown { x, y, .. } => {
                let point = PointDIP { x: *x, y: *y };
                if point.within(bounds.border_box) && self.enabled {
                    state.is_dragging = true;
                    state.is_hover = true;
                    state.update_state(self.enabled);

                    // Fire drag start callback
                    if let Some(handler) = self.on_drag.as_ref() {
                        handler(true, arenas, shell);
                    }

                    Cursor::Grabbing.set();

                    // Update value immediately on click
                    let new_value = self.value_from_x(*x, bounds);
                    if (new_value - self.value).abs() > f32::EPSILON {
                        self.value = new_value;
                        if let Some(handler) = self.on_value_change.as_ref() {
                            handler(self.value, arenas, shell);
                        }
                    }

                    shell.capture_event(instance.id);
                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            super::Event::MouseButtonUp { .. } => {
                if state.is_dragging {
                    state.is_dragging = false;
                    state.update_state(self.enabled);

                    // Fire drag end callback
                    if let Some(handler) = self.on_drag.as_ref() {
                        handler(false, arenas, shell);
                    }

                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            super::Event::MouseMove { x, y } | super::Event::MouseEnter { x, y } => {
                let point = PointDIP { x: *x, y: *y };
                let was_hover = state.is_hover;
                state.is_hover = point.within(bounds.border_box);

                if state.is_dragging && self.enabled {
                    // Update value while dragging
                    let new_value = self.value_from_x(*x, bounds);
                    if (new_value - self.value).abs() > f32::EPSILON {
                        self.value = new_value;
                        if let Some(handler) = self.on_value_change.as_ref() {
                            handler(self.value, arenas, shell);
                        }
                        shell.request_redraw(hwnd, RedrawRequest::Immediate);
                    }
                } else if was_hover != state.is_hover {
                    state.update_state(self.enabled);
                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            super::Event::MouseLeave { .. } => {
                let was_hover = state.is_hover;
                state.is_hover = false;

                if was_hover && !state.is_dragging {
                    state.update_state(self.enabled);
                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            _ => {}
        }
    }

    fn paint(
        &mut self,
        _arenas: &UIArenas,
        instance: &mut Instance,
        _shell: &Shell<Message>,
        recorder: &mut CommandRecorder,
        _style: crate::layout::model::ElementStyle,
        bounds: Bounds,
        _now: Instant,
    ) {
        let state = with_state!(mut instance as SliderWidgetState);
        state.update_state(self.enabled);
        let cur_style = state.get_current_style(&self.styles);

        // Calculate dimensions
        let track_left = bounds.content_box.x + self.thumb_size / 2.0;
        let track_right = bounds.content_box.x + bounds.content_box.width - self.thumb_size / 2.0;
        let track_width = track_right - track_left;
        let track_y = bounds.content_box.y + (bounds.content_box.height - self.track_height) / 2.0;

        // Calculate thumb position
        let value_ratio = if self.max > self.min {
            ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let thumb_x = track_left + value_ratio * track_width;
        let thumb_y = bounds.content_box.y + bounds.content_box.height / 2.0;

        // Draw track (background)
        recorder.fill_rounded_rectangle(
            &crate::gfx::RectDIP {
                x: track_left,
                y: track_y,
                width: track_width,
                height: self.track_height,
            },
            &crate::layout::model::BorderRadius::all(self.track_height / 2.0),
            cur_style.track_color,
        );

        // Draw filled track (progress)
        let filled_width = value_ratio * track_width;
        if filled_width > 0.0 {
            recorder.fill_rounded_rectangle(
                &crate::gfx::RectDIP {
                    x: track_left,
                    y: track_y,
                    width: filled_width,
                    height: self.track_height,
                },
                &crate::layout::model::BorderRadius::all(self.track_height / 2.0),
                cur_style.filled_track_color,
            );
        }

        // Draw thumb
        let thumb_radius = self.thumb_size / 2.0;
        recorder.fill_rounded_rectangle(
            &crate::gfx::RectDIP {
                x: thumb_x - thumb_radius,
                y: thumb_y - thumb_radius,
                width: self.thumb_size,
                height: self.thumb_size,
            },
            &crate::layout::model::BorderRadius::all(thumb_radius),
            cur_style.thumb_color,
        );

        // Draw thumb border
        if let Some(border_color) = cur_style.thumb_border_color {
            recorder.draw_rounded_rectangle_stroked(
                &crate::gfx::RectDIP {
                    x: thumb_x - thumb_radius,
                    y: thumb_y - thumb_radius,
                    width: self.thumb_size,
                    height: self.thumb_size,
                },
                &crate::layout::model::BorderRadius::all(thumb_radius),
                border_color,
                2.0,
            );
        }

        // Add subtle shadow on thumb when hovering or dragging
        if state.state == SliderState::Hover || state.state == SliderState::Dragging {
            recorder.draw_blurred_shadow(
                &crate::gfx::RectDIP {
                    x: thumb_x - thumb_radius,
                    y: thumb_y - thumb_radius,
                    width: self.thumb_size,
                    height: self.thumb_size,
                },
                &crate::layout::model::DropShadow {
                    blur_radius: 4.0,
                    spread_radius: 0.0,
                    offset_x: 0.0,
                    offset_y: 1.0,
                    color: Color::from(0x0000001A), // Black with 10% opacity
                },
                Some(&crate::layout::model::BorderRadius::all(thumb_radius)),
            );
        }
    }

    fn cursor(
        &self,
        _arenas: &UIArenas,
        instance: &Instance,
        point: PointDIP,
        bounds: Bounds,
    ) -> Option<super::Cursor> {
        if point.within(bounds.border_box) && self.enabled {
            let state = with_state!(instance as SliderWidgetState);
            if state.is_dragging {
                Some(Cursor::Grabbing)
            } else {
                self.cursor
            }
        } else {
            None
        }
    }
}
