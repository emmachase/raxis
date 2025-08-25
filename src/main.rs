#![windows_subsystem = "windows"]

use std::{cell::RefCell, rc::Rc};

use raxis::{
    HookManager,
    layout::model::{
        BorderRadius, BoxAmount, Direction, DropShadow, Element, ElementContent, ScrollConfig,
        Sizing,
    },
    w_id,
    widgets::{button::Button, text::Text, text_input::TextInput},
};

fn view(mut hook: HookManager) -> Element {
    let mut instance = hook.instance(w_id!());
    let state = instance.use_hook(|| Rc::new(RefCell::new(0))).clone();

    Element {
        id: Some(w_id!()),
        background_color: Some(0xFF0000FF),
        direction: Direction::LeftToRight,
        width: Sizing::grow(),
        height: Sizing::Fit {
            min: 200.0,
            max: f32::INFINITY,
        },
        child_gap: 10.0,
        padding: BoxAmount::all(10.0),
        scroll: Some(ScrollConfig {
            // horizontal: Some(true),
            ..Default::default()
        }),
        children: vec![
            Element {
                id: Some(w_id!()),
                background_color: Some(0x00FF00FF),
                drop_shadow: Some(DropShadow::simple(5.0, 5.0).blur_radius(5.0)),
                width: Sizing::Fit {
                    min: 10.0,
                    max: 100.0,
                },
                height: Sizing::Fit {
                    min: 100.0,
                    max: f32::INFINITY,
                },
                // scroll: Some(ScrollConfig {
                //     // horizontal: Some(true),
                //     ..Default::default()
                // }),

                // content: Some(ElementContent::Widget(Box::new(TextInput::new()))),
                children: vec![Element {
                    id: Some(w_id!()),
                    background_color: Some(0x00FF00FF),
                    width: Sizing::grow(),
                    height: Sizing::grow(),

                    content: Some(ElementContent::Widget(Box::new(TextInput::new()))),
                    ..Default::default()
                }],
                ..Default::default()
            },
            Element {
                id: Some(w_id!()),
                // background_color: Some(0x00FF00FF),
                // drop_shadow: Some(DropShadow::simple(5.0, 5.0).blur_radius(5.0)),
                width: Sizing::fit(),
                height: Sizing::fit(),
                // scroll: Some(ScrollConfig {
                //     // horizontal: Some(true),
                //     ..Default::default()
                // }),
                content: Some(ElementContent::Widget(Box::new(
                    Text::new("Hello world!").with_font_size(24.0),
                ))),

                ..Default::default()
            },
            Element {
                id: Some(w_id!()),
                drop_shadow: Some(DropShadow::new(2.0, 2.0, 0.0, 6.0, 0x00000060)),
                content: Some(ElementContent::Widget(Box::new(
                    Button::new(format!("Button {}", state.borrow())).with_click_handler(
                        move || {
                            println!("Button clicked");
                            *state.borrow_mut() += 1;
                        },
                    ),
                ))),
                ..Default::default()
            },
            Element {
                id: Some(w_id!()),
                background_color: Some(0x00FFFFFF),
                drop_shadow: Some(DropShadow::simple(5.0, 5.0)),
                border_radius: Some(BorderRadius::tr_bl(10.0)),
                width: Sizing::fixed(50.0),
                height: Sizing::fixed(50.0),
                // scroll: Some(ScrollConfig {
                //     // horizontal: Some(true),
                //     ..Default::default()
                // }),

                // content: Some(ElementContent::Widget(Box::new(TextInput::new()))),
                ..Default::default()
            },
        ],

        ..Default::default()
    }
}

fn main() {
    raxis::runtime::run_event_loop(view).expect("Failed to run event loop");
}
