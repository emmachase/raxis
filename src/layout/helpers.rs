use crate::{
    layout::model::{Direction, Element, ElementContent},
    util::str::StableString,
    widgets::text::Text,
};

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

#[macro_export]
macro_rules! row {
    ($($child: expr),*) => {
        row(vec![$($child.into()),*])
    };
}

#[macro_export]
macro_rules! column {
    ($($child: expr),*) => {
        column(vec![$($child.into()),*])
    };
}
