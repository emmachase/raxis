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
    pub fn min(&self) -> f32 {
        match self {
            Sizing::Fixed { px } => *px,
            Sizing::Grow { min, .. } => *min,
            Sizing::Fit { min, .. } => *min,
            Sizing::Percent { .. } => 0.0,
        }
    }

    pub fn max(&self) -> f32 {
        match self {
            Sizing::Fixed { px } => *px,
            Sizing::Grow { max, .. } => *max,
            Sizing::Fit { max, .. } => *max,
            Sizing::Percent { .. } => f32::INFINITY,
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

// ---------- Drop Shadow ----------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DropShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub spread_radius: f32,
    pub blur_radius: f32,
    pub color: Color, // RGBA packed
}

impl DropShadow {
    pub fn new(
        offset_x: f32,
        offset_y: f32,
        spread_radius: f32,
        blur_radius: f32,
        color: u32,
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
pub enum ElementContent {
    Text {
        /// Array of text spans that make up the text content
        // pub spans: Vec<TextSpan>,
        /// Processed text after wrapping, with spans and line heights together
        // pub wrapped_lines: Vec<WrappedLine>,

        /// Device layout
        layout: Option<IDWriteTextLayout>,
    },
    Widget(Box<dyn Widget>),
}

impl ElementContent {
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

    pub fn unwrap_widget(&mut self) -> &mut Box<dyn Widget> {
        if let ElementContent::Widget(widget) = self {
            widget
        } else {
            panic!("ElementContent is not a Widget");
        }
    }
}

pub type UIKey = slotmap::DefaultKey;

#[derive(Debug)]
pub struct UIElement {
    pub parent: Option<UIKey>,
    pub children: Vec<UIKey>,

    pub content: Option<ElementContent>,

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

    pub background_color: Option<u32>,
    pub color: Option<u32>,
    pub word_break: Option<WordBreak>,
    pub padding: BoxAmount,
    pub drop_shadow: Option<DropShadow>,

    pub id: Option<u64>,
    pub id_map: HashMap<u64, UIKey>,
}

impl Default for UIElement {
    fn default() -> Self {
        Self {
            parent: None,
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
            drop_shadow: None,
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

#[derive(Debug, Default)]
pub struct Element {
    pub children: Vec<Element>,

    pub content: Option<ElementContent>,

    pub direction: Direction,

    // These names mirror the TS definitions, though they may be confusing.
    pub horizontal_alignment: HorizontalAlignment,
    pub vertical_alignment: VerticalAlignment,

    pub width: Sizing,
    pub height: Sizing,

    pub child_gap: f32,

    pub floating: Option<FloatingConfig>,
    pub scroll: Option<ScrollConfig>,

    pub background_color: Option<u32>,
    pub color: Option<u32>,
    pub word_break: Option<WordBreak>,
    pub padding: BoxAmount,
    pub drop_shadow: Option<DropShadow>,

    pub id: Option<u64>,
}

fn to_shell(element: Element) -> (UIElement, Vec<Element>) {
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
            drop_shadow: element.drop_shadow,
            id: element.id,
            ..Default::default()
        },
        children,
    )
}

pub fn create_tree(device_resources: &DeviceResources, tree: &mut OwnedUITree, root: Element) {
    let mut queue = vec![(root, None)];
    let mut root_key = None;

    tree.slots.clear();

    while let Some((element, parent)) = queue.pop() {
        let (mut shell, children) = to_shell(element);
        shell.parent = parent;

        // Initialize widget state if new
        if let Some(ElementContent::Widget(ref widget)) = shell.content {
            if let Some(id) = shell.id {
                tree.widget_state.entry(id).or_insert(Instance::new(
                    id,
                    &**widget,
                    device_resources,
                ));
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
