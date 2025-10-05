use std::{cell::RefCell, collections::HashMap, time::Instant};

use slotmap::SlotMap;
use string_interner::{StringInterner, backend::StringBackend};

use crate::{
    HookState, Shell,
    gfx::{
        RectDIP, color::Color, command_recorder::CommandRecorder, draw_commands::DrawCommandList,
    },
    layout::{
        model::{Axis, BorderRadius, ElementStyle, ScrollBarSize, UIElement, UIKey},
        positioning::position_elements,
        visitors::VisitFrame,
    },
    runtime::scroll::ScrollStateManager,
    widgets::{Instance, PaintOwnership},
};

pub mod helpers;
pub mod model;

mod float;
pub mod visitors;

mod fit_along_axis;
mod grow_and_shrink_along_axis;
mod positioning;

use fit_along_axis::fit_along_axis;
use grow_and_shrink_along_axis::grow_and_shrink_along_axis;

pub struct UIArenas {
    pub strings: StringInterner<StringBackend>,
}

pub struct OwnedUITree<Message> {
    pub root: UIKey,
    pub slots: SlotMap<UIKey, UIElement<Message>>,
    pub widget_state: HashMap<u64, Instance>,
    pub hook_state: HashMap<u64, HookState>,
    pub arenas: UIArenas,
}
pub type BorrowedUITree<'a, Message> = &'a mut OwnedUITree<Message>;

impl<Message> Default for OwnedUITree<Message> {
    fn default() -> Self {
        Self {
            root: UIKey::default(),
            slots: SlotMap::new(),
            widget_state: HashMap::new(),
            hook_state: HashMap::new(),
            arenas: UIArenas {
                strings: StringInterner::new(),
            },
        }
    }
}

#[allow(dead_code)]
fn propagate_inherited_properties<Message>(ui_tree: BorrowedUITree<'_, Message>, root: UIKey) {
    visitors::visit_bfs(ui_tree, root, |ui_tree, key, parent| {
        if let Some(parent_key) = parent {
            if ui_tree.slots[key].color.is_none() && ui_tree.slots[parent_key].color.is_some() {
                ui_tree.slots[key].color = ui_tree.slots[parent_key].color;
            }
        }

        if let Some(id) = ui_tree.slots[key].id {
            ui_tree.slots[root].id_map.insert(id, key);
        }

        // TODO: propagate Font
    });
}

pub fn layout<Message>(
    ui_tree: BorrowedUITree<'_, Message>,
    root: UIKey,
    scroll_state_manager: &mut ScrollStateManager,
    dip_scale: f32,
) {
    propagate_inherited_properties(ui_tree, root);

    fit_along_axis(ui_tree, root, Axis::X);
    grow_and_shrink_along_axis(ui_tree, root, Axis::X);

    fit_along_axis(ui_tree, root, Axis::Y);
    grow_and_shrink_along_axis(ui_tree, root, Axis::Y);

    position_elements(ui_tree, root, scroll_state_manager, dip_scale);
}

pub const DEFAULT_SCROLLBAR_TRACK_COLOR: Color = Color::from_hex(0x00000033);
pub const DEFAULT_SCROLLBAR_THUMB_COLOR: Color = Color::from_hex(0x00000055);
pub const DEFAULT_SCROLLBAR_SIZE: ScrollBarSize = ScrollBarSize::ThinThick(8.0, 16.0);
pub const DEFAULT_SCROLLBAR_MIN_THUMB_SIZE: f32 = 16.0;
pub const DEFAULT_SCROLLBAR_TRACK_RADIUS: BorderRadius = BorderRadius::all(0.0);
pub const DEFAULT_SCROLLBAR_THUMB_RADIUS: BorderRadius = BorderRadius::all(0.0);

pub fn paint<Message>(
    shell: &mut Shell<Message>,
    ui_tree: BorrowedUITree<'_, Message>,
    root: UIKey,
) -> DrawCommandList {
    // Generate commands first
    generate_paint_commands(shell, ui_tree, root)
}

pub fn generate_paint_commands<Message>(
    shell: &mut Shell<Message>,
    ui_tree: BorrowedUITree<'_, Message>,
    root: UIKey,
) -> DrawCommandList {
    let recorder = CommandRecorder::new();
    let now = Instant::now();

    // TODO: Modify visitor to allow handing this around
    // rather than unnecessarily creating a RefCell<>
    let recorder = RefCell::new(recorder);
    let shell = RefCell::new(shell);

    {
        // Track current z-index for deferred rendering
        let current_z_index = RefCell::new(0i32);

        visitors::visit_deferring_dfs(
            ui_tree,
            root,
            |ui_tree, key, _parent| {
                // Defer if this element's z-index is greater than current z-index
                let element_z_index = ui_tree.slots[key].z_index.unwrap_or(0);
                let current = *current_z_index.borrow();
                element_z_index > current
            },
            |ui_tree, key, parent| {
                let mut recorder = recorder.borrow_mut();

                let inherited_color = if let Some(parent_key) = parent {
                    ui_tree.slots[parent_key].color
                } else {
                    None
                };

                let element = &mut ui_tree.slots[key];
                let x = element.x;
                let y = element.y;
                let width = element.computed_width;
                let height = element.computed_height;

                let has_scroll_x =
                    matches!(element.scroll.as_ref(), Some(s) if s.horizontal.is_some());
                let has_scroll_y =
                    matches!(element.scroll.as_ref(), Some(s) if s.vertical.is_some());

                let bounds = element.bounds();
                let mut style = ElementStyle::from(&*element);
                if let Some(color) = inherited_color {
                    style.color.get_or_insert(color);
                }

                let (style, ownership) = if let Some(widget) = element.content.as_mut() {
                    let state = ui_tree.widget_state.get_mut(&element.id.unwrap()).unwrap();
                    (widget.adjust_style(state, style), widget.paint_ownership())
                } else {
                    (style, PaintOwnership::Contents)
                };

                // Back-propagate color
                if let Some(color) = style.color {
                    element.color = Some(color);
                }

                if let Some(opacity) = element.opacity
                    && opacity < 1.0
                {
                    recorder.push_layer(opacity);
                }

                if matches!(ownership, PaintOwnership::Contents) {
                    // Draw drop shadow first (behind the element)
                    if let Some(shadow) = &style.drop_shadow {
                        let element_rect = RectDIP {
                            x,
                            y,
                            width,
                            height,
                        };

                        recorder.draw_blurred_shadow(
                            &element_rect,
                            shadow,
                            style.border_radius.as_ref(),
                        );
                    }

                    if let Some(color) = style.background_color {
                        let element_rect = RectDIP {
                            x,
                            y,
                            width,
                            height,
                        };

                        if let Some(border_radius) = &style.border_radius {
                            // Use rounded rectangle rendering
                            recorder.fill_rounded_rectangle(&element_rect, border_radius, color);
                        } else {
                            // Use regular rectangle rendering
                            recorder.fill_rectangle(&element_rect, color);
                        }
                    }

                    if let Some(border) = &style.border {
                        let x = element.x;
                        let y = element.y;
                        let width = element.computed_width;
                        let height = element.computed_height;
                        let element_rect = RectDIP {
                            x,
                            y,
                            width,
                            height,
                        };
                        recorder.draw_border(&element_rect, style.border_radius.as_ref(), border);
                    }
                }

                if has_scroll_x || has_scroll_y {
                    let clip_rect = RectDIP {
                        x,
                        y,
                        width,
                        height,
                    };

                    if let Some(border_radius) = &element.border_radius {
                        // Use layer with rounded rectangle geometry for clipping
                        recorder.push_rounded_clip(&clip_rect, border_radius);
                    } else {
                        // Use regular axis-aligned clipping for non-rounded elements
                        recorder.push_axis_aligned_clip(&clip_rect);
                    }
                }

                if let Some(widget) = element.content.as_mut() {
                    let state = ui_tree.widget_state.get_mut(&element.id.unwrap()).unwrap();
                    widget.paint(
                        &ui_tree.arenas,
                        state,
                        &shell.borrow(),
                        &mut recorder,
                        style,
                        bounds,
                        now,
                    );
                }
            },
            Some(
                &mut |OwnedUITree { slots, .. }: BorrowedUITree<'_, Message>, key, _parent| {
                    let mut recorder = recorder.borrow_mut();
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

                        let track_radius = scroll_config
                            .scrollbar_track_radius
                            .unwrap_or(DEFAULT_SCROLLBAR_TRACK_RADIUS);
                        let thumb_radius = scroll_config
                            .scrollbar_thumb_radius
                            .unwrap_or(DEFAULT_SCROLLBAR_THUMB_RADIUS);

                        if let Some(ScrollbarGeom {
                            track_rect,
                            thumb_rect,
                            ..
                        }) = compute_scrollbar_geom(&mut shell.borrow_mut(), element, Axis::X)
                        {
                            let needs_clip = !track_radius.contains(&thumb_radius);

                            if needs_clip {
                                recorder.push_rounded_clip(&track_rect, &track_radius);
                            }

                            recorder.fill_rounded_rectangle(
                                &track_rect,
                                &track_radius,
                                scrollbar_track_color,
                            );
                            recorder.fill_rounded_rectangle(
                                &thumb_rect,
                                &thumb_radius,
                                scrollbar_thumb_color,
                            );

                            if needs_clip {
                                recorder.pop_rounded_clip();
                            }
                        }

                        if let Some(ScrollbarGeom {
                            track_rect,
                            thumb_rect,
                            ..
                        }) = compute_scrollbar_geom(&mut shell.borrow_mut(), element, Axis::Y)
                        {
                            let needs_clip = !track_radius.contains(&thumb_radius);

                            if needs_clip {
                                recorder.push_rounded_clip(&track_rect, &track_radius);
                            }

                            recorder.fill_rounded_rectangle(
                                &track_rect,
                                &track_radius,
                                scrollbar_track_color,
                            );
                            recorder.fill_rounded_rectangle(
                                &thumb_rect,
                                &thumb_radius,
                                scrollbar_thumb_color,
                            );

                            if needs_clip {
                                recorder.pop_rounded_clip();
                            }
                        }

                        if element.border_radius.is_some() {
                            // Use layer with rounded rectangle geometry for clipping
                            recorder.pop_rounded_clip();
                        } else {
                            // Use regular axis-aligned clipping for non-rounded elements
                            recorder.pop_axis_aligned_clip();
                        }
                    }

                    if let Some(opacity) = element.opacity
                        && opacity < 1.0
                    {
                        recorder.pop_layer();
                    }
                },
            ),
            Some(
                |ui_tree: BorrowedUITree<'_, Message>, deferred_frames: &[VisitFrame]| {
                    // After each pass, find the next lowest z-index and update current_z_index
                    if !deferred_frames.is_empty() {
                        let mut next_z_index = i32::MAX;
                        for frame in deferred_frames {
                            let element_z_index = ui_tree.slots[frame.element].z_index.unwrap_or(0);
                            if element_z_index < next_z_index {
                                next_z_index = element_z_index;
                            }
                        }
                        *current_z_index.borrow_mut() = next_z_index;
                    }
                },
            ),
        );
    }

    // Extract commands from the recorder

    recorder.into_inner().take_commands()
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
    shell: &mut Shell<Message>,
    element: &UIElement<Message>,
    axis: Axis,
) -> Option<ScrollbarGeom> {
    let has_scroll_x = matches!(element.scroll.as_ref(), Some(s) if s.horizontal.is_some());
    let has_scroll_y = matches!(element.scroll.as_ref(), Some(s) if s.vertical.is_some());
    let id = element.id?;
    let sc = element.scroll.as_ref()?;

    let scroll_metadata = shell.scroll_state_manager.get_scroll_metadata(id);

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
            let scrollbar_size = scroll_metadata.animation.1.interpolate(
                shell,
                scrollbar_size.thin(),
                scrollbar_size.thick(),
                Instant::now(),
            );

            let scrollbar_min_thumb_size = sc
                .scrollbar_min_thumb_size
                .unwrap_or(DEFAULT_SCROLLBAR_MIN_THUMB_SIZE);

            let scroll_y = scroll_metadata.position.y;
            let effective_scroll_y = scroll_y.clamp(0.0, max_scroll_y);
            let progress = if max_scroll_y > 0.0 {
                effective_scroll_y / max_scroll_y
            } else {
                0.0
            };
            let visible_ratio = (height / content_height).min(1.0);

            let safe_area_padding = sc.safe_area_padding.unwrap_or_default();
            let x = element.x + safe_area_padding.left;
            let y = element.y + safe_area_padding.top;
            let safe_width = width - safe_area_padding.left - safe_area_padding.right;
            let safe_height = height - safe_area_padding.top - safe_area_padding.bottom;

            let thumb_len = (safe_height * visible_ratio).max(scrollbar_min_thumb_size);
            let range = (safe_height - thumb_len).max(0.0);

            let track_rect = RectDIP {
                x: x + safe_width - scrollbar_size,
                y,
                width: scrollbar_size,
                height: safe_height,
            };
            let thumb_rect = RectDIP {
                x: x + safe_width - scrollbar_size,
                y: y + range * progress,
                width: scrollbar_size,
                height: thumb_len,
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
            let scrollbar_size = scroll_metadata.animation.0.interpolate(
                shell,
                scrollbar_size.thin(),
                scrollbar_size.thick(),
                Instant::now(),
            );

            let scrollbar_min_thumb_size = sc
                .scrollbar_min_thumb_size
                .unwrap_or(DEFAULT_SCROLLBAR_MIN_THUMB_SIZE);

            let scroll_x = scroll_metadata.position.x;
            let effective_scroll_x = scroll_x.clamp(0.0, max_scroll_x);
            let progress = if max_scroll_x > 0.0 {
                effective_scroll_x / max_scroll_x
            } else {
                0.0
            };
            let visible_ratio = (width / content_width).min(1.0);

            let safe_area_padding = sc.safe_area_padding.unwrap_or_default();
            let x = element.x + safe_area_padding.left;
            let y = element.y + safe_area_padding.top;
            let safe_width = width - safe_area_padding.left - safe_area_padding.right;
            let safe_height = height - safe_area_padding.top - safe_area_padding.bottom;

            let thumb_len = (safe_width * visible_ratio).max(scrollbar_min_thumb_size);
            let range = (safe_width - thumb_len).max(0.0);

            let track_rect = RectDIP {
                x,
                y: y + safe_height - scrollbar_size,
                width: safe_width,
                height: scrollbar_size,
            };
            let thumb_rect = RectDIP {
                x: x + range * progress,
                y: y + safe_height - scrollbar_size,
                width: thumb_len,
                height: scrollbar_size,
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
