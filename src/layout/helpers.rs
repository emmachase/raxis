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
