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
            axis_align_content: Alignment::Center,
            ..Default::default()
        }],
        axis_align_content: Alignment::Center,
        ..Default::default()
    }
}

pub trait ElementAlignmentExt {
    /// Set how children are distributed along the main axis (justify-content in CSS)
    fn axis_align_content(self, alignment: Alignment) -> Self;
    /// Set default main-axis alignment for all children (justify-items, ZStack only)
    fn axis_align_items(self, alignment: Alignment) -> Self;
    /// Set default cross-axis alignment for all children (align-items in CSS)
    fn cross_align_items(self, alignment: Alignment) -> Self;
    /// Set how wrapped rows/columns are distributed along cross axis (align-content in CSS)
    fn cross_align_content(self, alignment: Alignment) -> Self;
    /// Set how this element aligns on main axis in parent (justify-self, ZStack only)
    fn axis_align_self(self, alignment: Alignment) -> Self;
    /// Set how this element aligns on cross axis in parent (align-self in CSS)
    fn cross_align_self(self, alignment: Alignment) -> Self;

    // Legacy methods
    #[deprecated(since = "0.1.0", note = "Use axis_align_content or cross_align_self")]
    fn axis_align(self, alignment: Alignment) -> Self;
    #[deprecated(since = "0.1.0", note = "Use cross_align_items or cross_align_self")]
    fn cross_align(self, alignment: Alignment) -> Self;
    #[deprecated(since = "0.1.0", note = "Use axis_align_content or cross_align_self")]
    fn align_x(self, alignment: Alignment) -> Self;
    #[deprecated(since = "0.1.0", note = "Use axis_align_content or cross_align_self")]
    fn align_y(self, alignment: Alignment) -> Self;
}

impl<Message> ElementAlignmentExt for Element<Message> {
    fn axis_align_content(mut self, alignment: Alignment) -> Self {
        self.children = self.children.axis_align_content(alignment);
        self
    }

    fn axis_align_items(mut self, alignment: Alignment) -> Self {
        self.children = self.children.axis_align_items(alignment);
        self
    }

    fn cross_align_items(mut self, alignment: Alignment) -> Self {
        self.children = self.children.cross_align_items(alignment);
        self
    }

    fn cross_align_content(mut self, alignment: Alignment) -> Self {
        self.children = self.children.cross_align_content(alignment);
        self
    }

    fn axis_align_self(mut self, alignment: Alignment) -> Self {
        self.children = self.children.axis_align_self(alignment);
        self
    }

    fn cross_align_self(mut self, alignment: Alignment) -> Self {
        self.children = self.children.cross_align_self(alignment);
        self
    }

    fn axis_align(self, alignment: Alignment) -> Self {
        self.axis_align_content(alignment)
    }

    fn cross_align(self, alignment: Alignment) -> Self {
        self.cross_align_self(alignment)
    }

    fn align_x(self, alignment: Alignment) -> Self {
        self.axis_align_content(alignment)
    }

    fn align_y(self, alignment: Alignment) -> Self {
        self.cross_align_self(alignment)
    }
}

impl<Message> ElementAlignmentExt for Vec<Element<Message>> {
    fn axis_align_content(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.axis_align_content = alignment;
        });
        self
    }

    fn axis_align_items(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.axis_align_items = alignment;
        });
        self
    }

    fn cross_align_items(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.cross_align_items = alignment;
        });
        self
    }

    fn cross_align_content(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.cross_align_content = alignment;
        });
        self
    }

    fn axis_align_self(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.axis_align_self = Some(alignment);
        });
        self
    }

    fn cross_align_self(mut self, alignment: Alignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.cross_align_self = Some(alignment);
        });
        self
    }

    fn axis_align(self, alignment: Alignment) -> Self {
        self.axis_align_content(alignment)
    }

    fn cross_align(self, alignment: Alignment) -> Self {
        self.cross_align_self(alignment)
    }

    fn align_x(self, alignment: Alignment) -> Self {
        self.axis_align_content(alignment)
    }

    fn align_y(self, alignment: Alignment) -> Self {
        self.cross_align_self(alignment)
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
