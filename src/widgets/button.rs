use std::any::Any;
use std::fmt::Debug;
use std::time::Instant;

use windows::Win32::Foundation::HWND;

use crate::gfx::{PointDIP, RectDIP};
use crate::layout::UIArenas;
use crate::layout::model::{Border, BorderRadius, Element};
use crate::widgets::{Bounds, Color, widget};
use crate::widgets::{Instance, Widget};
use crate::{RedrawRequest, Shell, with_state};

/// Button states for visual feedback
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonState {
    Normal,
    Hover,
    Pressed,
    Disabled,
}

/// Style configuration for a button in a specific state
#[derive(Debug, Clone)]
pub struct ButtonStyle {
    pub bg_color: Color,
    pub text_color: Color,
    pub border: Option<Border>,
    pub border_radius: Option<BorderRadius>,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            bg_color: Color {
                r: 0.9,
                g: 0.9,
                b: 0.9,
                a: 1.0,
            },
            text_color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            border: Some(Border {
                width: 1.0,
                color: Color {
                    r: 0.7,
                    g: 0.7,
                    b: 0.7,
                    a: 1.0,
                },
                ..Default::default()
            }),
            border_radius: None,
        }
    }
}

/// Complete style set for all button states
#[derive(Debug, Clone)]
pub struct ButtonStyleSet {
    pub normal: ButtonStyle,
    pub hover: ButtonStyle,
    pub pressed: ButtonStyle,
    pub disabled: ButtonStyle,
}

impl Default for ButtonStyleSet {
    fn default() -> Self {
        Self {
            normal: ButtonStyle::default(),
            hover: ButtonStyle {
                bg_color: Color {
                    r: 0.85,
                    g: 0.85,
                    b: 0.85,
                    a: 1.0,
                },
                text_color: Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
                border: Some(Border {
                    width: 1.0,
                    color: Color {
                        r: 0.6,
                        g: 0.6,
                        b: 0.6,
                        a: 1.0,
                    },
                    ..Default::default()
                }),
                border_radius: None,
            },
            pressed: ButtonStyle {
                bg_color: Color {
                    r: 0.75,
                    g: 0.75,
                    b: 0.75,
                    a: 1.0,
                },
                text_color: Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
                border: Some(Border {
                    width: 1.0,
                    color: Color {
                        r: 0.5,
                        g: 0.5,
                        b: 0.5,
                        a: 1.0,
                    },
                    ..Default::default()
                }),
                border_radius: None,
            },
            disabled: ButtonStyle {
                bg_color: Color {
                    r: 0.95,
                    g: 0.95,
                    b: 0.95,
                    a: 1.0,
                },
                text_color: Color {
                    r: 0.6,
                    g: 0.6,
                    b: 0.6,
                    a: 1.0,
                },
                border: Some(Border {
                    width: 1.0,
                    color: Color {
                        r: 0.8,
                        g: 0.8,
                        b: 0.8,
                        a: 1.0,
                    },
                    ..Default::default()
                }),
                border_radius: None,
            },
        }
    }
}

pub type OnClickFn<Message> = dyn Fn(&mut UIArenas, &mut Shell<Message>);

/// Button widget with text label and click handling
pub struct Button<Message> {
    pub enabled: bool,
    pub on_click: Option<Box<OnClickFn<Message>>>,
    pub styles: ButtonStyleSet,
}

impl<Message> Debug for Button<Message> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Button")
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl<Message: 'static + Send> Button<Message> {
    pub fn new() -> Self {
        Self {
            enabled: true,
            on_click: None,
            styles: ButtonStyleSet::default(),
        }
    }

    pub fn with_click_handler(
        mut self,
        handler: impl Fn(&mut UIArenas, &mut Shell<Message>) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn with_styles(mut self, styles: ButtonStyleSet) -> Self {
        self.styles = styles;
        self
    }

    pub fn with_bg_color(mut self, color: Color) -> Self {
        self.styles.normal.bg_color = color;
        self.styles.hover.bg_color = Color {
            r: color.r * 0.9,
            g: color.g * 0.9,
            b: color.b * 0.9,
            a: color.a,
        };
        self.styles.pressed.bg_color = Color {
            r: color.r * 0.8,
            g: color.g * 0.8,
            b: color.b * 0.8,
            a: color.a,
        };
        self
    }

    pub fn with_border_radius(mut self, radius: f32) -> Self {
        let border_radius = BorderRadius::all(radius);
        self.styles.normal.border_radius = Some(border_radius);
        self.styles.hover.border_radius = Some(border_radius);
        self.styles.pressed.border_radius = Some(border_radius);
        self.styles.disabled.border_radius = Some(border_radius);
        self
    }

    pub fn with_border(mut self, width: f32, color: Color) -> Self {
        let border = Border {
            width,
            color,
            ..Default::default()
        };
        self.styles.normal.border = Some(border.clone());
        self.styles.hover.border = Some(border.clone());
        self.styles.pressed.border = Some(border.clone());
        self.styles.disabled.border = Some(border);
        self
    }

    pub fn with_no_border(mut self) -> Self {
        self.styles.normal.border = None;
        self.styles.hover.border = None;
        self.styles.pressed.border = None;
        self.styles.disabled.border = None;
        self
    }

    pub fn as_element(self, id: u64, children: impl Into<Element<Message>>) -> Element<Message> {
        Element {
            id: Some(id),
            children: vec![children.into()],
            content: widget(self),
            ..Default::default()
        }
    }
}

impl<Message: 'static + Send> Default for Button<Message> {
    fn default() -> Self {
        Self::new()
    }
}

struct ButtonWidgetState {
    // Button state
    state: ButtonState,
    is_mouse_down: bool,
    is_mouse_over: bool,
}

impl ButtonWidgetState {
    pub fn new() -> Self {
        Self {
            state: ButtonState::Normal,
            is_mouse_down: false,
            is_mouse_over: false,
        }
    }

    pub fn into_any(self) -> Box<dyn Any> {
        Box::new(self)
    }

    fn update_state(&mut self, enabled: bool) {
        self.state = if !enabled {
            ButtonState::Disabled
        } else if self.is_mouse_down && self.is_mouse_over {
            ButtonState::Pressed
        } else if self.is_mouse_over {
            ButtonState::Hover
        } else {
            ButtonState::Normal
        };
    }

    fn get_current_style<'a>(&self, styles: &'a ButtonStyleSet) -> &'a ButtonStyle {
        match self.state {
            ButtonState::Normal => &styles.normal,
            ButtonState::Hover => &styles.hover,
            ButtonState::Pressed => &styles.pressed,
            ButtonState::Disabled => &styles.disabled,
        }
    }

    fn draw_button_background(
        &self,
        recorder: &mut crate::gfx::command_recorder::CommandRecorder,
        bounds: RectDIP,
        style: &ButtonStyle,
    ) {
        // Draw button background with border radius support
        if let Some(border_radius) = &style.border_radius {
            recorder.fill_rounded_rectangle(&bounds, border_radius, style.bg_color);
        } else {
            recorder.fill_rectangle(&bounds, style.bg_color);
        }

        // Draw button border
        if let Some(border) = &style.border {
            if let Some(border_radius) = &style.border_radius {
                recorder.draw_rounded_rectangle_stroked(
                    &bounds,
                    border_radius,
                    border.color,
                    border.width,
                );
            } else {
                recorder.draw_rectangle(&bounds, border.color, border.width);
            }
        }
    }
}

impl<Message> Widget<Message> for Button<Message> {
    fn state(
        &self,
        _arenas: &UIArenas,
        _device_resources: &crate::runtime::DeviceResources,
    ) -> super::State {
        Some(ButtonWidgetState::new().into_any())
    }

    fn limits_x(
        &self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
    ) -> super::limit_response::SizingForX {
        super::limit_response::SizingForX {
            min_width: 0.0,
            preferred_width: 0.0,
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
            min_height: 0.0,
            preferred_height: 0.0,
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
        let state = with_state!(mut instance as ButtonWidgetState);

        match event {
            super::Event::MouseButtonDown { x, y, .. } => {
                let point = PointDIP { x: *x, y: *y };
                if point.within(bounds.border_box) && self.enabled {
                    state.is_mouse_down = true;
                    state.is_mouse_over = true;
                    state.update_state(self.enabled);
                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            super::Event::MouseButtonUp { x, y, .. } => {
                let point = PointDIP { x: *x, y: *y };
                let was_pressed = state.is_mouse_down && state.is_mouse_over;

                state.is_mouse_down = false;
                state.is_mouse_over = point.within(bounds.border_box);
                state.update_state(self.enabled);

                // Trigger click if mouse was released over the button
                if was_pressed && point.within(bounds.border_box) && self.enabled {
                    if let Some(handler) = self.on_click.as_ref() {
                        handler(arenas, shell);
                    }
                }

                shell.request_redraw(hwnd, RedrawRequest::Immediate);
            }
            super::Event::MouseMove { x, y } => {
                let point = PointDIP { x: *x, y: *y };
                let was_over = state.is_mouse_over;
                state.is_mouse_over = point.within(bounds.border_box);

                if was_over != state.is_mouse_over {
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
        recorder: &mut crate::gfx::command_recorder::CommandRecorder,
        bounds: Bounds,
        _now: Instant,
    ) {
        let state = with_state!(mut instance as ButtonWidgetState);

        // Update visual state
        state.update_state(self.enabled);

        // Get current style based on button state
        let current_style = state.get_current_style(&self.styles);

        // Draw button background and border
        state.draw_button_background(recorder, bounds.border_box, current_style);
    }

    fn cursor(
        &self,
        _arenas: &UIArenas,
        _instance: &Instance,
        point: PointDIP,
        bounds: Bounds,
    ) -> Option<super::Cursor> {
        if point.within(bounds.border_box) && self.enabled {
            Some(super::Cursor::Arrow)
        } else {
            None
        }
    }
}
