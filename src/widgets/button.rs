use std::any::Any;
use std::time::Instant;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_REGULAR,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, IDWriteFactory,
    IDWriteTextFormat, IDWriteTextLayout,
};
use windows::core::Result;
use windows_core::{PCWSTR, w};
use windows_numerics::Vector2;

use crate::gfx::{PointDIP, RectDIP};
use crate::widgets::{Color, Instance, Renderer, Widget};
use crate::{RedrawRequest, Shell, with_state};

/// Button states for visual feedback
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonState {
    Normal,
    Hover,
    Pressed,
    Disabled,
}

/// Button widget with text label and click handling
#[derive(Debug, Clone)]
pub struct Button {
    pub text: String,
    pub enabled: bool,
    pub on_click: Option<fn()>,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            enabled: true,
            on_click: None,
        }
    }

    pub fn with_click_handler(mut self, handler: fn()) -> Self {
        self.on_click = Some(handler);
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

impl Default for Button {
    fn default() -> Self {
        Self::new("Button")
    }
}

struct ButtonWidgetState {
    // DirectWrite objects for text rendering
    dwrite_factory: IDWriteFactory,
    text_format: IDWriteTextFormat,
    text_layout: Option<IDWriteTextLayout>,

    // Button state
    state: ButtonState,
    is_mouse_down: bool,
    is_mouse_over: bool,

    // Layout
    bounds: RectDIP,
}

impl ButtonWidgetState {
    pub fn new(dwrite_factory: IDWriteFactory, text_format: IDWriteTextFormat) -> Self {
        let mut s = Self {
            dwrite_factory,
            text_format,
            text_layout: None,
            state: ButtonState::Normal,
            is_mouse_down: false,
            is_mouse_over: false,
            bounds: RectDIP::default(),
        };
        s.build_text_layout(&"", s.bounds)
            .expect("build text layout failed");
        s
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

    fn build_text_layout(&mut self, text: &str, bounds: RectDIP) -> Result<()> {
        if bounds != self.bounds {
            self.bounds = bounds;

            let wtext: Vec<u16> = text.encode_utf16().collect();

            unsafe {
                let layout = self.dwrite_factory.CreateTextLayout(
                    &wtext,
                    &self.text_format,
                    bounds.width_dip,
                    bounds.height_dip,
                )?;

                self.text_layout = Some(layout);
            }
        }
        Ok(())
    }

    fn get_button_colors(&self) -> (Color, Color, Color) {
        match self.state {
            ButtonState::Normal => (
                Color {
                    r: 0.9,
                    g: 0.9,
                    b: 0.9,
                    a: 1.0,
                }, // Background
                Color {
                    r: 0.7,
                    g: 0.7,
                    b: 0.7,
                    a: 1.0,
                }, // Border
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }, // Text
            ),
            ButtonState::Hover => (
                Color {
                    r: 0.85,
                    g: 0.85,
                    b: 0.85,
                    a: 1.0,
                },
                Color {
                    r: 0.6,
                    g: 0.6,
                    b: 0.6,
                    a: 1.0,
                },
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            ),
            ButtonState::Pressed => (
                Color {
                    r: 0.75,
                    g: 0.75,
                    b: 0.75,
                    a: 1.0,
                },
                Color {
                    r: 0.5,
                    g: 0.5,
                    b: 0.5,
                    a: 1.0,
                },
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            ),
            ButtonState::Disabled => (
                Color {
                    r: 0.95,
                    g: 0.95,
                    b: 0.95,
                    a: 1.0,
                },
                Color {
                    r: 0.8,
                    g: 0.8,
                    b: 0.8,
                    a: 1.0,
                },
                Color {
                    r: 0.6,
                    g: 0.6,
                    b: 0.6,
                    a: 1.0,
                },
            ),
        }
    }

    fn draw_button_background(&self, renderer: &Renderer, bounds: RectDIP) {
        let (bg_color, border_color, _) = self.get_button_colors();

        // Draw button background
        renderer.fill_rectangle(&bounds, bg_color);

        // Draw border (simple 1px border by drawing slightly smaller rect)
        let border_width = 1.0;
        renderer.draw_rectangle(&bounds, border_color.clone(), border_width);
    }

    fn draw_button_text(&self, renderer: &Renderer, bounds: RectDIP) -> Result<()> {
        if let Some(layout) = &self.text_layout {
            let (_, _, text_color) = self.get_button_colors();

            unsafe {
                renderer.brush.SetColor(&D2D1_COLOR_F {
                    r: text_color.r,
                    g: text_color.g,
                    b: text_color.b,
                    a: text_color.a,
                });

                renderer.render_target.DrawTextLayout(
                    Vector2 {
                        X: bounds.x_dip,
                        Y: bounds.y_dip,
                    },
                    layout,
                    renderer.brush,
                    windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                );
            }
        }
        Ok(())
    }
}

impl Widget for Button {
    fn state(&self, device_resources: &crate::runtime::DeviceResources) -> super::State {
        let text_format = unsafe {
            let text_format = device_resources
                .dwrite_factory
                .CreateTextFormat(
                    PCWSTR(w!("Segoe UI").as_ptr()),
                    None,
                    DWRITE_FONT_WEIGHT_REGULAR,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    14.0,
                    PCWSTR(w!("en-us").as_ptr()),
                )
                .unwrap();

            text_format
                .SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)
                .unwrap();
            text_format
                .SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
                .unwrap();
            text_format
        };

        Some(
            ButtonWidgetState::new(device_resources.dwrite_factory.clone(), text_format).into_any(),
        )
    }

    fn limits_x(&self, _instance: &Instance) -> super::limit_response::SizingForX {
        super::limit_response::SizingForX {
            min_width: 80.0,
            preferred_width: 80.0,
        }
    }

    fn limits_y(&self, _instance: &Instance, _width: f32) -> super::limit_response::SizingForY {
        super::limit_response::SizingForY {
            min_height: 32.0,
            preferred_height: 32.0,
        }
    }

    fn update(
        &mut self,
        instance: &mut Instance,
        hwnd: HWND,
        shell: &mut Shell,
        event: &super::Event,
        bounds: RectDIP,
    ) {
        let state = with_state!(mut instance as ButtonWidgetState);

        match event {
            super::Event::MouseButtonDown { x, y, .. } => {
                let point = PointDIP {
                    x_dip: *x,
                    y_dip: *y,
                };
                if point.within(bounds) && self.enabled {
                    state.is_mouse_down = true;
                    state.is_mouse_over = true;
                    state.update_state(self.enabled);
                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            super::Event::MouseButtonUp { x, y, .. } => {
                let point = PointDIP {
                    x_dip: *x,
                    y_dip: *y,
                };
                let was_pressed = state.is_mouse_down && state.is_mouse_over;

                state.is_mouse_down = false;
                state.is_mouse_over = point.within(bounds);
                state.update_state(self.enabled);

                // Trigger click if mouse was released over the button
                if was_pressed && point.within(bounds) && self.enabled {
                    if let Some(handler) = self.on_click {
                        handler();
                    }
                }

                shell.request_redraw(hwnd, RedrawRequest::Immediate);
            }
            super::Event::MouseMove { x, y } => {
                let point = PointDIP {
                    x_dip: *x,
                    y_dip: *y,
                };
                let was_over = state.is_mouse_over;
                state.is_mouse_over = point.within(bounds);

                if was_over != state.is_mouse_over {
                    state.update_state(self.enabled);
                }

                shell.request_redraw(hwnd, RedrawRequest::Immediate);
            }
            _ => {}
        }
    }

    fn paint(
        &mut self,
        instance: &mut Instance,
        _shell: &Shell,
        renderer: &Renderer,
        bounds: RectDIP,
        _now: Instant,
    ) {
        let state = with_state!(mut instance as ButtonWidgetState);

        // Build text layout if needed
        let _ = state.build_text_layout(&self.text, bounds);

        // Update visual state
        state.update_state(self.enabled);

        // Draw button background and border
        state.draw_button_background(renderer, bounds);

        // Draw button text
        let _ = state.draw_button_text(renderer, bounds);
    }

    fn cursor(
        &self,
        _instance: &Instance,
        point: PointDIP,
        bounds: RectDIP,
    ) -> Option<super::Cursor> {
        if point.within(bounds) && self.enabled {
            Some(super::Cursor::Arrow)
        } else {
            None
        }
    }
}
