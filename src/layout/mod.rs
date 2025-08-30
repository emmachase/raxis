use std::{collections::HashMap, time::Instant};

use slotmap::SlotMap;

use crate::{
    HookState, Shell,
    gfx::{RectDIP, command_recorder::CommandRecorder, draw_commands::DrawCommandList},
    layout::{
        model::{Axis, ElementContent, UIElement, UIKey},
        positioning::position_elements,
    },
    runtime::scroll::ScrollStateManager,
    widgets::Instance,
};

pub mod model;

mod float;
pub mod visitors;

mod fit_along_axis;
mod grow_and_shrink_along_axis;
mod positioning;

use fit_along_axis::fit_along_axis;
use grow_and_shrink_along_axis::grow_and_shrink_along_axis;

pub struct OwnedUITree<Message> {
    pub root: UIKey,
    pub slots: SlotMap<UIKey, UIElement<Message>>,
    pub widget_state: HashMap<u64, Instance>,
    pub hook_state: HashMap<u64, HookState>,
}
pub type BorrowedUITree<'a, Message> = &'a mut OwnedUITree<Message>;

impl<Message> Default for OwnedUITree<Message> {
    fn default() -> Self {
        Self {
            root: UIKey::default(),
            slots: SlotMap::new(),
            widget_state: HashMap::new(),
            hook_state: HashMap::new(),
        }
    }
}

#[allow(dead_code)]
fn set_parent_references<Message>(ui_tree: BorrowedUITree<'_, Message>, root: UIKey) {
    visitors::visit_bfs(ui_tree, root, |ui_tree, key, parent| {
        ui_tree.slots[key].parent = parent;
    });
}

#[allow(dead_code)]
fn propagate_inherited_properties<Message>(ui_tree: BorrowedUITree<'_, Message>, root: UIKey) {
    visitors::visit_bfs(ui_tree, root, |ui_tree, key, parent| {
        if let Some(parent_key) = parent {
            if ui_tree.slots[key].color.is_none() && ui_tree.slots[parent_key].color.is_some() {
                ui_tree.slots[key].color = ui_tree.slots[parent_key].color;
            }
        }

        // TODO: propagate Font
    });
}

fn wrap_text<Message>(ui_tree: BorrowedUITree<'_, Message>, root: UIKey) {
    visitors::visit_bfs(ui_tree, root, |ui_tree, key, _parent| {
        if let Some(ElementContent::Text { layout, .. }) = ui_tree.slots[key].content.as_ref() {
            let element = &ui_tree.slots[key];

            let available_width =
                element.computed_width - element.padding.left - element.padding.right;

            unsafe {
                layout
                    .as_ref()
                    .unwrap()
                    .SetMaxWidth(available_width)
                    .unwrap();
            }
        }
    });
}

pub fn layout<Message>(
    ui_tree: BorrowedUITree<'_, Message>,
    root: UIKey,
    scroll_state_manager: &mut ScrollStateManager,
) {
    set_parent_references(ui_tree, root);
    propagate_inherited_properties(ui_tree, root);

    fit_along_axis(ui_tree, root, Axis::X);
    grow_and_shrink_along_axis(ui_tree, root, Axis::X);

    wrap_text(ui_tree, root);

    fit_along_axis(ui_tree, root, Axis::Y);
    grow_and_shrink_along_axis(ui_tree, root, Axis::Y);

    position_elements(ui_tree, root, scroll_state_manager);
}

pub const DEFAULT_SCROLLBAR_TRACK_COLOR: u32 = 0x00000033;
pub const DEFAULT_SCROLLBAR_THUMB_COLOR: u32 = 0x00000055;
pub const DEFAULT_SCROLLBAR_SIZE: f32 = 16.0;
pub const DEFAULT_SCROLLBAR_MIN_THUMB_SIZE: f32 = 16.0;

pub fn paint<Message>(
    shell: &Shell<Message>,
    ui_tree: BorrowedUITree<'_, Message>,
    root: UIKey,
    scroll_state_manager: &mut ScrollStateManager,
) -> DrawCommandList {
    // Generate commands first
    generate_paint_commands(shell, ui_tree, root, scroll_state_manager)
}

pub fn generate_paint_commands<Message>(
    shell: &Shell<Message>,
    ui_tree: BorrowedUITree<'_, Message>,
    root: UIKey,
    scroll_state_manager: &mut ScrollStateManager,
) -> DrawCommandList {
    let recorder = CommandRecorder::new();
    let now = Instant::now();

    // TODO: Modify visitor to allow handing this around
    // rather than unnecessarily creating a Rc<Refcell<>>
    let recorder = std::rc::Rc::new(std::cell::RefCell::new(recorder));

    {
        let recorder_clone = recorder.clone();

        visitors::visit_dfs(
            ui_tree,
            root,
            |ui_tree, key, _parent| {
                let mut recorder = recorder.borrow_mut();
                let element = &mut ui_tree.slots[key];
                let x = element.x;
                let y = element.y;
                let width = element.computed_width;
                let height = element.computed_height;

                let has_scroll_x =
                    matches!(element.scroll.as_ref(), Some(s) if s.horizontal.is_some());
                let has_scroll_y =
                    matches!(element.scroll.as_ref(), Some(s) if s.vertical.is_some());

                // Draw drop shadow first (behind the element)
                if let Some(shadow) = &element.drop_shadow {
                    let element_rect = RectDIP {
                        x_dip: x,
                        y_dip: y,
                        width_dip: width,
                        height_dip: height,
                    };

                    recorder.draw_blurred_shadow(
                        &element_rect,
                        shadow,
                        element.border_radius.as_ref(),
                    );
                }

                if let Some(color) = element.background_color {
                    let element_rect = RectDIP {
                        x_dip: x,
                        y_dip: y,
                        width_dip: width,
                        height_dip: height,
                    };

                    if let Some(border_radius) = &element.border_radius {
                        // Use rounded rectangle rendering
                        recorder.fill_rounded_rectangle(&element_rect, border_radius, color);
                    } else {
                        // Use regular rectangle rendering
                        recorder.fill_rectangle(&element_rect, color);
                    }
                }

                if has_scroll_x || has_scroll_y {
                    let clip_rect = RectDIP {
                        x_dip: x,
                        y_dip: y,
                        width_dip: width,
                        height_dip: height,
                    };

                    if let Some(border_radius) = &element.border_radius {
                        // Use layer with rounded rectangle geometry for clipping
                        recorder.push_rounded_clip(&clip_rect, border_radius);
                    } else {
                        // Use regular axis-aligned clipping for non-rounded elements
                        recorder.push_axis_aligned_clip(&clip_rect);
                    }
                }

                // if let Some(layout) = element.content.as_ref().and_then(|c| c.layout.as_ref()) {
                let bounds = element.bounds();
                if let Some(content) = element.content.as_mut() {
                    match content {
                        ElementContent::Text { layout, .. } => {
                            let color = element.color.unwrap_or(0x000000FF);

                            recorder.draw_text(
                                &bounds.content_box,
                                layout.as_ref().unwrap(),
                                color,
                            );
                        }
                        ElementContent::Widget(widget) => {
                            let state = ui_tree.widget_state.get_mut(&element.id.unwrap()).unwrap();
                            widget.paint(
                                state,
                                shell,
                                &mut recorder,
                                bounds,
                                now,
                                // 1.0 / 240.0, /* TODO: dt */
                            );
                        }
                    }
                }
            },
            Some(
                &mut |OwnedUITree { slots, .. }: BorrowedUITree<'_, Message>, key, _parent| {
                    let mut recorder = recorder_clone.borrow_mut();
                    let element = &slots[key];

                    let has_scroll_x =
                        matches!(element.scroll.as_ref(), Some(s) if s.horizontal.is_some());
                    let has_scroll_y =
                        matches!(element.scroll.as_ref(), Some(s) if s.vertical.is_some());

                    if has_scroll_x || has_scroll_y {
                        let scroll_config = element.scroll.as_ref().unwrap();
                        let scrollbar_track_color = scroll_config
                            .scrollbar_track_color
                            .unwrap_or(DEFAULT_SCROLLBAR_TRACK_COLOR);
                        let scrollbar_thumb_color = scroll_config
                            .scrollbar_thumb_color
                            .unwrap_or(DEFAULT_SCROLLBAR_THUMB_COLOR);

                        if let Some(ScrollbarGeom {
                            track_rect,
                            thumb_rect,
                            ..
                        }) = compute_scrollbar_geom(element, Axis::X, scroll_state_manager)
                        {
                            recorder.fill_rectangle(&track_rect, scrollbar_track_color);
                            recorder.fill_rectangle(&thumb_rect, scrollbar_thumb_color);
                        }

                        if let Some(ScrollbarGeom {
                            track_rect,
                            thumb_rect,
                            ..
                        }) = compute_scrollbar_geom(element, Axis::Y, scroll_state_manager)
                        {
                            recorder.fill_rectangle(&track_rect, scrollbar_track_color);
                            recorder.fill_rectangle(&thumb_rect, scrollbar_thumb_color);
                        }

                        if element.border_radius.is_some() {
                            // Use layer with rounded rectangle geometry for clipping
                            recorder.pop_rounded_clip();
                        } else {
                            // Use regular axis-aligned clipping for non-rounded elements
                            recorder.pop_axis_aligned_clip();
                        }
                    }

                    // Draw element border after content and scrollbars, and after popping clip
                    // so that Outset borders render outside the element bounds.
                    if let Some(border) = &element.border {
                        let x = element.x;
                        let y = element.y;
                        let width = element.computed_width;
                        let height = element.computed_height;
                        let element_rect = RectDIP {
                            x_dip: x,
                            y_dip: y,
                            width_dip: width,
                            height_dip: height,
                        };
                        recorder.draw_border(&element_rect, element.border_radius.as_ref(), border);
                    }
                },
            ),
        );
    }

    // Extract commands from the recorder

    std::rc::Rc::try_unwrap(recorder)
        .map_err(|_| "Failed to unwrap recorder")
        .unwrap()
        .into_inner()
        .take_commands()
}

// ===== Reusable scrollbar geometry helpers =====

#[derive(Clone, Copy, Debug)]
pub struct ScrollbarGeom {
    pub axis: Axis,
    pub track_rect: RectDIP,
    pub thumb_rect: RectDIP,
    // Run length along the scroll axis that the thumb can travel
    pub range: f32,
    // Track start coordinate along the scroll axis (x for X, y for Y)
    pub track_start: f32,
    // Maximum scroll value for this axis (content - viewport)
    pub max_scroll: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum ScrollDirection {
    Positive,
    Negative,
}

pub fn can_scroll_further<Message>(
    element: &UIElement<Message>,
    axis: Axis,
    direction: ScrollDirection,
    scroll_state_manager: &ScrollStateManager,
) -> bool {
    let id = match element.id {
        Some(id) => id,
        None => return false,
    };

    let scroll_position = scroll_state_manager.get_scroll_position(id);
    let (max_scroll_x, max_scroll_y) = (
        element.computed_content_width - element.computed_width,
        element.computed_content_height - element.computed_height,
    );

    match (axis, direction) {
        (Axis::X, ScrollDirection::Positive) => scroll_position.x < max_scroll_x,
        (Axis::X, ScrollDirection::Negative) => scroll_position.x > 0.0,
        (Axis::Y, ScrollDirection::Positive) => scroll_position.y < max_scroll_y,
        (Axis::Y, ScrollDirection::Negative) => scroll_position.y > 0.0,
    }
}

pub fn compute_scrollbar_geom<Message>(
    element: &UIElement<Message>,
    axis: Axis,
    scroll_state_manager: &ScrollStateManager,
) -> Option<ScrollbarGeom> {
    let has_scroll_x = matches!(element.scroll.as_ref(), Some(s) if s.horizontal.is_some());
    let has_scroll_y = matches!(element.scroll.as_ref(), Some(s) if s.vertical.is_some());
    let id = element.id?;
    let sc = element.scroll.as_ref()?;

    match axis {
        Axis::Y if has_scroll_y => {
            let width = element.computed_width;
            let height = element.computed_height;
            let content_height = element.computed_content_height;
            let max_scroll_y = (content_height - height).max(0.0);
            if content_height <= height {
                return None;
            }

            let scrollbar_size = sc.scrollbar_size.unwrap_or(DEFAULT_SCROLLBAR_SIZE);
            let scrollbar_min_thumb_size = sc
                .scrollbar_min_thumb_size
                .unwrap_or(DEFAULT_SCROLLBAR_MIN_THUMB_SIZE);

            let visible_ratio = (height / content_height).min(1.0);
            let thumb_len = (height * visible_ratio).max(scrollbar_min_thumb_size);
            let range = (height - thumb_len).max(0.0);

            let scroll_y = scroll_state_manager.get_scroll_position(id).y;
            let effective_scroll_y = scroll_y.clamp(0.0, max_scroll_y);
            let progress = if max_scroll_y > 0.0 {
                effective_scroll_y / max_scroll_y
            } else {
                0.0
            };

            let x = element.x;
            let y = element.y;
            let track_rect = RectDIP {
                x_dip: x + width - scrollbar_size,
                y_dip: y,
                width_dip: scrollbar_size,
                height_dip: height,
            };
            let thumb_rect = RectDIP {
                x_dip: x + width - scrollbar_size,
                y_dip: y + range * progress,
                width_dip: scrollbar_size,
                height_dip: thumb_len,
            };

            Some(ScrollbarGeom {
                axis,
                track_rect,
                thumb_rect,
                range,
                track_start: y,
                max_scroll: max_scroll_y,
            })
        }
        Axis::X if has_scroll_x => {
            let width = element.computed_width;
            let height = element.computed_height;
            let content_width = element.computed_content_width;
            let max_scroll_x = (content_width - width).max(0.0);
            if content_width <= width {
                return None;
            }

            let scrollbar_size = sc.scrollbar_size.unwrap_or(DEFAULT_SCROLLBAR_SIZE);
            let scrollbar_min_thumb_size = sc
                .scrollbar_min_thumb_size
                .unwrap_or(DEFAULT_SCROLLBAR_MIN_THUMB_SIZE);

            let visible_ratio = (width / content_width).min(1.0);
            let thumb_len = (width * visible_ratio).max(scrollbar_min_thumb_size);
            let range = (width - thumb_len).max(0.0);

            let scroll_x = scroll_state_manager.get_scroll_position(id).x;
            let effective_scroll_x = scroll_x.clamp(0.0, max_scroll_x);
            let progress = if max_scroll_x > 0.0 {
                effective_scroll_x / max_scroll_x
            } else {
                0.0
            };

            let x = element.x;
            let y = element.y;
            let track_rect = RectDIP {
                x_dip: x,
                y_dip: y + height - scrollbar_size,
                width_dip: width,
                height_dip: scrollbar_size,
            };
            let thumb_rect = RectDIP {
                x_dip: x + range * progress,
                y_dip: y + height - scrollbar_size,
                width_dip: thumb_len,
                height_dip: scrollbar_size,
            };

            Some(ScrollbarGeom {
                axis,
                track_rect,
                thumb_rect,
                range,
                track_start: x,
                max_scroll: max_scroll_x,
            })
        }
        _ => None,
    }
}
