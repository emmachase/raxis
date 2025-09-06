// #![windows_subsystem = "windows"]

use std::{cell::RefCell, rc::Rc};

use raxis::{
    HookManager,
    layout::{
        helpers::center,
        model::{
            Border, BorderPlacement, BorderRadius, BoxAmount, Color, Direction, DropShadow,
            Element, ScrollConfig, Sizing, StrokeDashStyle, StrokeLineCap, StrokeLineJoin,
            VerticalAlignment,
        },
    },
    runtime::task::Task,
    util::{str::StableString, unique::combine_id},
    w_id,
    widgets::{
        button::Button,
        svg::ViewBox,
        svg_path::SvgPath,
        text::{ParagraphAlignment, Text, TextAlignment},
        text_input::TextInput,
        widget,
    },
};
use raxis_proc_macro::svg_path;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

enum Message {}

fn demo_box(label: &'static str, border: Border, radius: Option<BorderRadius>) -> Element<Message> {
    Element {
        id: Some(combine_id(w_id!(), label)),
        width: Sizing::grow(), //Sizing::fixed(160.0),
        height: Sizing::fixed(80.0),
        background_color: Some(0xFAFAFAFF.into()),
        padding: BoxAmount::all(8.0),
        border: Some(border),
        border_radius: radius,
        vertical_alignment: VerticalAlignment::Center,
        content: widget(Text::new(label).with_paragraph_alignment(ParagraphAlignment::Center)),
        ..Default::default()
    }
}

fn border_demos() -> Element<Message> {
    let inset = Border {
        width: 4.0,
        color: Color::from(0x1976D2FF),
        placement: BorderPlacement::Inset,
        ..Default::default()
    };
    let center = Border {
        width: 6.0,
        color: Color::from(0xE53935FF),
        placement: BorderPlacement::Center,
        ..Default::default()
    };
    let outset = Border {
        width: 8.0,
        color: Color::from(0xFB8C00FF),
        placement: BorderPlacement::Outset,
        ..Default::default()
    };

    let dashed = Border {
        width: 3.0,
        color: Color::from(0x424242FF),
        placement: BorderPlacement::Center,
        dash_style: Some(StrokeDashStyle::Dash),
        dash_cap: StrokeLineCap::Round,
        ..Default::default()
    };
    let dotted = Border {
        width: 3.0,
        color: Color::from(0x424242FF),
        placement: BorderPlacement::Center,
        dash_style: Some(StrokeDashStyle::Dot),
        dash_cap: StrokeLineCap::Square,
        ..Default::default()
    };
    let dash_dot = Border {
        width: 3.0,
        color: Color::from(0x424242FF),
        placement: BorderPlacement::Center,
        dash_style: Some(StrokeDashStyle::DashDot),
        dash_cap: StrokeLineCap::Triangle,
        ..Default::default()
    };
    let dash_dot_dot = Border {
        width: 3.0,
        color: Color::from(0x424242FF),
        placement: BorderPlacement::Center,
        dash_style: Some(StrokeDashStyle::DashDotDot),
        dash_cap: StrokeLineCap::Square,
        ..Default::default()
    };
    let custom = Border {
        width: 3.0,
        color: Color::from(0x424242FF),
        placement: BorderPlacement::Center,
        dash_style: Some(StrokeDashStyle::Custom {
            dashes: vec![6.0, 2.0, 2.0, 2.0],
            offset: 0.0,
        }),
        dash_cap: StrokeLineCap::Round,
        ..Default::default()
    };

    Element {
        id: Some(w_id!()),
        direction: Direction::TopToBottom,
        width: Sizing::grow(),
        height: Sizing::fit(),
        child_gap: 10.0,
        padding: BoxAmount::all(12.0),
        background_color: Some(Color::WHITE),
        border: Some(Border {
            width: 1.0,
            color: Color {
                r: 0.85,
                g: 0.85,
                b: 0.85,
                a: 1.0,
            },
            ..Default::default()
        }),
        border_radius: Some(BorderRadius::all(8.0)),
        // drop_shadow: Some(DropShadow::simple(1.0, 1.0).blur_radius(3.0)),
        children: vec![
            // Title
            Element {
                id: Some(w_id!()),
                width: Sizing::grow(),
                height: Sizing::fit(),
                content: widget(Text::new("Border demos").with_font_size(20.0)),
                ..Default::default()
            },
            // Placements row
            Element {
                direction: Direction::LeftToRight,
                width: Sizing::grow(),
                height: Sizing::fit(),
                wrap: true,
                child_gap: 10.0,
                children: vec![
                    demo_box("Inset 4px", inset, None),
                    demo_box("Center 6px", center, Some(BorderRadius::all(10.0))),
                    demo_box("Outset 8px", outset, Some(BorderRadius::all(12.0))),
                    demo_box("Dashed", dashed, Some(BorderRadius::tl_br(8.0))),
                    demo_box("Dotted", dotted, Some(BorderRadius::tr_bl(8.0))),
                    demo_box("DashDot", dash_dot, Some(BorderRadius::top(8.0))),
                    demo_box("DashDotDot", dash_dot_dot, Some(BorderRadius::bottom(8.0))),
                    demo_box("Custom", custom, None),
                    // Element {
                    //     id: Some(w_id!()),
                    //     width: Sizing::fit(),
                    //     height: Sizing::fit(),
                    //     content: Some(ElementContent::Widget(Box::new(Spinner::default()))),
                    //     ..Default::default()
                    // },
                ],
                ..Default::default()
            },
            // Dash styles row
            // Element {
            //     direction: Direction::LeftToRight,
            //     width: Sizing::grow(),
            //     height: Sizing::fit(),
            //     child_gap: 10.0,
            //     children: vec![
            //         demo_box("Dashed", dashed, Some(BorderRadius::tl_br(8.0))),
            //         demo_box("Dotted", dotted, Some(BorderRadius::tr_bl(8.0))),
            //         demo_box("DashDot", dash_dot, Some(BorderRadius::top(8.0))),
            //     ],
            //     ..Default::default()
            // },
            // Element {
            //     direction: Direction::LeftToRight,
            //     width: Sizing::grow(),
            //     height: Sizing::fit(),
            //     child_gap: 10.0,
            //     children: vec![
            //         demo_box("DashDotDot", dash_dot_dot, Some(BorderRadius::bottom(8.0))),
            //         demo_box("Custom", custom, None),
            //     ],
            //     ..Default::default()
            // },
        ],
        ..Default::default()
    }
}

#[derive(Debug, Clone)]
struct TodoItem {
    id: u32,
    text: StableString,
    completed: bool,
}

#[derive(Debug)]
struct TodoState {
    items: Vec<TodoItem>,
    next_id: u32,
    input_text: String,
}

fn todo_app(mut hook: HookManager<Message>) -> Element<Message> {
    let mut instance = hook.instance(w_id!());
    let todo_state = instance
        .use_hook(|| {
            Rc::new(RefCell::new(TodoState {
                items: vec![
                    TodoItem {
                        id: 1,
                        text: "Learn Raxis framework".into(),
                        completed: false,
                    },
                    TodoItem {
                        id: 2,
                        text: "Build todo app".into(),
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
        background_color: Some(0xF1F5EDFF.into()), // Light gray background
        direction: Direction::TopToBottom,
        width: Sizing::percent(1.0),
        height: Sizing::grow(),
        padding: BoxAmount::all(20.0),
        child_gap: 15.0,
        children: vec![
            // Header
            Element {
                id: Some(w_id!()),
                width: Sizing::grow(),
                height: Sizing::fit(),
                content: widget(Text::new("Todo List").with_font_size(32.0)),
                ..Default::default()
            },
            // Border demos
            border_demos(),
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
                        height: Sizing::fit().min(40.0).max(120.0),
                        scroll: Some(ScrollConfig {
                            vertical: Some(true),
                            sticky_bottom: Some(true),
                            ..Default::default()
                        }),
                        background_color: Some(Color::WHITE),
                        border_radius: Some(BorderRadius::all(8.0)),
                        border: Some(Border {
                            width: 1.0,
                            color: Color {
                                r: 0.85,
                                g: 0.85,
                                b: 0.85,
                                a: 1.0,
                            },
                            ..Default::default()
                        }),
                        drop_shadow: Some(
                            DropShadow::simple(0.0, 1.0)
                                .blur_radius(2.0)
                                .color(Color::from(0x0000000D)),
                        ),
                        // drop_shadow: Some(DropShadow::simple(1.0, 1.0).blur_radius(3.0)),
                        children: vec![Element {
                            id: Some(w_id!()),
                            width: Sizing::grow(),
                            height: Sizing::grow(),
                            padding: BoxAmount::new(5.0, 12.0, 5.0, 12.0),
                            content: widget(
                                TextInput::new()
                                    .with_text_changed_handler({
                                        let todo_state = todo_state.clone();
                                        move |text| {
                                            todo_state.borrow_mut().input_text = text.to_string();
                                        }
                                    })
                                    .with_paragraph_alignment(ParagraphAlignment::Center),
                            ),
                            ..Default::default()
                        }],

                        ..Default::default()
                    },
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::fit(),
                        height: Sizing::fixed(40.0),
                        border_radius: Some(BorderRadius::all(8.0)),
                        // drop_shadow: Some(DropShadow::simple(1.0, 1.0).blur_radius(3.0)),
                        content: widget(
                            Button::new()
                                .with_bg_color(Color::from(0xe91923ff))
                                .with_border_radius(8.0)
                                .with_no_border()
                                .with_drop_shadow(
                                    DropShadow::simple(0.0, 1.0)
                                        .blur_radius(2.0)
                                        .color(Color::from(0x0000000D)),
                                )
                                .with_click_handler({
                                    let todo_state = todo_state.clone();
                                    move |arenas, _| {
                                        let mut state = todo_state.borrow_mut();
                                        if !state.input_text.trim().is_empty() {
                                            let id = state.next_id;
                                            let text = state.input_text.clone();
                                            state.items.push(TodoItem {
                                                id,
                                                text: StableString::Interned(
                                                    arenas.strings.get_or_intern(text.trim()),
                                                ),
                                                // text.trim().to_string(),
                                                completed: false,
                                            });
                                            state.next_id += 1;
                                            // state.input_text.clear();
                                        }
                                    }
                                }),
                        ),

                        children: vec![Element {
                            id: Some(w_id!()),
                            width: Sizing::grow().min(80.0),
                            height: Sizing::grow(),
                            content: widget(
                                Text::new("Add")
                                    .with_paragraph_alignment(ParagraphAlignment::Center)
                                    .with_text_alignment(TextAlignment::Center)
                                    .with_color(Color::WHITE),
                            ),
                            ..Default::default()
                        }],

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
            // Svg::new(include_str!("../assets/discord.svg")).as_element(w_id!()),
        ],
        ..Default::default()
    }
}

fn todo_item(
    _hook: &mut HookManager<Message>,
    item: TodoItem,
    todo_state: Rc<RefCell<TodoState>>,
) -> Element<Message> {
    Element {
        id: Some(combine_id(w_id!(), item.id)),
        direction: Direction::LeftToRight,
        width: Sizing::grow(),
        height: Sizing::fit(),
        background_color: Some(Color::WHITE),
        border: Some(Border {
            width: 1.0,
            color: Color {
                r: 0.85,
                g: 0.85,
                b: 0.85,
                a: 1.0,
            },
            ..Default::default()
        }),
        border_radius: Some(BorderRadius::all(8.0)),
        drop_shadow: Some(
            DropShadow::simple(0.0, 1.0)
                .blur_radius(3.0)
                .color(Color::from(0x0000001A)),
        ),
        // drop_shadow: Some(DropShadow::simple(0.0, 2.0).blur_radius(4.0)),
        padding: BoxAmount::all(12.0),
        child_gap: 12.0,
        children: vec![
            // Checkbox
            Element {
                id: Some(combine_id(w_id!(), item.id)),
                width: Sizing::fixed(20.0),
                height: Sizing::fixed(20.0),
                background_color: Some(if item.completed {
                    Color::from(0x4CAF50FF)
                } else {
                    Color::WHITE
                }),
                direction: Direction::TopToBottom,
                vertical_alignment: VerticalAlignment::Center,
                border_radius: Some(BorderRadius::all(4.0)),
                // drop_shadow: Some(DropShadow::simple(0.5, 0.5).blur_radius(2.0)),
                content: widget(
                    Button::new()
                        .with_bg_color(if item.completed {
                            Color {
                                r: 0.3,
                                g: 0.7,
                                b: 0.3,
                                a: 1.0,
                            } // Green when completed
                        } else {
                            Color {
                                r: 0.95,
                                g: 0.95,
                                b: 0.95,
                                a: 1.0,
                            } // Light gray when not completed
                        })
                        .with_border_radius(4.0)
                        .with_border(
                            1.0,
                            if item.completed {
                                Color {
                                    r: 0.2,
                                    g: 0.5,
                                    b: 0.2,
                                    a: 1.0,
                                }
                            } else {
                                Color {
                                    r: 0.8,
                                    g: 0.8,
                                    b: 0.8,
                                    a: 1.0,
                                }
                            },
                        )
                        .with_click_handler({
                            let todo_state = todo_state.clone();
                            let item_id = item.id;
                            move |_, _| {
                                let mut state = todo_state.borrow_mut();
                                if let Some(todo) = state.items.iter_mut().find(|t| t.id == item_id)
                                {
                                    todo.completed = !todo.completed;
                                }
                            }
                        }),
                ),

                children: if item.completed {
                    vec![center(
                        SvgPath::new(svg_path!("M20 6 9 17l-5-5"), ViewBox::new(24.0, 24.0))
                            .with_size(16.0, 16.0)
                            .with_stroke(Color::WHITE)
                            .with_stroke_width(2.0)
                            .with_stroke_cap(StrokeLineCap::Round)
                            .with_stroke_join(StrokeLineJoin::Round)
                            .as_element(w_id!()), // Explicitly re-use id to re-use path geometry
                    )]
                } else {
                    vec![]
                },
                ..Default::default()
            },
            // Todo text
            Element {
                id: Some(combine_id(w_id!(), item.id)),
                width: Sizing::grow(),
                height: Sizing::fit(),
                vertical_alignment: VerticalAlignment::Center,
                content: widget(Text::new(item.text).with_font_size(16.0)),
                ..Default::default()
            },
            // Delete button
            Element {
                id: Some(combine_id(w_id!(), item.id)),
                width: Sizing::fit(),
                height: Sizing::fit(),
                vertical_alignment: VerticalAlignment::Center,
                content: widget(
                    Button::new()
                        .with_bg_color(Color::from(0xe91923ff))
                        .with_border_radius(4.0)
                        .with_no_border()
                        .with_drop_shadow(
                            DropShadow::simple(0.0, 1.0)
                                .blur_radius(2.0)
                                .color(Color::from(0x0000000D)),
                        )
                        // .with_border(
                        //     1.0,
                        //     Color {
                        //         r: 0.7,
                        //         g: 0.2,
                        //         b: 0.2,
                        //         a: 1.0,
                        //     },
                        // )
                        .with_click_handler({
                            let todo_state = todo_state.clone();
                            let item_id = item.id;
                            move |_, _| {
                                let mut state = todo_state.borrow_mut();
                                state.items.retain(|t| t.id != item_id);
                            }
                        }),
                ),

                children: vec![Element {
                    id: Some(combine_id(w_id!(), item.id)),
                    width: Sizing::grow().min(32.0),
                    height: Sizing::grow().min(32.0),
                    content: widget(
                        Text::new("✕")
                            .with_paragraph_alignment(ParagraphAlignment::Center)
                            .with_text_alignment(TextAlignment::Center)
                            .with_color(Color::WHITE),
                    ),
                    ..Default::default()
                }],

                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn view(_state: &(), hook: HookManager<Message>) -> Element<Message> {
    todo_app(hook)
}

fn update(_state: &mut (), _message: Message) -> Option<Task<Message>> {
    None
}

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    raxis::runtime::run_event_loop(view, update, (), |_state| None)
        .expect("Failed to run event loop");
}
