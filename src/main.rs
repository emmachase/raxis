// #![windows_subsystem = "windows"]

use raxis::{
    layout::model::{BoxAmount, Direction, Element, ElementContent, ScrollConfig, Sizing},
    w_id,
    widgets::text_input::TextInput,
};

fn view() -> Element {
    Element {
        id: Some(w_id!()),
        background_color: Some(0xFF0000FF),
        direction: Direction::TopToBottom,
        width: Sizing::grow(),
        height: Sizing::Fit {
            min: 200.0,
            max: f32::INFINITY,
        },
        child_gap: 10.0,
        padding: BoxAmount::all(10.0),
        scroll: Some(ScrollConfig {
            horizontal: Some(true),
            ..Default::default()
        }),
        children: vec![Element {
            id: Some(w_id!()),
            background_color: Some(0x00FF00FF),
            width: Sizing::grow(),
            height: Sizing::Fit {
                min: 100.0,
                max: f32::INFINITY,
            },

            content: Some(ElementContent::Widget(Box::new(TextInput::new()))),
            ..Default::default()
        }],

        ..Default::default()
    }
}

fn main() {
    raxis::runtime::run_event_loop(view).expect("Failed to run event loop");
}
