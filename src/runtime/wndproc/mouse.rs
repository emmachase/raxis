use slotmap::DefaultKey;
use std::ops::DerefMut;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ScreenToClient};
use windows::Win32::System::SystemServices::MK_SHIFT;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, HTCLIENT, SPI_GETWHEELSCROLLLINES, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SystemParametersInfoW,
};

use crate::gfx::PointDIP;
use crate::layout::model::Axis;
use crate::runtime::input::MiddleMouseScrollState;
use crate::runtime::scroll::{
    ScrollDirection, ScrollPosition, can_scroll_further, compute_scrollbar_geom,
};
use crate::runtime::util::{get_modifiers, state_mut_from_hwnd, window_rect};
use crate::widgets::{Cursor, Event};
use crate::{RedrawRequest, Shell, dips_scale};

use super::super::LINE_HEIGHT;

/// Handle WM_LBUTTONDOWN
pub fn handle_lbuttondown<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    // Capture mouse & keyboard input
    let _ = unsafe { SetFocus(Some(hwnd)) };
    let _ = unsafe { SetCapture(hwnd) };

    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();

        // Extract mouse position in client pixels
        let xi = (lparam.0 & 0xFFFF) as i16 as i32;
        let yi = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
        let x_px = xi as f32;
        let y_px = yi as f32;
        let to_dip = dips_scale(hwnd);
        let x = x_px * to_dip;
        let y = y_px * to_dip;

        // First, check scrollbar thumb hit-testing
        if state.scroll_drag.is_none() {
            if let Some(drag) = state.hit_test_scrollbar_thumb(x, y, true) {
                state.scroll_drag = Some(drag);
                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
                return LRESULT(0);
            }
        }

        // Update multi-click tracking
        let click_count = state.mouse_state.update_click_count(POINT { x: xi, y: yi });

        let modifiers = get_modifiers();
        state.shell.dispatch_event(
            hwnd,
            &mut state.ui_tree,
            Event::MouseButtonDown {
                x,
                y,
                click_count,
                modifiers,
            },
        );

        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }
    LRESULT(0)
}

/// Handle WM_MOUSEMOVE
pub fn handle_mousemove<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();

        // Current mouse in pixels
        let xi = (lparam.0 & 0xFFFF) as i16 as i32;
        let yi = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
        let to_dip = dips_scale(hwnd);
        let x = (xi as f32) * to_dip;
        let y = (yi as f32) * to_dip;

        // Handle scrollbar dragging if active
        if let Some(drag) = state.scroll_drag {
            // Find element by id
            let mut found_key: Option<DefaultKey> = None;
            for k in state.ui_tree.slots.keys() {
                if state.ui_tree.slots[k].id == Some(drag.element_id) {
                    found_key = Some(k);
                    break;
                }
            }

            if let Some(k) = found_key {
                let el = &state.ui_tree.slots[k];
                let axis = drag.axis;
                if let Some(geom) = compute_scrollbar_geom(&mut state.shell, el, axis) {
                    let pos_along = match drag.axis {
                        Axis::Y => y,
                        Axis::X => x,
                    };
                    let rel =
                        (pos_along - geom.track_start - drag.grab_offset).clamp(0.0, geom.range);
                    let progress = if geom.range > 0.0 {
                        rel / geom.range
                    } else {
                        0.0
                    };
                    let new_scroll = progress * geom.max_scroll;
                    let cur = state
                        .shell
                        .scroll_state_manager
                        .get_scroll_position(drag.element_id);
                    match drag.axis {
                        Axis::Y => {
                            state.shell.scroll_state_manager.set_scroll_position(
                                drag.element_id,
                                ScrollPosition {
                                    x: cur.x,
                                    y: new_scroll,
                                },
                            );
                        }
                        Axis::X => {
                            state.shell.scroll_state_manager.set_scroll_position(
                                drag.element_id,
                                ScrollPosition {
                                    x: new_scroll,
                                    y: cur.y,
                                },
                            );
                        }
                    }
                    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
                }
            }
            return LRESULT(0);
        }

        // Handle middle mouse scrolling if active - update current mouse position
        if let Some(ref mut middle_scroll) = state.middle_mouse_scroll {
            // Update current mouse position
            middle_scroll.current_x = x;
            middle_scroll.current_y = y;

            // Request continuous redraws while scrolling
            state.shell.request_redraw(hwnd, RedrawRequest::Immediate);
            return LRESULT(0);
        }

        if let Some(drag) = state.hit_test_scrollbar_thumb(x, y, false) {
            state
                .shell
                .scroll_state_manager
                .set_active(drag.element_id, drag.axis);
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        } else if state.shell.scroll_state_manager.set_inactive() {
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        }

        // Continue manual drag (selection or preview drop position)
        state
            .shell
            .dispatch_event(hwnd, &mut state.ui_tree, Event::MouseMove { x, y });
    }
    LRESULT(0)
}

/// Handle WM_MOUSEWHEEL
pub fn handle_mousewheel<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let wheel_delta = (wparam.0 >> 16) as i16;
    let modifiers = (wparam.0 & 0xFFFF) as u16;
    let x = (lparam.0 & 0xFFFF) as i16 as i32 as f32;
    let y = (lparam.0 >> 16) as i16 as i32 as f32;

    let shift = (modifiers & MK_SHIFT.0 as u16) != 0;
    let axis = if shift { Axis::X } else { Axis::Y };

    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        let rect = window_rect(hwnd).unwrap();

        let to_dip = dips_scale(hwnd);
        let x_dip = (x - rect.left as f32) * to_dip;
        let y_dip = (y - rect.top as f32) * to_dip;
        let wheel_delta = -wheel_delta as f32 / 120.0;
        let modifiers = get_modifiers();
        state.shell.dispatch_event(
            hwnd,
            &mut state.ui_tree,
            Event::MouseWheel {
                x: x_dip,
                y: y_dip,
                wheel_delta,
                modifiers,
            },
        );

        if state.shell.capture_event(0) {
            // Use the same visibility logic as event dispatch:
            // 1. Find innermost element at position
            // 2. Walk up ancestry to find first scrollable element
            // This prevents scrolling through obscuring elements
            if let Some(innermost_key) =
                Shell::find_innermost_element_at(&mut state.ui_tree, x_dip, y_dip)
            {
                let ancestry = Shell::collect_ancestry(&mut state.ui_tree, innermost_key);

                // Walk up the ancestry from innermost to outermost
                for &key in &ancestry {
                    let element = &state.ui_tree.slots[key];

                    if element.scroll.is_some()
                        && let Some(element_id) = element.id
                    {
                        // Check if this element can scroll in the requested direction
                        if can_scroll_further(
                            element,
                            axis,
                            if wheel_delta > 0.0 {
                                ScrollDirection::Positive
                            } else {
                                ScrollDirection::Negative
                            },
                            &state.shell.scroll_state_manager,
                        ) {
                            let mut scroll_lines = 3;
                            let _ = unsafe {
                                SystemParametersInfoW(
                                    SPI_GETWHEELSCROLLLINES,
                                    0,
                                    Some(&mut scroll_lines as *mut i32 as *mut _),
                                    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                                )
                            };

                            let wheel_delta =
                                wheel_delta * LINE_HEIGHT as f32 * scroll_lines as f32;

                            let (delta_x, delta_y) = if axis == Axis::Y {
                                (0.0, wheel_delta)
                            } else {
                                (wheel_delta, 0.0)
                            };

                            // Get current scroll position (either from active animation or actual position)
                            let current_pos = state
                                .shell
                                .scroll_state_manager
                                .get_scroll_position(element_id);
                            let current_animated_pos = state
                                .smooth_scroll_manager
                                .get_current_position(element_id, current_pos);

                            // Use accumulate_scroll_delta for fast scrolling support
                            let delta = ScrollPosition {
                                x: delta_x,
                                y: delta_y,
                            };

                            state.smooth_scroll_manager.accumulate_scroll_delta(
                                element_id,
                                current_animated_pos,
                                delta,
                            );

                            state.shell.request_redraw(hwnd, RedrawRequest::Immediate);

                            break;
                        }
                    }
                }
            }
        }
    }
    LRESULT(0)
}

/// Handle WM_LBUTTONUP
pub fn handle_lbuttonup<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();

        // If we take scroll_drag, ignore the event as we consume it
        if state.scroll_drag.take().is_none() {
            let x_px = (lparam.0 & 0xFFFF) as i16 as i32 as f32;
            let y_px = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32 as f32;
            let to_dip = dips_scale(hwnd);
            let x = x_px * to_dip;
            let y = y_px * to_dip;

            let modifiers = get_modifiers();
            state.shell.dispatch_event(
                hwnd,
                &mut state.ui_tree,
                Event::MouseButtonUp {
                    x,
                    y,
                    click_count: state.mouse_state.click_count,
                    modifiers,
                },
            );
        }
    }

    // Release mouse capture
    let _ = unsafe { ReleaseCapture() };
    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };

    LRESULT(0)
}

/// Handle WM_MBUTTONDOWN
pub fn handle_mbuttondown<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    lparam: LPARAM,
) -> LRESULT {
    let _ = unsafe { SetFocus(Some(hwnd)) };
    let _ = unsafe { SetCapture(hwnd) };

    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();

        // Extract mouse position in client pixels
        let xi = (lparam.0 & 0xFFFF) as i16 as i32;
        let yi = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
        let to_dip = dips_scale(hwnd);
        let x = (xi as f32) * to_dip;
        let y = (yi as f32) * to_dip;

        // Find the scrollable element at this position
        if let Some(innermost_key) = Shell::find_innermost_element_at(&mut state.ui_tree, x, y) {
            let ancestry = Shell::collect_ancestry(&mut state.ui_tree, innermost_key);

            // Walk up the ancestry from innermost to outermost
            for &key in &ancestry {
                let element = &state.ui_tree.slots[key];

                if element.scroll.is_some()
                    && let Some(element_id) = element.id
                {
                    // Found a scrollable element, start middle mouse scroll
                    state.middle_mouse_scroll = Some(MiddleMouseScrollState {
                        element_id,
                        origin_x: x,
                        origin_y: y,
                        current_x: x,
                        current_y: y,
                    });
                    break;
                }
            }
        }
    }
    LRESULT(0)
}

/// Handle WM_MBUTTONUP
pub fn handle_mbuttonup<State: 'static, Message: 'static + Send + Clone>(hwnd: HWND) -> LRESULT {
    if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
        let state = state.deref_mut();
        state.middle_mouse_scroll = None;
    }

    // Release mouse capture
    let _ = unsafe { ReleaseCapture() };
    LRESULT(0)
}

/// Handle WM_SETCURSOR
pub fn handle_setcursor<State: 'static, Message: 'static + Send + Clone>(
    hwnd: HWND,
    lparam: LPARAM,
) -> Option<LRESULT> {
    // Set I-beam cursor when hovering over visible text bounds (in client area)
    let hit_test = (lparam.0 & 0xFFFF) as u32;
    if hit_test == HTCLIENT {
        if let Some(mut state) = state_mut_from_hwnd::<State, Message>(hwnd) {
            let state = state.deref_mut();
            // Get mouse in client pixels and convert to DIPs
            let mut pt = POINT { x: 0, y: 0 };
            let _ = unsafe { GetCursorPos(&mut pt) };
            let _ = unsafe { ScreenToClient(hwnd, &mut pt) };
            let to_dip = dips_scale(hwnd);
            let x_dip = (pt.x as f32) * to_dip;
            let y_dip = (pt.y as f32) * to_dip;
            let point = PointDIP { x: x_dip, y: y_dip };

            // Check if hovering over a scrollbar first
            if state
                .hit_test_scrollbar_thumb(x_dip, y_dip, false)
                .is_some()
            {
                Cursor::Arrow.set();
                return Some(LRESULT(1));
            }

            let mut cursor = None;

            if let Some(target_key) =
                Shell::find_innermost_element_at(&mut state.ui_tree, x_dip, y_dip)
            {
                let ancestry = Shell::collect_ancestry(&mut state.ui_tree, target_key);

                for element in ancestry {
                    let bounds = state.ui_tree.slots[element].bounds();

                    if let Some(id) = state.ui_tree.slots[element].id {
                        if point.within(bounds.border_box)
                            && Shell::is_point_visible_in_scroll_ancestors(
                                &mut state.ui_tree,
                                element,
                                point,
                            )
                        {
                            if let Some(instance) = state.ui_tree.widget_state.get(&id) {
                                if let Some(ref widget) = state.ui_tree.slots[element].content {
                                    cursor = widget.cursor(
                                        &state.ui_tree.arenas,
                                        instance,
                                        point,
                                        bounds,
                                    );
                                }
                            }
                        }
                    }

                    if cursor.is_some() {
                        break;
                    }
                }

                if let Some(cursor) = cursor {
                    cursor.set();
                    return Some(LRESULT(1));
                }
            }
        }
    }

    None
}
