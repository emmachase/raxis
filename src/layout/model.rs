//! This module defines geometry, sizing, alignment, scrolling, and element types
//! for a simple UI layout system.

use std::collections::HashMap;

use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;

use crate::{
    layout::OwnedUITree,
    runtime::DeviceResources,
    widgets::{Color, Instance, Widget},
};

// ---------- Geometry & basic types ----------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoxAmount {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl BoxAmount {
    pub fn all(amount: f32) -> Self {
        Self {
            top: amount,
            right: amount,
            bottom: amount,
            left: amount,
        }
    }

    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    LeftToRight,
    TopToBottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HorizontalAlignment {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VerticalAlignment {
    #[default]
    Top,
    Center,
    Bottom,
}

/// How to break text at word boundaries
/// Default: AfterWord
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WordBreak {
    None,
    // Anywhere, // TODO: Implement
    #[default]
    AfterWord,
}

// ---------- Inline text model ----------

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TextSpan {
    /// The text content of this span
    pub text: String,
    /// Optional color override for this specific span (RGBA packed)
    pub color: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WrappedLine {
    /// The spans that make up this line
    pub spans: Vec<TextSpan>,
    /// The height of this line (maximum of all span heights)
    pub height: f32,
}

impl Default for WrappedLine {
    fn default() -> Self {
        Self {
            spans: Vec::new(),
            height: 0.0,
        }
    }
}

// ---------- Sizing ----------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    /// Fixed pixel size. Equivalent to min=max=px in TS "Fixed".
    Fixed { px: f32 },
    /// Grow between [min, max].
    Grow { min: f32, max: f32 },
    /// Fit content between [min, max].
    Fit { min: f32, max: f32 },
    /// Percentage of parent size (0..=1 or 0..=100 based on convention).
    Percent { percent: f32 },
}

impl Sizing {
    pub fn get_min(&self) -> f32 {
        match self {
            Sizing::Fixed { px } => *px,
            Sizing::Grow { min, .. } => *min,
            Sizing::Fit { min, .. } => *min,
            Sizing::Percent { .. } => 0.0,
        }
    }

    pub fn get_max(&self) -> f32 {
        match self {
            Sizing::Fixed { px } => *px,
            Sizing::Grow { max, .. } => *max,
            Sizing::Fit { max, .. } => *max,
            Sizing::Percent { .. } => f32::INFINITY,
        }
    }

    pub fn min(self, min: f32) -> Self {
        match self {
            Sizing::Fixed { px } => Sizing::Fixed { px },
            Sizing::Grow { max, .. } => Sizing::Grow { min, max },
            Sizing::Fit { max, .. } => Sizing::Fit { min, max },
            Sizing::Percent { .. } => Sizing::Percent { percent: 1.0 },
        }
    }

    pub fn max(self, max: f32) -> Self {
        match self {
            Sizing::Fixed { px } => Sizing::Fixed { px },
            Sizing::Grow { min, .. } => Sizing::Grow { min, max },
            Sizing::Fit { min, .. } => Sizing::Fit { min, max },
            Sizing::Percent { .. } => Sizing::Percent { percent: 1.0 },
        }
    }

    pub fn fit() -> Self {
        Sizing::Fit {
            min: 0.0,
            max: f32::INFINITY,
        }
    }

    pub fn grow() -> Self {
        Sizing::Grow {
            min: 0.0,
            max: f32::INFINITY,
        }
    }

    pub fn fixed(px: f32) -> Self {
        Sizing::Fixed { px }
    }

    pub fn percent(percent: f32) -> Self {
        Sizing::Percent { percent }
    }
}

impl Default for Sizing {
    fn default() -> Self {
        Sizing::fit()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

// ---------- Floating / Anchoring ----------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Offset2D {
    pub x: Option<f32>,
    pub y: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Alignment2D<A, B> {
    pub x: Option<A>,
    pub y: Option<B>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FloatingConfig {
    pub offset: Option<Offset2D>,

    /// Defaults to parent when None
    pub anchor_id: Option<u64>,
    pub anchor: Option<Alignment2D<HorizontalAlignment, VerticalAlignment>>,
    pub align: Option<Alignment2D<HorizontalAlignment, VerticalAlignment>>,
}

// ---------- Border Radius ----------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl BorderRadius {
    pub fn all(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    pub fn top(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }
    }

    pub fn bottom(radius: f32) -> Self {
        Self {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    pub fn left(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: radius,
        }
    }

    pub fn right(radius: f32) -> Self {
        Self {
            top_left: 0.0,
            top_right: radius,
            bottom_right: radius,
            bottom_left: 0.0,
        }
    }

    pub fn tl_br(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: 0.0,
            bottom_right: radius,
            bottom_left: 0.0,
        }
    }

    pub fn tr_bl(radius: f32) -> Self {
        Self {
            top_left: 0.0,
            top_right: radius,
            bottom_right: 0.0,
            bottom_left: radius,
        }
    }
}

// ---------- Drop Shadow ----------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DropShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub spread_radius: f32,
    pub blur_radius: f32,
    pub color: Color,
}

impl DropShadow {
    pub fn new(
        offset_x: f32,
        offset_y: f32,
        spread_radius: f32,
        blur_radius: f32,
        color: u32, // RGBA packed
    ) -> Self {
        Self {
            offset_x,
            offset_y,
            spread_radius,
            blur_radius,
            color: Color::from(color),
        }
    }

    pub fn simple(offset_x: f32, offset_y: f32) -> Self {
        Self {
            offset_x,
            offset_y,
            spread_radius: 0.0,
            blur_radius: 0.0,
            color: Color::default(),
        }
    }

    pub fn offset(self, offset_x: f32, offset_y: f32) -> Self {
        Self {
            offset_x,
            offset_y,
            ..self
        }
    }

    pub fn spread_radius(self, spread_radius: f32) -> Self {
        Self {
            spread_radius,
            ..self
        }
    }

    pub fn blur_radius(self, blur_radius: f32) -> Self {
        Self {
            blur_radius,
            ..self
        }
    }

    pub fn color(self, color: u32) -> Self {
        Self {
            color: Color::from(color),
            ..self
        }
    }
}

// ---------- Border ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BorderPlacement {
    #[default]
    Inset,
    Center,
    Outset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BorderDashCap {
    Round,
    #[default]
    Square,
    Triangle,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BorderDashStyle {
    Solid,
    Dash,
    Dot,
    DashDot,
    DashDotDot,
    Custom { dashes: Vec<f32>, offset: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Color,
    pub placement: BorderPlacement,
    pub dash_style: Option<BorderDashStyle>,
    pub dash_cap: BorderDashCap,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            width: 1.0,
            color: Color::default(),
            placement: BorderPlacement::Inset,
            dash_style: None,
            dash_cap: BorderDashCap::default(),
        }
    }
}

// ---------- Scrolling ----------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScrollConfig {
    pub horizontal: Option<bool>,
    pub vertical: Option<bool>,
    pub horizontal_scroll_amount: Option<f32>,
    pub vertical_scroll_amount: Option<f32>,
    pub max_horizontal_scroll: Option<f32>,
    pub max_vertical_scroll: Option<f32>,

    // Sticky scrolling behavior
    pub sticky_bottom: Option<bool>, // keep scrolled to bottom when content height increases
    pub sticky_right: Option<bool>,  // keep scrolled to right when content width increases

    // Scrollbar appearance customization
    pub scrollbar_size: Option<f32>, // width of the scrollbar track
    pub scrollbar_track_color: Option<u32>,
    pub scrollbar_thumb_color: Option<u32>,
    pub scrollbar_min_thumb_size: Option<f32>, // minimum size of the thumb in pixels
}

// ---------- Element tree ----------

#[derive(Debug)]
pub enum ElementContent<Message> {
    Text {
        /// Array of text spans that make up the text content
        // pub spans: Vec<TextSpan>,
        /// Processed text after wrapping, with spans and line heights together
        // pub wrapped_lines: Vec<WrappedLine>,

        /// Device layout
        layout: Option<IDWriteTextLayout>,
    },
    Widget(Box<dyn Widget<Message>>),
}

impl<Message> ElementContent<Message> {
    pub fn is_text(&self) -> bool {
        matches!(self, ElementContent::Text { .. })
    }

    pub fn is_widget(&self) -> bool {
        matches!(self, ElementContent::Widget { .. })
    }

    pub fn unwrap_text(&self) -> &Option<IDWriteTextLayout> {
        if let ElementContent::Text { layout } = self {
            layout
        } else {
            panic!("ElementContent is not a Text");
        }
    }

    pub fn unwrap_widget(&mut self) -> &mut Box<dyn Widget<Message>> {
        if let ElementContent::Widget(widget) = self {
            widget
        } else {
            panic!("ElementContent is not a Widget");
        }
    }
}

pub type UIKey = slotmap::DefaultKey;

#[derive(Debug)]
pub struct UIElement<Message> {
    pub children: Vec<UIKey>,

    pub content: Option<ElementContent<Message>>,

    pub direction: Direction,

    // These names mirror the TS definitions, though they may be confusing.
    pub horizontal_alignment: HorizontalAlignment,
    pub vertical_alignment: VerticalAlignment,

    pub width: Sizing,
    pub height: Sizing,

    pub child_gap: f32,

    pub __positioned: bool,
    pub computed_width: f32,
    pub computed_height: f32,
    pub computed_content_width: f32,
    pub computed_content_height: f32,
    pub min_width: f32,
    pub min_height: f32,
    pub x: f32,
    pub y: f32,

    pub floating: Option<FloatingConfig>,
    pub scroll: Option<ScrollConfig>,

    pub background_color: Option<Color>,
    pub color: Option<Color>,
    pub word_break: Option<WordBreak>,
    pub padding: BoxAmount,
    pub border_radius: Option<BorderRadius>,
    pub drop_shadow: Option<DropShadow>,
    pub border: Option<Border>,

    pub id: Option<u64>,
    pub id_map: HashMap<u64, UIKey>,
}

impl<Message> Default for UIElement<Message> {
    fn default() -> Self {
        Self {
            children: Vec::new(),
            content: None,
            direction: Direction::LeftToRight,
            horizontal_alignment: HorizontalAlignment::Left,
            vertical_alignment: VerticalAlignment::Top,
            width: Sizing::default(),
            height: Sizing::default(),
            child_gap: 0.0,
            __positioned: false,
            computed_width: 0.0,
            computed_height: 0.0,
            computed_content_width: 0.0,
            computed_content_height: 0.0,
            min_width: 0.0,
            min_height: 0.0,
            x: 0.0,
            y: 0.0,
            floating: None,
            scroll: None,
            background_color: None,
            color: None,
            word_break: None,
            padding: BoxAmount::default(),
            border_radius: None,
            drop_shadow: None,
            border: None,
            id: None,
            id_map: HashMap::new(),
        }
    }
}

// impl UIElement {
//     pub fn is_text_element(&self) -> bool {
//         self.content.is_some()
//     }
// }

// pub struct DeclTree {
//     pub root: UIElement,
//     pub children: Vec<DeclTree>,
// }

// impl DeclTree {
//     pub fn hookup(self, slots: BorrowedUITree) -> DefaultKey {
//         let root = slots.insert(self.root);
//         for child in self.children {
//             // let k = slots.insert(child);
//             let k = DeclTree::hookup(child, slots);
//             slots[k].parent = Some(root);
//             slots[root].children.push(k);
//         }
//         root
//     }
// }

// pub fn element<F: FnOnce() -> Vec<UIElement>>(
//     tree: RefCell<OwnedUITree>,
//     element: UIElement,
//     children: F,
// ) -> UIElement {
//     let children = children();
//     // let root = tree.borrow_mut().insert(element);
//     for child in children {
//         let k = tree.borrow_mut().insert(child);
//         tree.borrow_mut()[root].children.push(k);
//     }
//     element
// }

#[derive(Debug)]
pub struct Element<Message> {
    pub children: Vec<Element<Message>>,

    pub content: Option<ElementContent<Message>>,

    pub direction: Direction,

    // These names mirror the TS definitions, though they may be confusing.
    pub horizontal_alignment: HorizontalAlignment,
    pub vertical_alignment: VerticalAlignment,

    pub width: Sizing,
    pub height: Sizing,

    pub child_gap: f32,

    pub floating: Option<FloatingConfig>,
    pub scroll: Option<ScrollConfig>,

    pub background_color: Option<Color>,
    pub color: Option<Color>,
    pub word_break: Option<WordBreak>,
    pub padding: BoxAmount,
    pub border_radius: Option<BorderRadius>,
    pub drop_shadow: Option<DropShadow>,
    pub border: Option<Border>,

    pub id: Option<u64>,
}

impl<Message> Element<Message> {
    pub fn with_id(self, id: u64) -> Self {
        Self {
            id: Some(id),
            ..self
        }
    }

    pub fn with_horizontal_alignment(self, align: HorizontalAlignment) -> Self {
        Self {
            horizontal_alignment: align,
            ..self
        }
    }

    pub fn with_vertical_alignment(self, align: VerticalAlignment) -> Self {
        Self {
            vertical_alignment: align,
            ..self
        }
    }

    pub fn with_width(self, width: Sizing) -> Self {
        Self { width, ..self }
    }

    pub fn with_height(self, height: Sizing) -> Self {
        Self { height, ..self }
    }

    pub fn with_child_gap(self, gap: f32) -> Self {
        Self {
            child_gap: gap,
            ..self
        }
    }

    pub fn with_floating(self, floating: impl Into<FloatingConfig>) -> Self {
        Self {
            floating: Some(floating.into()),
            ..self
        }
    }

    pub fn with_scroll(self, scroll: impl Into<ScrollConfig>) -> Self {
        Self {
            scroll: Some(scroll.into()),
            ..self
        }
    }

    pub fn with_background_color(self, color: impl Into<Color>) -> Self {
        Self {
            background_color: Some(color.into()),
            ..self
        }
    }

    pub fn with_color(self, color: impl Into<Color>) -> Self {
        Self {
            color: Some(color.into()),
            ..self
        }
    }

    pub fn with_padding(self, padding: impl Into<BoxAmount>) -> Self {
        Self {
            padding: padding.into(),
            ..self
        }
    }

    pub fn with_word_break(self, word_break: WordBreak) -> Self {
        Self {
            word_break: Some(word_break),
            ..self
        }
    }

    pub fn with_border_radius(self, border_radius: impl Into<BorderRadius>) -> Self {
        Self {
            border_radius: Some(border_radius.into()),
            ..self
        }
    }

    pub fn with_drop_shadow(self, drop_shadow: impl Into<DropShadow>) -> Self {
        Self {
            drop_shadow: Some(drop_shadow.into()),
            ..self
        }
    }

    pub fn with_border(self, border: impl Into<Border>) -> Self {
        Self {
            border: Some(border.into()),
            ..self
        }
    }
}

impl<Message> Default for Element<Message> {
    fn default() -> Self {
        Self {
            children: Vec::new(),
            content: None,
            direction: Direction::LeftToRight,
            horizontal_alignment: HorizontalAlignment::Left,
            vertical_alignment: VerticalAlignment::Top,
            width: Sizing::default(),
            height: Sizing::default(),
            child_gap: 0.0,
            floating: None,
            scroll: None,
            background_color: None,
            color: None,
            word_break: None,
            padding: BoxAmount::default(),
            border_radius: None,
            drop_shadow: None,
            border: None,
            id: None,
        }
    }
}

fn to_shell<Message>(element: Element<Message>) -> (UIElement<Message>, Vec<Element<Message>>) {
    let children = element.children;
    (
        UIElement {
            children: Vec::new(),
            content: element.content,
            direction: element.direction,
            horizontal_alignment: element.horizontal_alignment,
            vertical_alignment: element.vertical_alignment,
            width: element.width,
            height: element.height,
            child_gap: element.child_gap,
            floating: element.floating,
            scroll: element.scroll,
            background_color: element.background_color,
            color: element.color,
            word_break: element.word_break,
            padding: element.padding,
            border_radius: element.border_radius,
            drop_shadow: element.drop_shadow,
            border: element.border,
            id: element.id,
            ..Default::default()
        },
        children,
    )
}

pub fn create_tree<Message>(
    device_resources: &DeviceResources,
    tree: &mut OwnedUITree<Message>,
    root: Element<Message>,
) {
    let mut queue = vec![(root, None)];
    let mut root_key = None;

    tree.slots.clear();

    while let Some((element, parent)) = queue.pop() {
        let (shell, children) = to_shell(element);

        // Initialize widget state if new
        if let Some(ElementContent::Widget(ref widget)) = shell.content {
            if let Some(id) = shell.id {
                tree.widget_state.entry(id).or_insert_with(|| {
                    Instance::new(id, &**widget, &tree.arenas, device_resources)
                });
            }
        }

        let key = tree.slots.insert(shell);
        if let Some(parent) = parent {
            tree.slots[parent].children.push(key);
        }
        if parent.is_none() {
            root_key = Some(key);
        }

        for child in children.into_iter().rev() {
            queue.push((child, Some(key)));
        }
    }

    tree.root = root_key.expect("no root found");
}
