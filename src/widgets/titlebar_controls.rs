//! Default Windows-style titlebar controls (minimize, maximize/restore, close buttons).
//!
//! These buttons are assigned the `MAGIC_ID_TITLEBAR_*` IDs so that the runtime's
//! non-client hit testing can return `HTMINBUTTON`, `HTMAXBUTTON`, and `HTCLOSE`
//! when the mouse is over them, enabling proper Windows caption button behavior.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::util::windows::is_windows_11;
use crate::widgets::svg_path::ColorChoice;
use crate::widgets::text::Text;
use crate::{
    close_window, minimize_window, toggle_maximize_window, w_id,
    layout::model::{Color, Direction, Element, Sizing},
    layout::helpers::center,
    Animation, HookManager,
    widgets::button::{Button, ButtonState},
    MAGIC_ID_TITLEBAR_CLOSE, MAGIC_ID_TITLEBAR_MAXIMIZE, MAGIC_ID_TITLEBAR_MINIMIZE,
};

/// Default button width for Windows-style caption buttons (46px at 100% DPI).
pub const CAPTION_BUTTON_WIDTH: f32 = 46.0;
/// Default button height for Windows-style caption buttons (matches titlebar height).
pub const CAPTION_BUTTON_HEIGHT: f32 = 32.0;

/// Creates a row of Windows-style titlebar control buttons (minimize, maximize, close).
///
/// The buttons are assigned the magic IDs so that `WM_NCHITTEST` returns the correct
/// hit-test codes (`HTMINBUTTON`, `HTMAXBUTTON`, `HTCLOSE`) when hovering over them.
///
/// # Example
/// ```ignore
/// fn view(state: &State, hook: &mut HookManager<Message>) -> Element<Message> {
///     Element {
///         direction: Direction::TopToBottom,
///         children: vec![
///             // Custom titlebar row
///             Element {
///                 direction: Direction::LeftToRight,
///                 height: Sizing::fixed(32.0),
///                 children: vec![
///                     // ... window title, drag area, etc.
///                     titlebar_controls(),
///                 ],
///                 ..Default::default()
///             },
///             // ... rest of window content
///         ],
///         ..Default::default()
///     }
/// }
/// ```
pub fn titlebar_controls<Message: 'static + Send + Clone>(
    hook: &mut HookManager<Message>,
) -> Element<Message> {
    Element {
        direction: Direction::LeftToRight,
        width: Sizing::fit(),
        height: Sizing::fixed(CAPTION_BUTTON_HEIGHT),
        children: vec![
            minimize_button(hook),
            maximize_button(hook),
            close_button(hook),
        ],
        color: Some(Color::WHITE),
        ..Default::default()
    }
}

const WINDOWS_11_ICON_FONT: &str = "Segoe Fluent Icons";
const WINDOWS_10_ICON_FONT: &str = "Segoe MDL2 Assets";

fn get_font() -> &'static str {
    if is_windows_11() {
        WINDOWS_11_ICON_FONT
    } else {
        WINDOWS_10_ICON_FONT
    }
}

/// Animation duration for hover/press transitions.
const ANIMATION_DURATION: Duration = Duration::from_millis(100);

/// Creates a minimize button with the standard Windows icon.
pub fn minimize_button<Message: 'static + Send + Clone>(
    hook: &mut HookManager<Message>,
) -> Element<Message> {
    let mut instance = hook.instance(w_id!());
    let hover_anim = instance.use_hook(|| Rc::new(RefCell::new(Animation::new(false).duration(ANIMATION_DURATION)))).clone();
    let press_anim = instance.use_hook(|| Rc::new(RefCell::new(Animation::new(false).duration(ANIMATION_DURATION)))).clone();

    let normal_color = Color::TRANSPARENT;
    let hover_color = Color::from(0xFFFFFF1A); // ~10% white overlay
    let pressed_color = Color::from(0xFFFFFF33); // ~20% white overlay

    Button::new()
        .clear()
        .with_bg_color(Color::TRANSPARENT)
        .with_adjust_style(move |state, _focused, shell, mut style| {
            let now = Instant::now();
            hover_anim.borrow_mut().update(matches!(state, ButtonState::Hover | ButtonState::Pressed));
            press_anim.borrow_mut().update(matches!(state, ButtonState::Pressed));

            // Interpolate from normal -> hover
            let base_color = hover_anim.borrow().interpolate_using(
                shell,
                |hovered| if hovered { hover_color } else { normal_color },
                now,
            );
            // Then from hover -> pressed
            let final_color = press_anim.borrow().interpolate_using(
                shell,
                |pressed| if pressed { pressed_color } else { base_color },
                now,
            );

            style.background_color = Some(final_color);
            style
        })
        .with_click_handler(|_, shell| {
            shell.dispatch_task(minimize_window());
        })
        .as_element(
            MAGIC_ID_TITLEBAR_MINIMIZE,
            Element {
                width: Sizing::fixed(CAPTION_BUTTON_WIDTH),
                height: Sizing::fixed(CAPTION_BUTTON_HEIGHT),
                children: vec![center(
                    Text::new("\u{e921}" /* ChromeMinimize */)
                        .with_font_size(10.0)
                        .with_font_family(get_font())
                        .with_color(ColorChoice::CurrentColor)
                        .as_element()
                )],
                ..Default::default()
            },
        )
}

/// Creates a maximize/restore button with the standard Windows icon.
pub fn maximize_button<Message: 'static + Send + Clone>(
    hook: &mut HookManager<Message>,
) -> Element<Message> {
    let mut instance = hook.instance(w_id!());
    let hover_anim = instance.use_hook(|| Rc::new(RefCell::new(Animation::new(false).duration(ANIMATION_DURATION)))).clone();
    let press_anim = instance.use_hook(|| Rc::new(RefCell::new(Animation::new(false).duration(ANIMATION_DURATION)))).clone();

    let normal_color = Color::TRANSPARENT;
    let hover_color = Color::from(0xFFFFFF1A); // ~10% white overlay
    let pressed_color = Color::from(0xFFFFFF33); // ~20% white overlay

    Button::new()
        .clear()
        .with_bg_color(Color::TRANSPARENT)
        .with_adjust_style(move |state, _focused, shell, mut style| {
            let now = Instant::now();
            hover_anim.borrow_mut().update(matches!(state, ButtonState::Hover | ButtonState::Pressed));
            press_anim.borrow_mut().update(matches!(state, ButtonState::Pressed));

            let base_color = hover_anim.borrow().interpolate_using(
                shell,
                |hovered| if hovered { hover_color } else { normal_color },
                now,
            );
            let final_color = press_anim.borrow().interpolate_using(
                shell,
                |pressed| if pressed { pressed_color } else { base_color },
                now,
            );

            style.background_color = Some(final_color);
            style
        })
        .with_click_handler(|_, shell| {
            shell.dispatch_task(toggle_maximize_window());
        })
        .as_element(
            MAGIC_ID_TITLEBAR_MAXIMIZE,
            Element {
                width: Sizing::fixed(CAPTION_BUTTON_WIDTH),
                height: Sizing::fixed(CAPTION_BUTTON_HEIGHT),
                children: vec![center(
                    Text::new(if hook.window_zoomed { "\u{e923}" /* ChromeRestore */ } else { "\u{e922}" /* ChromeMaximize */ })
                        .with_font_size(10.0)
                        .with_font_family(get_font())
                        .with_color(ColorChoice::CurrentColor)
                        .as_element()
                )],
                ..Default::default()
            },
        )
}

/// Creates a close button with the standard Windows X icon and red hover.
pub fn close_button<Message: 'static + Send + Clone>(
    hook: &mut HookManager<Message>,
) -> Element<Message> {
    let mut instance = hook.instance(w_id!());
    let hover_anim = instance.use_hook(|| Rc::new(RefCell::new(Animation::new(false).duration(ANIMATION_DURATION)))).clone();
    let press_anim = instance.use_hook(|| Rc::new(RefCell::new(Animation::new(false).duration(ANIMATION_DURATION)))).clone();

    let normal_color = Color::TRANSPARENT;
    let hover_color = Color::from(0xE81123FF); // Windows red
    let pressed_color = Color::from(0xF1707AFF); // Lighter red when pressed

    Button::new()
        .clear()
        .with_bg_color(Color::TRANSPARENT)
        .with_adjust_style(move |state, _focused, shell, mut style| {
            let now = Instant::now();
            hover_anim.borrow_mut().update(matches!(state, ButtonState::Hover | ButtonState::Pressed));
            press_anim.borrow_mut().update(matches!(state, ButtonState::Pressed));

            let base_color = hover_anim.borrow().interpolate_using(
                shell,
                |hovered| if hovered { hover_color } else { normal_color },
                now,
            );
            let final_color = press_anim.borrow().interpolate_using(
                shell,
                |pressed| if pressed { pressed_color } else { base_color },
                now,
            );

            style.background_color = Some(final_color);
            style
        })
        .with_click_handler(|_, shell| {
            shell.dispatch_task(close_window());
        })
        .as_element(
            MAGIC_ID_TITLEBAR_CLOSE,
            Element {
                width: Sizing::fixed(CAPTION_BUTTON_WIDTH),
                height: Sizing::fixed(CAPTION_BUTTON_HEIGHT),
                children: vec![center(
                    Text::new("\u{e8bb}" /* ChromeClose */)
                        .with_font_size(10.0)
                        .with_font_family(get_font())
                        .with_color(ColorChoice::CurrentColor)
                        .as_element()
                )],
                ..Default::default()
            },
        )
}
