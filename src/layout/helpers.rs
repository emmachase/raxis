use crate::layout::model::{Alignment, Direction, Element, Sizing};

pub fn row<Message>(children: Vec<Element<Message>>) -> Element<Message> {
    Element {
        children,
        direction: Direction::LeftToRight,
        ..Default::default()
    }
}

pub fn column<Message>(children: Vec<Element<Message>>) -> Element<Message> {
    Element {
        children,
        direction: Direction::TopToBottom,
        ..Default::default()
    }
}

pub fn container<Message>(content: impl Into<Element<Message>>) -> Element<Message> {
    Element {
        children: vec![content.into()],
        ..Default::default()
    }
}

pub fn spacer<Message>() -> Element<Message> {
    Element {
        width: Sizing::grow(),
        height: Sizing::grow(),
        ..Default::default()
    }
}

pub fn center<Message>(content: impl Into<Element<Message>>) -> Element<Message> {
    Element {
        direction: Direction::TopToBottom,
        width: Sizing::grow(),
        height: Sizing::grow(),
        children: vec![Element {
            direction: Direction::LeftToRight,
            width: Sizing::grow(),
            children: vec![content.into()],
            justify_content: Alignment::Center,
            ..Default::default()
        }],
        justify_content: Alignment::Center,
        ..Default::default()
    }
}

pub trait ElementAlignmentExt {
    /// Set how children are distributed along the main axis (justify-content in CSS)
    fn justify_content(self, alignment: Alignment) -> Self;
    /// Set default main-axis alignment for all children (justify-items, ZStack only)
    fn justify_items(self, alignment: Alignment) -> Self;
    /// Set default cross-axis alignment for all children (align-items in CSS)
    fn align_items(self, alignment: Alignment) -> Self;
    /// Set how wrapped rows/columns are distributed along cross axis (align-content in CSS)
    fn align_content(self, alignment: Alignment) -> Self;
    /// Set how this element aligns on main axis in parent (justify-self, ZStack only)
    fn justify_self(self, alignment: Alignment) -> Self;
    /// Set how this element aligns on cross axis in parent (align-self in CSS)
    fn align_self(self, alignment: Alignment) -> Self;

    // Legacy methods
    #[deprecated(since = "0.1.0", note = "Use justify_content or align_self")]
    fn axis_align(self, alignment: Alignment) -> Self;
    #[deprecated(since = "0.1.0", note = "Use align_items or align_self")]
    fn cross_align(self, alignment: Alignment) -> Self;
    #[deprecated(since = "0.1.0", note = "Use justify_content or align_self")]
    fn align_x(self, alignment: Alignment) -> Self;
    #[deprecated(since = "0.1.0", note = "Use justify_content or align_self")]
    fn align_y(self, alignment: Alignment) -> Self;
}

impl<Message> ElementAlignmentExt for Element<Message> {
    fn justify_content(mut self, alignment: Alignment) -> Self {
        self.children = self.children.justify_content(alignment);
        self
    }

    fn justify_items(mut self, alignment: Alignment) -> Self {
        self.children = self.children.justify_items(alignment);
        self
    }

    fn align_items(mut self, alignment: Alignment) -> Self {
        self.children = self.children.align_items(alignment);
        self
    }

    fn align_content(mut self, alignment: Alignment) -> Self {
        self.children = self.children.align_content(alignment);
        self
    }

    fn justify_self(mut self, alignment: Alignment) -> Self {
        self.children = self.children.justify_self(alignment);
        self
    }

    fn align_self(mut self, alignment: Alignment) -> Self {
        self.children = self.children.align_self(alignment);
        self
    }

    fn axis_align(self, alignment: Alignment) -> Self {
        self.justify_content(alignment)
    }

    fn cross_align(self, alignment: Alignment) -> Self {
        self.align_self(alignment)
    }

    fn align_x(self, alignment: Alignment) -> Self {
        self.justify_content(alignment)
    }

    fn align_y(self, alignment: Alignment) -> Self {
        self.align_self(alignment)
    }
}

impl<Message> ElementAlignmentExt for Vec<Element<Message>> {
    fn justify_content(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.justify_content = alignment;
        });
        self
    }

    fn justify_items(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.justify_items = alignment;
        });
        self
    }

    fn align_items(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.align_items = alignment;
        });
        self
    }

    fn align_content(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.align_content = alignment;
        });
        self
    }

    fn justify_self(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.justify_self = Some(alignment);
        });
        self
    }

    fn align_self(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.align_self = Some(alignment);
        });
        self
    }

    fn axis_align(self, alignment: Alignment) -> Self {
        self.justify_content(alignment)
    }

    fn cross_align(self, alignment: Alignment) -> Self {
        self.align_self(alignment)
    }

    fn align_x(self, alignment: Alignment) -> Self {
        self.justify_content(alignment)
    }

    fn align_y(self, alignment: Alignment) -> Self {
        self.align_self(alignment)
    }
}

#[macro_export]
macro_rules! row {
    ($($child: expr),* $(,)?) => {
        $crate::layout::helpers::row(vec![$($child.into()),*])
    };
}

#[macro_export]
macro_rules! column {
    ($($child: expr),* $(,)?) => {
        $crate::layout::helpers::column(vec![$($child.into()),*])
    };
}
