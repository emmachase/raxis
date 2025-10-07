// #![windows_subsystem = "windows"]

use std::{
    cell::RefCell,
    collections::HashSet,
    rc::Rc,
    time::{Duration, Instant},
};

use lazy_static::lazy_static;
use raxis::{
    HookManager,
    layout::{
        helpers::{center, spacer},
        model::{
            Border, BorderPlacement, BorderRadius, BoxAmount, Color, Direction, DropShadow,
            Element, FloatingConfig, ScrollConfig, Sizing, StrokeDashStyle, StrokeLineCap,
            StrokeLineJoin, TextShadow, VerticalAlignment,
        },
    },
    math::easing::Easing,
    row,
    runtime::{Backdrop, font_manager::FontIdentifier, scroll::ScrollPosition, task::Task},
    use_animation,
    util::{str::StableString, unique::combine_id},
    w_id,
    widgets::{
        button::Button,
        image::Image,
        slider::Slider,
        svg::ViewBox,
        svg_path::SvgPath,
        text::{ParagraphAlignment, Text, TextAlignment},
        text_input::TextInput,
        toggle::Toggle,
        widget,
    },
};
use raxis_core::svg;
use raxis_proc_macro::svg_path;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[derive(Default)]
struct State {
    modal_open: bool,
}

enum Message {
    ToggleModal,
}

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
            dashes: &[6.0, 2.0, 2.0, 2.0],
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

fn animated_button(hook: &mut HookManager<Message>) -> Element<Message> {
    let mut instance = hook.instance(w_id!());
    let toggled = instance.use_state(|| false);
    let animation =
        use_animation(&mut instance, *toggled.borrow()).duration(Duration::from_millis(100));
    let width = animation.interpolate(hook, 50.0, 100.0, Instant::now());

    Button::new()
        .with_click_handler(move |_, _| {
            let mut v = toggled.borrow_mut();
            *v = !*v;
        })
        .as_element(w_id!(), Text::new("Animate"))
        .with_width(Sizing::fixed(width))
        .with_height(Sizing::fit())
    // Element {
    //     id: Some(w_id!()),
    //     width: Sizing::fixed(width),
    //     height: Sizing::fit(),
    //     content: widget(Text::new("Animated Button").with_font_size(20.0)),
    //     ..Default::default()
    // }
}

fn slider_demos(hook: &mut HookManager<Message>) -> Element<Message> {
    let mut instance = hook.instance(w_id!());
    let volume = instance.use_state(|| 50.0);
    let brightness = instance.use_state(|| 75.0);
    let temperature = instance.use_state(|| 20.5);
    let steps = instance.use_state(|| 5.0);

    Element {
        id: Some(w_id!()),
        direction: Direction::TopToBottom,
        width: Sizing::grow(),
        height: Sizing::fit(),
        background_color: Some(Color::WHITE),
        padding: BoxAmount::all(12.0),
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
        child_gap: 16.0,
        children: vec![
            // Title
            Element {
                id: Some(w_id!()),
                width: Sizing::grow(),
                height: Sizing::fit(),
                content: widget(
                    Text::new("Slider demos")
                        .with_font_size(20.0)
                        .with_text_shadows(vec![
                            TextShadow {
                                offset_x: 2.0,
                                offset_y: 2.0,
                                blur_radius: 2.0,
                                color: Color::from(0xFF00FFFF),
                            },
                            TextShadow {
                                offset_x: -2.0,
                                offset_y: -2.0,
                                blur_radius: 2.0,
                                color: Color::from(0xFFFF00FF),
                            },
                        ]),
                ),
                ..Default::default()
            },
            // Volume slider
            Element {
                id: Some(w_id!()),
                direction: Direction::TopToBottom,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 8.0,
                children: vec![
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        content: widget(
                            Text::new(format!("Volume: {:.0}", *volume.borrow()))
                                .with_font_size(14.0),
                        ),
                        ..Default::default()
                    },
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        padding: BoxAmount::vertical(4.0),
                        content: widget(
                            Slider::new(0.0, 100.0, *volume.borrow()).with_value_change_handler({
                                let volume = volume.clone();
                                move |value, _, _| {
                                    *volume.borrow_mut() = value;
                                }
                            }),
                        ),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            // Brightness slider with custom colors
            Element {
                id: Some(w_id!()),
                direction: Direction::TopToBottom,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 8.0,
                children: vec![
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        content: widget(
                            Text::new(format!("Brightness: {:.0}%", *brightness.borrow()))
                                .with_font_size(14.0),
                        ),
                        ..Default::default()
                    },
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        padding: BoxAmount::vertical(4.0),
                        content: widget(
                            Slider::new(0.0, 100.0, *brightness.borrow())
                                .with_filled_track_color(Color::from(0xFBBF24FF)) // Amber
                                .with_thumb_color(Color::from(0xFBBF24FF))
                                .with_thumb_border_color(Color::from(0xFBBF24FF).deviate(0.1))
                                .with_value_change_handler({
                                    let brightness = brightness.clone();
                                    move |value, _, _| {
                                        *brightness.borrow_mut() = value;
                                    }
                                }),
                        ),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            // Temperature slider with step
            Element {
                id: Some(w_id!()),
                direction: Direction::TopToBottom,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 8.0,
                children: vec![
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        content: widget(
                            Text::new(format!("Temperature: {:.1}°C", *temperature.borrow()))
                                .with_font_size(14.0),
                        ),
                        ..Default::default()
                    },
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        padding: BoxAmount::vertical(4.0),
                        content: widget(
                            Slider::new(15.0, 30.0, *temperature.borrow())
                                .with_step(0.5)
                                .with_filled_track_color(Color::from(0xEF4444FF)) // Red
                                .with_thumb_color(Color::from(0xEF4444FF))
                                .with_thumb_border_color(Color::from(0xEF4444FF).deviate(0.1))
                                .with_value_change_handler({
                                    let temperature = temperature.clone();
                                    move |value, _, _| {
                                        *temperature.borrow_mut() = value;
                                    }
                                }),
                        ),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            // Stepped slider (discrete values)
            Element {
                id: Some(w_id!()),
                direction: Direction::TopToBottom,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 8.0,
                children: vec![
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        content: widget(
                            Text::new(format!("Steps: {:.0}", *steps.borrow()))
                                .with_font_size(14.0),
                        ),
                        ..Default::default()
                    },
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        padding: BoxAmount::vertical(4.0),
                        content: widget(
                            Slider::new(0.0, 10.0, *steps.borrow())
                                .with_step(1.0)
                                .with_filled_track_color(Color::from(0x8B5CF6FF)) // Purple
                                .with_thumb_color(Color::from(0x8B5CF6FF))
                                .with_thumb_border_color(Color::from(0x8B5CF6FF).deviate(0.1))
                                .with_value_change_handler({
                                    let steps = steps.clone();
                                    move |value, _, _| {
                                        *steps.borrow_mut() = value;
                                    }
                                }),
                        ),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            // Disabled slider
            Element {
                id: Some(w_id!()),
                direction: Direction::TopToBottom,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 8.0,
                children: vec![
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        content: widget(Text::new("Disabled slider").with_font_size(14.0)),
                        ..Default::default()
                    },
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        padding: BoxAmount::vertical(4.0),
                        content: widget(Slider::new(0.0, 100.0, 30.0).disabled()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn toggle_demos(hook: &mut HookManager<Message>) -> Element<Message> {
    let mut instance = hook.instance(w_id!());
    let wifi = instance.use_state(|| true);
    let bluetooth = instance.use_state(|| false);
    let notifications = instance.use_state(|| true);
    let dark_mode = instance.use_state(|| false);

    Element {
        id: Some(w_id!()),
        direction: Direction::TopToBottom,
        width: Sizing::grow(),
        height: Sizing::fit(),
        background_color: Some(Color::WHITE),
        padding: BoxAmount::all(12.0),
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
        child_gap: 16.0,
        children: vec![
            // Title
            Element {
                id: Some(w_id!()),
                width: Sizing::grow(),
                height: Sizing::fit(),
                content: widget(Text::new("Toggle/Switch demos").with_font_size(20.0)),
                ..Default::default()
            },
            // WiFi toggle
            Element {
                id: Some(w_id!()),
                direction: Direction::LeftToRight,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 12.0,
                children: vec![
                    Toggle::new(*wifi.borrow())
                        .with_toggle_handler({
                            let wifi = wifi.clone();
                            move |checked, _, _| {
                                *wifi.borrow_mut() = checked;
                            }
                        })
                        .as_element(w_id!()),
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        content: widget(
                            Text::new(format!(
                                "WiFi: {}",
                                if *wifi.borrow() { "On" } else { "Off" }
                            ))
                            .with_font_size(14.0),
                        ),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            // Bluetooth toggle with custom colors
            Element {
                id: Some(w_id!()),
                direction: Direction::LeftToRight,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 12.0,
                children: vec![
                    Toggle::new(*bluetooth.borrow())
                        .with_track_colors(
                            Color::from(0xE2E8F0FF), // Off: Neutral-200
                            Color::from(0x3B82F6FF), // On: Blue
                        )
                        .with_toggle_handler({
                            let bluetooth = bluetooth.clone();
                            move |checked, _, _| {
                                *bluetooth.borrow_mut() = checked;
                            }
                        })
                        .as_element(w_id!()),
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        content: widget(
                            Text::new(format!(
                                "Bluetooth: {}",
                                if *bluetooth.borrow() {
                                    "Connected"
                                } else {
                                    "Disconnected"
                                }
                            ))
                            .with_font_size(14.0),
                        ),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            // Notifications toggle with green accent
            Element {
                id: Some(w_id!()),
                direction: Direction::LeftToRight,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 12.0,
                children: vec![
                    Toggle::new(*notifications.borrow())
                        .with_track_colors(
                            Color::from(0xE2E8F0FF), // Off: Neutral-200
                            Color::from(0x10B981FF), // On: Green
                        )
                        .with_toggle_handler({
                            let notifications = notifications.clone();
                            move |checked, _, _| {
                                *notifications.borrow_mut() = checked;
                            }
                        })
                        .as_element(w_id!()),
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        content: widget(
                            Text::new(format!(
                                "Notifications: {}",
                                if *notifications.borrow() {
                                    "Enabled"
                                } else {
                                    "Disabled"
                                }
                            ))
                            .with_font_size(14.0),
                        ),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            // Dark mode toggle with custom size and animation
            Element {
                id: Some(w_id!()),
                direction: Direction::LeftToRight,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 12.0,
                children: vec![
                    Toggle::new(*dark_mode.borrow())
                        .with_size(52.0, 28.0) // Larger toggle
                        .with_track_colors(
                            Color::from(0xE2E8F0FF), // Off: Neutral-200
                            Color::from(0x8B5CF6FF), // On: Purple
                        )
                        .with_animation_duration(Duration::from_millis(400)) // Slower animation
                        .with_animation_easing(Easing::EaseInOutCubic)
                        .with_toggle_handler({
                            let dark_mode = dark_mode.clone();
                            move |checked, _, _| {
                                *dark_mode.borrow_mut() = checked;
                            }
                        })
                        .as_element(w_id!()),
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        content: widget(
                            Text::new(format!(
                                "Dark Mode: {}",
                                if *dark_mode.borrow() {
                                    "🌙 Night"
                                } else {
                                    "☀️ Day"
                                }
                            ))
                            .with_font_size(14.0),
                        ),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            // Disabled toggle
            Element {
                id: Some(w_id!()),
                direction: Direction::LeftToRight,
                width: Sizing::grow(),
                height: Sizing::fit(),
                child_gap: 12.0,
                children: vec![
                    Toggle::new(false).disabled().as_element(w_id!()),
                    Element {
                        id: Some(w_id!()),
                        width: Sizing::grow(),
                        height: Sizing::fit(),
                        content: widget(
                            Text::new("Disabled Toggle")
                                .with_font_size(14.0)
                                .with_color(Color::from(0x94A3B8FF)),
                        ),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn todo_app(hook: &mut HookManager<Message>) -> Element<Message> {
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

    let pixie = Image::new("assets/pixie.jpg");

    Element {
        id: Some(w_id!()),
        background_color: Some(0xF1F5EDFF.into()), // Light gray background
        direction: Direction::TopToBottom,
        width: Sizing::grow(),
        height: Sizing::grow(),
        padding: BoxAmount::all(20.0),
        child_gap: 15.0,
        children: vec![
            // Header
            // Element {
            //     id: Some(w_id!()),
            //     width: Sizing::grow(),
            //     height: Sizing::fit(),
            //     content: widget(Text::new("Todo List").with_font_size(32.0)),
            //     ..Default::default()
            // },
            row![
                Text::new("Todo List").with_font_size(24.0).as_element(),
                spacer(),
                Button::new()
                    .with_click_handler(|_, s| s.publish(Message::ToggleModal))
                    .as_element(w_id!(), Text::new("Settings"))
            ]
            .with_width(Sizing::grow()),
            // Toggle demos
            toggle_demos(hook),
            // Slider demos
            slider_demos(hook),
            // Border demos
            border_demos(),
            animated_button(hook),
            pixie.as_element(w_id!()),
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
                height: Sizing::fit(),
                child_gap: 8.0,
                children: {
                    let state = todo_state.borrow();
                    state
                        .items
                        .iter()
                        .map(|item| todo_item(hook, item.clone(), todo_state.clone()))
                        .collect()
                },
                ..Default::default()
            },
            virtual_scroll(hook),
            // Svg::new(include_str!("../assets/discord.svg")).as_element(w_id!()),
        ],
        ..Default::default()
    }
}

const FILE_CONTENTS: &str = include_str!("../ipsum.txt");
lazy_static! {
    static ref LINES: Vec<String> = FILE_CONTENTS.lines().map(|s| s.to_string()).collect();
}

fn short_size(size: usize) -> String {
    let size_f = size as f64;
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.2} KB", size_f / 1024.0)
    } else {
        format!("{:.2} MB", size_f / 1024.0 / 1024.0)
    }
}

fn virtual_scroll(hook: &mut HookManager<Message>) -> Element<Message> {
    let container_id = w_id!();

    let mut state = hook.instance(container_id);
    let max_content_width = state.use_hook(|| Rc::new(RefCell::new(0.0f32))).clone();
    let max_line_length = state.use_hook(|| Rc::new(RefCell::new(0usize))).clone();
    let show_more = state
        .use_hook(|| Rc::new(RefCell::new(HashSet::<usize>::new())))
        .clone();

    let total_items = LINES.len();
    let line_height_no_gap = 10.0;
    let gap = 2.0;
    let padding = BoxAmount::all(8.0);
    let buffer_items_per_side = 2usize;

    let truncate_threshold = 3000;

    let line_height = line_height_no_gap + gap;

    let container_dims = hook
        .scroll_state_manager
        .get_container_dimensions(container_id);

    let content_dims = hook
        .scroll_state_manager
        .get_previous_content_dimensions(container_id);

    let mut max_content_width = max_content_width.borrow_mut();
    *max_content_width = max_content_width.max(content_dims.0);

    let visible_items =
        (container_dims.1 / line_height).ceil() as usize + buffer_items_per_side * 2;
    if container_dims.1 == 0.0 {
        // Need to run layout to get container dimensions
        hook.invalidate_layout();
    }

    let ScrollPosition {
        x: _scroll_x,
        y: scroll_y,
    } = hook.scroll_state_manager.get_scroll_position(container_id);

    let pre_scroll_items = (((scroll_y + gap - padding.top) / line_height).floor() as usize)
        .saturating_sub(buffer_items_per_side);
    let post_scroll_items = total_items
        .saturating_sub(pre_scroll_items)
        .saturating_sub(visible_items)
        .max(0);

    Element {
        id: Some(container_id),
        direction: Direction::TopToBottom,
        width: Sizing::grow(),
        height: Sizing::Fit {
            min: 0.0,
            max: 300.0,
        },
        scroll: Some(ScrollConfig {
            horizontal: Some(true),
            vertical: Some(true),
            safe_area_padding: Some(BoxAmount::from(4.0)),
            scrollbar_track_radius: Some(BorderRadius::all(4.0)),
            scrollbar_thumb_radius: Some(BorderRadius::all(4.0)),
            ..Default::default()
        }),
        border: Some(Border {
            width: 1.0,
            color: Color::from(0x000000FF),
            ..Default::default()
        }),
        child_gap: gap,
        padding,
        children: {
            // DWrite runs into precision issues with really long text (it only uses f32)
            // So we have to calculate the width manually with a f64
            // Obviously won't work with special glyphs but what are you gonna do? /shrug
            const MONO_CHAR_WIDTH: f64 = 6.02411;

            let mut max_line_length = max_line_length.borrow_mut();

            let mut text_children = (pre_scroll_items
                ..(pre_scroll_items + visible_items).min(total_items))
                .map(|i| {
                    if LINES[i].len() > truncate_threshold && !show_more.borrow().contains(&i) {
                        *max_line_length = max_line_length.max(truncate_threshold);

                        Element {
                            id: Some(combine_id(w_id!(), i % visible_items)),
                            height: Sizing::fixed(line_height_no_gap),
                            children: vec![
                                Text::new(StableString::Static(&LINES[i][0..truncate_threshold]))
                                    .with_word_wrap(false)
                                    .with_font_family(FontIdentifier::System(
                                        "Lucida Console".to_string(),
                                    ))
                                    .with_assisted_width(
                                        (MONO_CHAR_WIDTH * truncate_threshold as f64) as f32,
                                    )
                                    .with_font_size(10.0)
                                    .as_element()
                                    .with_id(combine_id(w_id!(), i % visible_items))
                                    .with_height(Sizing::fixed(line_height_no_gap)),
                                Button::new()
                                    .with_click_handler({
                                        let show_more = show_more.clone();
                                        move |_, _| {
                                            show_more.borrow_mut().insert(i);
                                        }
                                    })
                                    .as_element(
                                        combine_id(w_id!(), i % visible_items),
                                        Text::new(format!(
                                            "Show more ({})",
                                            short_size(LINES[i].len())
                                        ))
                                        .with_font_size(8.0),
                                    ),
                            ],

                            ..Default::default()
                        }
                    } else {
                        *max_line_length = max_line_length.max(LINES[i].len());

                        Text::new(StableString::Static(&LINES[i]))
                            .with_word_wrap(false)
                            .with_font_family(FontIdentifier::System("Lucida Console".to_string()))
                            .with_font_size(10.0)
                            .with_assisted_width((MONO_CHAR_WIDTH * LINES[i].len() as f64) as f32)
                            .as_element()
                            .with_id(combine_id(w_id!(), i % visible_items))
                            .with_height(Sizing::fixed(line_height_no_gap))
                    }
                })
                .collect();

            let keep_width = ((*max_line_length as f64 * MONO_CHAR_WIDTH) as f32)
                .max(*max_content_width - padding.left - padding.right);

            let mut children = vec![];
            if pre_scroll_items > 0 {
                children.push(Element {
                    id: Some(w_id!()),
                    width: Sizing::fixed(keep_width),
                    height: Sizing::fixed(line_height * pre_scroll_items as f32 - gap),
                    ..Default::default()
                });
            }

            children.append(&mut text_children);

            if post_scroll_items > 0 {
                children.push(Element {
                    id: Some(w_id!()),
                    width: Sizing::fixed(keep_width),
                    height: Sizing::fixed(line_height * post_scroll_items as f32 - gap),
                    ..Default::default()
                });
            }
            children
        },
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
                        SvgPath::new(svg![svg_path!("M20 6 9 17l-5-5")], ViewBox::new(24.0, 24.0))
                            .with_size(16.0, 16.0)
                            .with_stroke(Color::WHITE)
                            .with_stroke_width(2.0)
                            .with_stroke_cap(StrokeLineCap::Round)
                            .with_stroke_join(StrokeLineJoin::Round)
                            .as_element(combine_id(w_id!(), item.id)),
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

fn modal(state: &State, hook: &mut HookManager<Message>) -> Element<Message> {
    let mut instance = hook.instance(w_id!());
    let opacity = use_animation(&mut instance, state.modal_open);
    let opacity = opacity.interpolate(hook, 0.0, 1.0, Instant::now());

    if !state.modal_open && opacity == 0.0 {
        return Element::default();
    }

    Element {
        id: Some(w_id!()),
        width: Sizing::percent(1.0),
        height: Sizing::percent(1.0),
        opacity: Some(opacity),
        background_color: Some(Color::from(0x00000080)),
        floating: Some(FloatingConfig {
            ..Default::default()
        }),

        content: widget(
            Button::new()
                .clear()
                .with_click_handler(|_, s| s.publish(Message::ToggleModal)),
        ),

        children: vec![center(Element {
            id: Some(w_id!()),
            // width: Sizing::grow(),
            // height: Sizing::grow(),
            background_color: Some(Color::from(0xFFFFFFFF)),
            border_radius: Some(BorderRadius::all(4.0)),
            border: Some(Border {
                width: 1.0,
                color: Color::from(0x00000080),
                ..Default::default()
            }),

            children: vec![
                Text::new("Modal")
                    .with_text_alignment(TextAlignment::Center)
                    .with_color(Color::BLACK)
                    .as_element(),
            ],
            ..Default::default()
        })],

        ..Default::default()
    }
}

fn view(state: &State, hook: &mut HookManager<Message>) -> Element<Message> {
    Element {
        direction: Direction::TopToBottom,
        width: Sizing::Grow {
            min: 600.0,
            max: f32::MAX,
        },
        height: Sizing::Grow {
            min: 400.0,
            max: f32::MAX,
        },
        children: vec![
            // Element {
            //     height: Sizing::fixed(28.0),
            //     padding: BoxAmount::horizontal(8.0),
            //     children: vec![
            //         Text::new("Raxis Demo")
            //             .with_color(if hook.window_active {
            //                 Color::WHITE
            //             } else {
            //                 Color::from_rgba(1.0, 1.0, 1.0, 0.5)
            //             })
            //             .with_paragraph_alignment(ParagraphAlignment::Center)
            //             .with_font_size(12.0)
            //             .as_element()
            //             .with_height(Sizing::grow()),
            //     ],
            //     ..Default::default()
            // },
            Element {
                id: Some(w_id!()),
                direction: Direction::TopToBottom,
                width: Sizing::grow(),
                height: Sizing::grow(),
                scroll: Some(ScrollConfig {
                    vertical: Some(true),
                    safe_area_padding: Some(BoxAmount::from(4.0)),
                    scrollbar_track_radius: Some(BorderRadius::all(4.0)),
                    scrollbar_thumb_radius: Some(BorderRadius::all(4.0)),
                    ..Default::default()
                }),

                children: vec![todo_app(hook)],
                ..Default::default()
            },
            modal(state, hook),
        ],
        ..Default::default()
    }
    // Element {
    //     direction: Direction::TopToBottom,
    //     width: Sizing::fixed(100.0),
    //     height: Sizing::fixed(100.0),
    //     scroll: Some(ScrollConfig {
    //         horizontal: Some(true),
    //         vertical: Some(true),
    //         ..Default::default()
    //     }),
    //     children: vec![todo_app(hook)],
    //     ..Default::default()
    // }
    // todo_app(hook)
}

fn update(state: &mut State, message: Message) -> Option<Task<Message>> {
    match message {
        Message::ToggleModal => {
            state.modal_open = !state.modal_open;
            None
        }
    }
}

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    raxis::Application::new(State::default(), view, update, |_state| None)
        .with_title("Raxis Demo")
        .with_backdrop(Backdrop::Mica)
        .run()
        .expect("Failed to run event loop");
}
