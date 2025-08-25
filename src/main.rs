#![windows_subsystem = "windows"]

use std::{cell::RefCell, rc::Rc};

use raxis::{
    HookManager,
    layout::model::{
        BorderRadius, BoxAmount, Direction, DropShadow, Element, ElementContent, ScrollConfig,
        Sizing, VerticalAlignment,
    },
    util::unique::combine_id,
    w_id,
    widgets::{
        button::Button,
        text::{ParagraphAlignment, Text},
        text_input::TextInput,
    },
};

#[derive(Debug, Clone)]
struct TodoItem {
    id: u32,
    text: String,
    completed: bool,
}

#[derive(Debug)]
struct TodoState {
    items: Vec<TodoItem>,
    next_id: u32,
    input_text: String,
}

fn todo_app(mut hook: HookManager) -> Element {
    let mut instance = hook.instance(w_id!());
    let todo_state = instance
        .use_hook(|| {
            Rc::new(RefCell::new(TodoState {
                items: vec![
                    TodoItem {
                        id: 1,
                        text: "Learn Raxis framework".to_string(),
                        completed: false,
                    },
                    TodoItem {
                        id: 2,
                        text: "Build todo app".to_string(),
                        completed: true,
                    },
                ],
                next_id: 3,
                input_text: String::new(),
            }))
        })
        .clone();

    Element {
        id: Some(w_id!()),
        background_color: Some(0xF5F5F5FF), // Light gray background
        direction: Direction::TopToBottom,
        width: Sizing::grow(),
        height: Sizing::grow(),
        padding: BoxAmount::all(20.0),
        child_gap: 15.0,
        children: vec![
            // Header
            Element {
                id: Some(w_id!()),
                width: Sizing::grow(),
                height: Sizing::fit(),
                content: Some(ElementContent::Widget(Box::new(
                    Text::new("Todo List").with_font_size(32.0),
                ))),
                ..Default::default()
            },
            // Input section
            Element {
                id: Some(w_id!()),
                direction: Direction::LeftToRight,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 10.0,
                children: vec![
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::Fit {
                            min: 40.0,
                            max: 120.0,
                        },
                        scroll: Some(ScrollConfig {
                            vertical: Some(true),
                            ..Default::default()
                        }),
                        background_color: Some(0xFFFFFFFF),
                        border_radius: Some(BorderRadius::all(5.0)),
                        drop_shadow: Some(DropShadow::simple(1.0, 1.0).blur_radius(3.0)),

                        children: vec![Element {
                            id: Some(w_id!()),
                            width: Sizing::grow(),
                            height: Sizing::grow(),
                            padding: BoxAmount::new(5.0, 12.0, 5.0, 12.0),
                            content: Some(ElementContent::Widget(Box::new(
                                TextInput::new()
                                    .with_text_changed_handler({
                                        let todo_state = todo_state.clone();
                                        move |text| {
                                            todo_state.borrow_mut().input_text = text.to_string();
                                        }
                                    })
                                    .with_paragraph_alignment(ParagraphAlignment::Center),
                            ))),
                            ..Default::default()
                        }],

                        ..Default::default()
                    },
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::fit(),
                        height: Sizing::fixed(40.0),
                        drop_shadow: Some(DropShadow::simple(1.0, 1.0).blur_radius(3.0)),
                        content: Some(ElementContent::Widget(Box::new(
                            Button::new("Add".to_string()).with_click_handler({
                                let todo_state = todo_state.clone();
                                move || {
                                    let mut state = todo_state.borrow_mut();
                                    if !state.input_text.trim().is_empty() {
                                        let id = state.next_id;
                                        let text = state.input_text.clone();
                                        state.items.push(TodoItem {
                                            id,
                                            text: text.trim().to_string(),
                                            completed: false,
                                        });
                                        state.next_id += 1;
                                        state.input_text.clear();
                                    }
                                }
                            }),
                        ))),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            // Todo items list
            Element {
                id: Some(w_id!()),
                direction: Direction::TopToBottom,
                width: Sizing::grow(),
                height: Sizing::grow(),
                child_gap: 8.0,
                scroll: Some(ScrollConfig::default()),
                children: {
                    let state = todo_state.borrow();
                    state
                        .items
                        .iter()
                        .map(|item| todo_item(&mut hook, item.clone(), todo_state.clone()))
                        .collect()
                },
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn todo_item(
    hook: &mut HookManager,
    item: TodoItem,
    todo_state: Rc<RefCell<TodoState>>,
) -> Element {
    Element {
        id: Some(combine_id(w_id!(), item.id.into())),
        direction: Direction::LeftToRight,
        width: Sizing::grow(),
        height: Sizing::fit(),
        background_color: Some(0xFFFFFFFF),
        border_radius: Some(BorderRadius::all(8.0)),
        drop_shadow: Some(DropShadow::simple(1.0, 2.0).blur_radius(4.0)),
        padding: BoxAmount::all(12.0),
        child_gap: 12.0,
        children: vec![
            // Checkbox
            Element {
                id: Some(combine_id(w_id!(), item.id.into())),
                width: Sizing::fixed(20.0),
                height: Sizing::fixed(20.0),
                background_color: Some(if item.completed {
                    0x4CAF50FF
                } else {
                    0xFFFFFFFF
                }),
                vertical_alignment: VerticalAlignment::Center,
                border_radius: Some(BorderRadius::all(3.0)),
                drop_shadow: Some(DropShadow::simple(0.5, 0.5).blur_radius(2.0)),
                content: Some(ElementContent::Widget(Box::new(
                    Button::new(if item.completed { "✓" } else { "" }.to_string())
                        .with_click_handler({
                            let todo_state = todo_state.clone();
                            let item_id = item.id;
                            move || {
                                let mut state = todo_state.borrow_mut();
                                if let Some(todo) = state.items.iter_mut().find(|t| t.id == item_id)
                                {
                                    todo.completed = !todo.completed;
                                }
                            }
                        }),
                ))),
                ..Default::default()
            },
            // Todo text
            Element {
                id: Some(combine_id(w_id!(), item.id.into())),
                width: Sizing::grow(),
                height: Sizing::fit(),
                vertical_alignment: VerticalAlignment::Center,
                content: Some(ElementContent::Widget(Box::new(
                    Text::new(&item.text).with_font_size(16.0),
                ))),
                ..Default::default()
            },
            // Delete button
            Element {
                id: Some(combine_id(w_id!(), item.id.into())),
                width: Sizing::fit(),
                height: Sizing::fit(),
                vertical_alignment: VerticalAlignment::Center,
                content: Some(ElementContent::Widget(Box::new(
                    Button::new("✕".to_string()).with_click_handler({
                        let todo_state = todo_state.clone();
                        let item_id = item.id;
                        move || {
                            let mut state = todo_state.borrow_mut();
                            state.items.retain(|t| t.id != item_id);
                        }
                    }),
                ))),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn view(hook: HookManager) -> Element {
    todo_app(hook)
}

fn main() {
    raxis::runtime::run_event_loop(view).expect("Failed to run event loop");
}
