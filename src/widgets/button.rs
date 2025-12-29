use std::any::Any;
use std::fmt::Debug;
use std::time::Instant;

use windows::Win32::Foundation::HWND;

use crate::gfx::PointDIP;
use crate::gfx::command_recorder::CommandRecorder;
use crate::layout::UIArenas;
use crate::layout::model::{
    Border, BorderRadius, Color, DropShadow, Element, ElementStyle, TextShadow,
};
use crate::widgets::{Bounds, Cursor, widget};
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
    pub bg_color: Option<Color>,
    pub text_color: Option<Color>,
    pub border: Option<Border>,
    pub border_radius: Option<BorderRadius>,
    pub drop_shadows: Vec<DropShadow>,
    pub text_shadows: Vec<TextShadow>,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            bg_color: Some(Color {
                r: 0.9,
                g: 0.9,
                b: 0.9,
                a: 1.0,
            }),
            text_color: Some(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            border: Some(Border {
                width: 1.0,
                color: Color::from(0x00000033),
                ..Default::default()
            }),
            border_radius: None,
            drop_shadows: Vec::new(),
            text_shadows: Vec::new(),
        }
    }
}

impl ButtonStyle {
    pub fn clear() -> Self {
        Self {
            bg_color: None,
            text_color: None,
            border: None,
            border_radius: None,
            drop_shadows: Vec::new(),
            text_shadows: Vec::new(),
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
                bg_color: Some(Color {
                    r: 0.85,
                    g: 0.85,
                    b: 0.85,
                    a: 1.0,
                }),
                text_color: Some(Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
                border: Some(Border {
                    width: 1.0,
                    color: Color::from(0x00000033),
                    ..Default::default()
                }),
                border_radius: None,
                drop_shadows: Vec::new(),
                text_shadows: Vec::new(),
            },
            pressed: ButtonStyle {
                bg_color: Some(Color {
                    r: 0.75,
                    g: 0.75,
                    b: 0.75,
                    a: 1.0,
                }),
                text_color: Some(Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
                border: Some(Border {
                    width: 1.0,
                    color: Color::from(0x00000033),
                    ..Default::default()
                }),
                border_radius: None,
                drop_shadows: Vec::new(),
                text_shadows: Vec::new(),
            },
            disabled: ButtonStyle {
                bg_color: Some(Color {
                    r: 0.95,
                    g: 0.95,
                    b: 0.95,
                    a: 1.0,
                }),
                text_color: Some(Color {
                    r: 0.6,
                    g: 0.6,
                    b: 0.6,
                    a: 1.0,
                }),
                border: None,
                border_radius: None,
                drop_shadows: Vec::new(),
                text_shadows: Vec::new(),
            },
        }
    }
}

impl ButtonStyleSet {
    pub fn clear() -> Self {
        Self {
            normal: ButtonStyle::clear(),
            hover: ButtonStyle::clear(),
            pressed: ButtonStyle::clear(),
            disabled: ButtonStyle::clear(),
        }
    }

    pub fn ghost() -> Self {
        Self {
            normal: ButtonStyle {
                text_color: Some(Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.9,
                }),
                ..ButtonStyle::clear()
            },
            hover: ButtonStyle {
                bg_color: Some(Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.1,
                }),
                text_color: Some(Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.9,
                }),
                border: None,
                border_radius: None,
                drop_shadows: Vec::new(),
                text_shadows: Vec::new(),
            },
            pressed: ButtonStyle {
                bg_color: Some(Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.2,
                }),
                text_color: Some(Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.9,
                }),
                border: None,
                border_radius: None,
                drop_shadows: Vec::new(),
                text_shadows: Vec::new(),
            },
            disabled: ButtonStyle::clear(),
        }
    }
}

pub type OnClickFn<Message> = dyn Fn(&mut UIArenas, &mut Shell<Message>);
pub type IsFocused = bool;
pub type AdjustStyleFn = dyn Fn(ButtonState, IsFocused, ElementStyle) -> ElementStyle;

/// Button widget with text label and click handling
pub struct Button<Message> {
    pub enabled: bool,
    pub on_click: Option<Box<OnClickFn<Message>>>,
    pub styles: ButtonStyleSet,
    pub cursor: Option<Cursor>,
    pub adjust_style_fn: Option<Box<AdjustStyleFn>>,
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
            cursor: Some(Cursor::Pointer),
            adjust_style_fn: None,
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

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn ghost(mut self) -> Self {
        self.styles = ButtonStyleSet::ghost();
        self
    }

    pub fn clear(mut self) -> Self {
        self.styles = ButtonStyleSet::clear();
        self.cursor = None;
        self
    }

    pub fn with_styles(mut self, styles: ButtonStyleSet) -> Self {
        self.styles = styles;
        self
    }

    pub fn with_bg_color(mut self, color: Color) -> Self {
        self.styles.normal.bg_color = Some(color);
        self.styles.hover.bg_color = Some(color.deviate(0.05));
        self.styles.pressed.bg_color = Some(color.deviate(0.1));
        self.styles.disabled.bg_color = Some(color.darken(0.4).desaturate(0.3));
        self
    }

    pub fn with_no_bg_color(mut self) -> Self {
        self.styles.normal.bg_color = None;
        self.styles.hover.bg_color = None;
        self.styles.pressed.bg_color = None;
        self.styles.disabled.bg_color = None;
        self
    }

    pub fn with_border_radius(mut self, radius: impl Into<BorderRadius>) -> Self {
        let border_radius = radius.into();
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
        self.styles.normal.border = Some(border);
        self.styles.hover.border = Some(border);
        self.styles.pressed.border = Some(border);
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

    pub fn with_color(mut self, color: Color) -> Self {
        self.styles.normal.text_color = Some(color);
        self.styles.hover.text_color = Some(color);
        self.styles.pressed.text_color = Some(color);
        self.styles.disabled.text_color = Some(color);
        self
    }

    pub fn with_drop_shadow(mut self, shadow: DropShadow) -> Self {
        self.styles.normal.drop_shadows = vec![shadow];
        self.styles.hover.drop_shadows = vec![shadow];
        self.styles.pressed.drop_shadows = vec![shadow];
        self.styles.disabled.drop_shadows = vec![shadow];
        self
    }

    pub fn with_drop_shadows(mut self, shadows: Vec<DropShadow>) -> Self {
        self.styles.normal.drop_shadows = shadows.clone();
        self.styles.hover.drop_shadows = shadows.clone();
        self.styles.pressed.drop_shadows = shadows.clone();
        self.styles.disabled.drop_shadows = shadows;
        self
    }

    pub fn with_no_drop_shadow(mut self) -> Self {
        self.styles.normal.drop_shadows = Vec::new();
        self.styles.hover.drop_shadows = Vec::new();
        self.styles.pressed.drop_shadows = Vec::new();
        self.styles.disabled.drop_shadows = Vec::new();
        self
    }

    pub fn with_text_shadow(mut self, shadow: TextShadow) -> Self {
        self.styles.normal.text_shadows = vec![shadow];
        self.styles.hover.text_shadows = vec![shadow];
        self.styles.pressed.text_shadows = vec![shadow];
        self.styles.disabled.text_shadows = vec![shadow];
        self
    }

    pub fn with_text_shadows(mut self, shadows: Vec<TextShadow>) -> Self {
        self.styles.normal.text_shadows = shadows.clone();
        self.styles.hover.text_shadows = shadows.clone();
        self.styles.pressed.text_shadows = shadows.clone();
        self.styles.disabled.text_shadows = shadows;
        self
    }

    pub fn with_no_text_shadow(mut self) -> Self {
        self.styles.normal.text_shadows = Vec::new();
        self.styles.hover.text_shadows = Vec::new();
        self.styles.pressed.text_shadows = Vec::new();
        self.styles.disabled.text_shadows = Vec::new();
        self
    }

    pub fn with_adjust_style(
        mut self,
        adjust_fn: impl Fn(ButtonState, IsFocused, ElementStyle) -> ElementStyle + 'static,
    ) -> Self {
        self.adjust_style_fn = Some(Box::new(adjust_fn));
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

    fn adjust_style(
        &mut self,
        instance: &mut Instance,
        shell: &mut Shell<Message>,
        style: ElementStyle,
    ) -> ElementStyle {
        let state = with_state!(mut instance as ButtonWidgetState);
        state.update_state(self.enabled);
        let cur_style = state.get_current_style(&self.styles);
        let adjusted_style = ElementStyle {
            background_color: cur_style.bg_color.or(style.background_color),
            color: cur_style.text_color.or(style.color),
            border_radius: cur_style.border_radius.or(style.border_radius),
            drop_shadows: if !cur_style.drop_shadows.is_empty() {
                cur_style.drop_shadows.clone()
            } else {
                style.drop_shadows
            },
            text_shadows: if !cur_style.text_shadows.is_empty() {
                cur_style.text_shadows.clone()
            } else {
                style.text_shadows
            },
            border: cur_style.border.or(style.border),
            ..style
        };

        // Call custom adjust_style function if provided
        if let Some(adjust_fn) = &self.adjust_style_fn {
            adjust_fn(
                state.state,
                shell.focus_manager.is_focused(instance.id),
                adjusted_style,
            )
        } else {
            adjusted_style
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
                    shell.capture_event(instance.id);
                    shell.focus_manager.focus(instance.id);
                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            super::Event::MouseButtonUp { x, y, .. } => {
                let point = PointDIP { x: *x, y: *y };
                let was_pressed = state.is_mouse_down && state.is_mouse_over;

                // Trigger click if mouse was released over the button
                state.is_mouse_down = false;
                state.is_mouse_over = point.within(bounds.border_box);
                state.update_state(self.enabled);

                if was_pressed && point.within(bounds.border_box) && self.enabled {
                    if let Some(handler) = self.on_click.as_ref() {
                        handler(arenas, shell);
                    }
                }

                shell.request_redraw(hwnd, RedrawRequest::Immediate);
            }
            super::Event::MouseMove { x, y } | super::Event::MouseEnter { x, y } => {
                let point = PointDIP { x: *x, y: *y };
                let was_over = state.is_mouse_over;
                state.is_mouse_over = point.within(bounds.border_box);

                if was_over != state.is_mouse_over {
                    state.update_state(self.enabled);
                    shell.request_redraw(hwnd, RedrawRequest::Immediate);
                }
            }
            super::Event::MouseLeave { .. } => {
                let was_over = state.is_mouse_over;
                state.is_mouse_over = false;

                if was_over {
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
        _instance: &mut Instance,
        _shell: &mut Shell<Message>,
        _recorder: &mut CommandRecorder,
        _style: ElementStyle,
        _bounds: Bounds,
        _now: Instant,
    ) {
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
