use std::any::Any;
use std::fmt::Debug;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;

use crate::gfx::PointDIP;
use crate::gfx::command_recorder::CommandRecorder;
use crate::layout::UIArenas;
use crate::layout::model::{Color, Element};
use crate::math::easing::Easing;
use crate::widgets::{Bounds, Cursor, widget};
use crate::widgets::{Instance, Widget};
use crate::{Animation, RedrawRequest, Shell, with_state};

/// Toggle states for visual feedback
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToggleState {
    Normal,
    Hover,
    Active,
    Disabled,
}

/// Style configuration for toggle in a specific state
#[derive(Debug, Clone)]
pub struct ToggleStyle {
    pub track_color: Color,
    pub thumb_color: Color,
    pub thumb_border_color: Option<Color>,
}

impl Default for ToggleStyle {
    fn default() -> Self {
        Self {
            track_color: Color::from(0xE2E8F0FF), // Neutral-200
            thumb_color: Color::WHITE,
            thumb_border_color: None,
        }
    }
}

/// Complete style set for all toggle states (off and on)
#[derive(Debug, Clone)]
pub struct ToggleStyleSet {
    pub off_normal: ToggleStyle,
    pub off_hover: ToggleStyle,
    pub off_disabled: ToggleStyle,
    pub on_normal: ToggleStyle,
    pub on_hover: ToggleStyle,
    pub on_disabled: ToggleStyle,
}

impl Default for ToggleStyleSet {
    fn default() -> Self {
        Self {
            // Off states (unchecked)
            off_normal: ToggleStyle {
                track_color: Color::from(0xE2E8F0FF), // Neutral-200
                thumb_color: Color::WHITE,
                thumb_border_color: None,
            },
            off_hover: ToggleStyle {
                track_color: Color::from(0xCBD5E1FF), // Neutral-300
                thumb_color: Color::WHITE,
                thumb_border_color: None,
            },
            off_disabled: ToggleStyle {
                track_color: Color::from(0xF1F5F9FF),              // Neutral-100
                thumb_color: Color::from(0xF8FAFCFF),              // Neutral-50
                thumb_border_color: Some(Color::from(0xE2E8F0FF)), // Neutral-200
            },
            // On states (checked)
            on_normal: ToggleStyle {
                track_color: Color::from(0x0F172AFF), // Neutral-900
                thumb_color: Color::WHITE,
                thumb_border_color: None,
            },
            on_hover: ToggleStyle {
                track_color: Color::from(0x1E293BFF), // Neutral-800
                thumb_color: Color::WHITE,
                thumb_border_color: None,
            },
            on_disabled: ToggleStyle {
                track_color: Color::from(0x94A3B8FF),              // Neutral-400
                thumb_color: Color::from(0xF8FAFCFF),              // Neutral-50
                thumb_border_color: Some(Color::from(0xCBD5E1FF)), // Neutral-300
            },
        }
    }
}

pub type OnToggleFn<Message> = dyn Fn(bool, &mut UIArenas, &mut Shell<Message>);

/// Toggle/Switch widget with on/off state
pub struct Toggle<Message> {
    pub checked: bool,
    pub enabled: bool,
    pub on_toggle: Option<Box<OnToggleFn<Message>>>,
    pub styles: ToggleStyleSet,
    pub width: f32,
    pub height: f32,
    pub cursor: Option<Cursor>,
    pub animation_duration: Duration,
    pub animation_easing: Easing,
}

impl<Message> Debug for Toggle<Message> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Toggle")
            .field("checked", &self.checked)
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl<Message: 'static + Send> Toggle<Message> {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            enabled: true,
            on_toggle: None,
            styles: ToggleStyleSet::default(),
            width: 44.0,  // shadcn default width
            height: 24.0, // shadcn default height
            cursor: Some(Cursor::Pointer),
            animation_duration: Duration::from_millis(200),
            animation_easing: Easing::EaseOutCubic,
        }
    }

    pub fn with_toggle_handler(
        mut self,
        handler: impl Fn(bool, &mut UIArenas, &mut Shell<Message>) + 'static,
    ) -> Self {
        self.on_toggle = Some(Box::new(handler));
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

    pub fn with_styles(mut self, styles: ToggleStyleSet) -> Self {
        self.styles = styles;
        self
    }

    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn with_cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn with_track_colors(mut self, off_color: Color, on_color: Color) -> Self {
        // Off states
        self.styles.off_normal.track_color = off_color;
        self.styles.off_hover.track_color = off_color.deviate(0.05);

        // On states
        self.styles.on_normal.track_color = on_color;
        self.styles.on_hover.track_color = on_color.deviate(0.05);
        self
    }

    pub fn with_thumb_color(mut self, color: Color) -> Self {
        self.styles.off_normal.thumb_color = color;
        self.styles.off_hover.thumb_color = color;
        self.styles.on_normal.thumb_color = color;
        self.styles.on_hover.thumb_color = color;
        self
    }

    pub fn with_animation_duration(mut self, duration: Duration) -> Self {
        self.animation_duration = duration;
        self
    }

    pub fn with_animation_easing(mut self, easing: Easing) -> Self {
        self.animation_easing = easing;
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

impl<Message: 'static + Send> Default for Toggle<Message> {
    fn default() -> Self {
        Self::new(false)
    }
}

struct ToggleWidgetState {
    state: ToggleState,
    is_hover: bool,
    animation: Animation<bool>,
}

impl ToggleWidgetState {
    pub fn new(initial_checked: bool, duration: Duration, easing: Easing) -> Self {
        Self {
            state: ToggleState::Normal,
            is_hover: false,
            animation: Animation::new(initial_checked)
                .duration(duration)
                .easing(easing),
        }
    }

    pub fn into_any(self) -> Box<dyn Any> {
        Box::new(self)
    }

    fn update_state(&mut self, enabled: bool) {
        self.state = if !enabled {
            ToggleState::Disabled
        } else if self.is_hover {
            ToggleState::Hover
        } else {
            ToggleState::Normal
        };
    }

    fn get_current_style<'a>(&self, styles: &'a ToggleStyleSet, checked: bool) -> &'a ToggleStyle {
        match (checked, self.state) {
            (false, ToggleState::Normal) => &styles.off_normal,
            (false, ToggleState::Hover) => &styles.off_hover,
            (false, ToggleState::Disabled) => &styles.off_disabled,
            (false, ToggleState::Active) => &styles.off_hover,
            (true, ToggleState::Normal) => &styles.on_normal,
            (true, ToggleState::Hover) => &styles.on_hover,
            (true, ToggleState::Disabled) => &styles.on_disabled,
            (true, ToggleState::Active) => &styles.on_hover,
        }
    }
}

impl<Message> Widget<Message> for Toggle<Message> {
    fn state(
        &self,
        _arenas: &UIArenas,
        _device_resources: &crate::runtime::DeviceResources,
    ) -> super::State {
        Some(ToggleWidgetState::new(self.checked, self.animation_duration, self.animation_easing).into_any())
    }

    fn limits_x(
        &self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
    ) -> super::limit_response::SizingForX {
        super::limit_response::SizingForX {
            min_width: self.width,
            preferred_width: self.width,
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
            min_height: self.height,
            preferred_height: self.height,
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
        let state = with_state!(mut instance as ToggleWidgetState);

        match event {
            super::Event::MouseButtonDown { x, y, .. } => {
                let point = PointDIP { x: *x, y: *y };
                if point.within(bounds.border_box) && self.enabled {
                    // Toggle the state
                    self.checked = !self.checked;

                    // Update animation to new state
                    state.animation.update(self.checked);

                    // Fire toggle callback
                    if let Some(handler) = self.on_toggle.as_ref() {
                        handler(self.checked, arenas, shell);
                    }

                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            super::Event::MouseMove { x, y } | super::Event::MouseEnter { x, y } => {
                let point = PointDIP { x: *x, y: *y };
                let was_hover = state.is_hover;
                state.is_hover = point.within(bounds.border_box);

                if was_hover != state.is_hover {
                    state.update_state(self.enabled);
                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            super::Event::MouseLeave { .. } => {
                let was_hover = state.is_hover;
                state.is_hover = false;

                if was_hover {
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
        shell: &mut Shell<Message>,
        recorder: &mut CommandRecorder,
        _style: crate::layout::model::ElementStyle,
        bounds: Bounds,
        now: Instant,
    ) {
        let state = with_state!(mut instance as ToggleWidgetState);
        state.update_state(self.enabled);

        // Determine the target style based on current checked state
        let cur_style = state.get_current_style(&self.styles, self.checked);

        // Track dimensions (full rounded pill)
        let track_x = bounds.content_box.x;
        let track_y = bounds.content_box.y;
        let track_width = self.width.min(bounds.content_box.width);
        let track_height = self.height.min(bounds.content_box.height);
        let track_radius = track_height / 2.0;

        // Interpolate track color during animation
        let off_style = state.get_current_style(&self.styles, false);
        let on_style = state.get_current_style(&self.styles, true);
        let track_color =
            state
                .animation
                .interpolate(shell, off_style.track_color, on_style.track_color, now);

        // Draw track
        recorder.fill_rounded_rectangle(
            &crate::gfx::RectDIP {
                x: track_x,
                y: track_y,
                width: track_width,
                height: track_height,
            },
            &crate::layout::model::BorderRadius::all(track_radius),
            track_color,
        );

        // Thumb dimensions
        let thumb_size = track_height - 4.0; // 2px padding on each side
        let thumb_radius = thumb_size / 2.0;
        let thumb_padding = 2.0;

        // Calculate target thumb positions
        let thumb_x_off = track_x + thumb_padding;
        let thumb_x_on = track_x + track_width - thumb_size - thumb_padding;

        // Interpolate thumb position during animation
        let thumb_x = state
            .animation
            .interpolate(shell, thumb_x_off, thumb_x_on, now);

        let thumb_y = track_y + thumb_padding;

        // Draw thumb shadow for depth
        if self.enabled {
            recorder.draw_blurred_shadow(
                &crate::gfx::RectDIP {
                    x: thumb_x,
                    y: thumb_y,
                    width: thumb_size,
                    height: thumb_size,
                },
                &crate::layout::model::DropShadow {
                    blur_radius: 3.0,
                    spread_radius: 0.0,
                    offset_x: 0.0,
                    offset_y: 1.0,
                    color: Color::from(0x00000026), // Black with 15% opacity
                    inset: false,
                },
                Some(&crate::layout::model::BorderRadius::all(thumb_radius)),
            );
        }

        // Draw thumb
        recorder.fill_rounded_rectangle(
            &crate::gfx::RectDIP {
                x: thumb_x,
                y: thumb_y,
                width: thumb_size,
                height: thumb_size,
            },
            &crate::layout::model::BorderRadius::all(thumb_radius),
            cur_style.thumb_color,
        );

        // Draw thumb border if specified
        if let Some(border_color) = cur_style.thumb_border_color {
            recorder.draw_rounded_rectangle_stroked(
                &crate::gfx::RectDIP {
                    x: thumb_x,
                    y: thumb_y,
                    width: thumb_size,
                    height: thumb_size,
                },
                &crate::layout::model::BorderRadius::all(thumb_radius),
                border_color,
                1.0,
            );
        }

        // Add subtle focus ring on hover
        if state.state == ToggleState::Hover && self.enabled {
            recorder.draw_rounded_rectangle_stroked(
                &crate::gfx::RectDIP {
                    x: track_x - 2.0,
                    y: track_y - 2.0,
                    width: track_width + 4.0,
                    height: track_height + 4.0,
                },
                &crate::layout::model::BorderRadius::all(track_radius + 2.0),
                Color::from(0x0000001A), // Black with 10% opacity
                2.0,
            );
        }
    }

    fn cursor(
        &self,
        _arenas: &UIArenas,
        _instance: &Instance,
        point: PointDIP,
        bounds: Bounds,
    ) -> Option<super::Cursor> {
        if point.within(bounds.border_box) && self.enabled {
            self.cursor
        } else {
            None
        }
    }
}
