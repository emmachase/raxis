use crate::layout::model::{Direction, Element, HorizontalAlignment, Sizing, VerticalAlignment};

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
            horizontal_alignment: HorizontalAlignment::Center,
            ..Default::default()
        }],
        vertical_alignment: VerticalAlignment::Center,
        ..Default::default()
    }
}

pub trait ElementAlignmentExt {
    fn align_x(self, alignment: HorizontalAlignment) -> Self;
    fn align_y(self, alignment: VerticalAlignment) -> Self;
}

impl<Message> ElementAlignmentExt for Element<Message> {
    fn align_x(mut self, alignment: HorizontalAlignment) -> Self {
        self.children = self.children.align_x(alignment);
        self
    }

    fn align_y(mut self, alignment: VerticalAlignment) -> Self {
        self.children = self.children.align_y(alignment);
        self
    }
}

impl<Message> ElementAlignmentExt for Vec<Element<Message>> {
    fn align_x(mut self, alignment: HorizontalAlignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.horizontal_alignment = alignment;
        });
        self
    }

    fn align_y(mut self, alignment: VerticalAlignment) -> Self {
        self.iter_mut().for_each(|e| {
            e.vertical_alignment = alignment;
        });
        self
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
