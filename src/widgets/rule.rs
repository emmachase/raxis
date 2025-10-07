use windows::Win32::Foundation::HWND;

use crate::Shell;
use crate::gfx::command_recorder::CommandRecorder;
use crate::layout::UIArenas;
use crate::layout::model::{Color, Element, ElementStyle, Sizing, StrokeDashStyle, StrokeLineCap};
use crate::widgets::{Bounds, Event, Instance, Widget};

/// Orientation of the rule/divider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuleOrientation {
    #[default]
    Horizontal,
    Vertical,
}

/// A Rule (divider line) widget for creating horizontal or vertical separators
#[derive(Debug, Clone)]
pub struct Rule {
    pub orientation: RuleOrientation,
    pub color: Color,
    pub thickness: f32,
    pub dash_style: Option<StrokeDashStyle>,
    pub stroke_cap: Option<StrokeLineCap>,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            orientation: RuleOrientation::Horizontal,
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.8,
            },
            thickness: 1.0,
            dash_style: None,
            stroke_cap: None,
        }
    }
}

impl Rule {
    /// Create a new horizontal rule
    pub fn horizontal() -> Self {
        Self {
            orientation: RuleOrientation::Horizontal,
            ..Default::default()
        }
    }

    /// Create a new vertical rule
    pub fn vertical() -> Self {
        Self {
            orientation: RuleOrientation::Vertical,
            ..Default::default()
        }
    }

    /// Set the color of the rule
    pub fn with_color(self, color: impl Into<Color>) -> Self {
        Self {
            color: color.into(),
            ..self
        }
    }

    /// Set the thickness of the rule
    pub fn with_thickness(self, thickness: f32) -> Self {
        Self { thickness, ..self }
    }

    /// Set the rule to be dashed
    pub fn dashed(self) -> Self {
        Self {
            dash_style: Some(StrokeDashStyle::Dash),
            ..self
        }
    }

    /// Set the rule to be dotted
    pub fn dotted(self) -> Self {
        Self {
            dash_style: Some(StrokeDashStyle::Dot),
            ..self
        }
    }

    /// Set the rule to be dash-dot pattern
    pub fn dash_dot(self) -> Self {
        Self {
            dash_style: Some(StrokeDashStyle::DashDot),
            ..self
        }
    }

    /// Set a custom dash pattern
    pub fn with_custom_dashes(self, dashes: &'static [f32], offset: f32) -> Self {
        Self {
            dash_style: Some(StrokeDashStyle::Custom { dashes, offset }),
            ..self
        }
    }

    /// Set the stroke cap style
    pub fn with_stroke_cap(self, cap: StrokeLineCap) -> Self {
        Self {
            stroke_cap: Some(cap),
            ..self
        }
    }

    /// Set the rule to use round caps
    pub fn round_caps(self) -> Self {
        Self {
            stroke_cap: Some(StrokeLineCap::Round),
            ..self
        }
    }

    pub fn as_element<Message>(self, id: u64) -> Element<Message> {
        let orientation = self.orientation;
        let element = Element::default().with_id(id).with_widget(self);

        if orientation == RuleOrientation::Horizontal {
            element.with_width(Sizing::grow())
        } else {
            element.with_height(Sizing::grow())
        }
    }
}

/// Widget state for Rule - minimal since rules don't have interactive state
#[derive(Debug)]
pub struct RuleWidgetState {
    _placeholder: (), // Rules don't need state, but we need a struct
}

const MIN_SIZE: f32 = 1.0;

impl<Message> Widget<Message> for Rule {
    fn limits_x(
        &self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
    ) -> crate::widgets::limit_response::SizingForX {
        match self.orientation {
            RuleOrientation::Horizontal => {
                // Horizontal rules want to grow to fill available width
                crate::widgets::limit_response::SizingForX {
                    min_width: MIN_SIZE,
                    preferred_width: MIN_SIZE,
                }
            }
            RuleOrientation::Vertical => {
                // Vertical rules have fixed width based on thickness
                crate::widgets::limit_response::SizingForX {
                    min_width: self.thickness,
                    preferred_width: self.thickness,
                }
            }
        }
    }

    fn limits_y(
        &self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
        _border_width: f32,
        _content_width: f32,
    ) -> crate::widgets::limit_response::SizingForY {
        match self.orientation {
            RuleOrientation::Horizontal => {
                // Horizontal rules have fixed height based on thickness
                crate::widgets::limit_response::SizingForY {
                    min_height: self.thickness,
                    preferred_height: self.thickness,
                }
            }
            RuleOrientation::Vertical => {
                // Vertical rules want to grow to fill available height
                crate::widgets::limit_response::SizingForY {
                    min_height: MIN_SIZE,
                    preferred_height: MIN_SIZE,
                }
            }
        }
    }

    fn paint(
        &mut self,
        _arenas: &UIArenas,
        _instance: &mut Instance,
        _shell: &mut Shell<Message>,
        recorder: &mut CommandRecorder,
        _style: ElementStyle,
        bounds: Bounds,
        _now: std::time::Instant,
    ) {
        let rect = &bounds.content_box;

        match self.orientation {
            RuleOrientation::Horizontal => {
                // Draw a horizontal line across the center of the available area
                let y = (rect.y + rect.height / 2.0 + 0.5).round() - 0.5;
                recorder.draw_line(
                    rect.x,
                    y,
                    rect.x + rect.width,
                    y,
                    self.color,
                    self.thickness,
                    self.dash_style,
                    self.stroke_cap,
                );
            }
            RuleOrientation::Vertical => {
                // Draw a vertical line down the center of the available area
                let x = (rect.x + rect.width / 2.0 + 0.5).round() - 0.5;
                recorder.draw_line(
                    x,
                    rect.y,
                    x,
                    rect.y + rect.height,
                    self.color,
                    self.thickness,
                    self.dash_style,
                    self.stroke_cap,
                );
            }
        }
    }

    fn update(
        &mut self,
        _arenas: &mut UIArenas,
        _instance: &mut Instance,
        _hwnd: HWND,
        _shell: &mut Shell<Message>,
        _event: &Event,
        _bounds: Bounds,
    ) {
    }
}

/// Helper function to create a horizontal rule element
pub fn horizontal_rule<Message>(id: u64) -> Element<Message> {
    Element::default()
        .with_id(id)
        .with_width(Sizing::grow())
        .with_widget(Rule::horizontal())
}

/// Helper function to create a vertical rule element
pub fn vertical_rule<Message>(id: u64) -> Element<Message> {
    Element::default()
        .with_id(id)
        .with_height(Sizing::grow())
        .with_widget(Rule::vertical())
}

/// Helper function to create a horizontal rule with custom styling
pub fn styled_horizontal_rule<Message>(
    id: u64,
    color: impl Into<Color>,
    thickness: f32,
    dash_style: Option<StrokeDashStyle>,
) -> Element<Message> {
    let mut rule = Rule::horizontal()
        .with_color(color)
        .with_thickness(thickness);

    if let Some(style) = dash_style {
        rule.dash_style = Some(style);
    }

    Element::default()
        .with_id(id)
        .with_width(Sizing::grow())
        .with_widget(rule)
}

/// Helper function to create a vertical rule with custom styling  
pub fn styled_vertical_rule<Message>(
    id: u64,
    color: impl Into<Color>,
    thickness: f32,
    dash_style: Option<StrokeDashStyle>,
) -> Element<Message> {
    let mut rule = Rule::vertical().with_color(color).with_thickness(thickness);

    if let Some(style) = dash_style {
        rule.dash_style = Some(style);
    }

    Element::default()
        .with_id(id)
        .with_height(Sizing::grow())
        .with_widget(rule)
}
