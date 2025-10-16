use std::{cell::RefCell, collections::HashMap, time::Instant};

use slotmap::SlotMap;
use string_interner::{StringInterner, backend::StringBackend};

use crate::{
    HookState, Shell,
    gfx::{
        RectDIP, command_recorder::CommandRecorder, draw_commands::DrawCommandList,
    },
    layout::{
        model::{Axis, ElementStyle, UIElement, UIKey},
        positioning::position_elements,
        visitors::VisitFrame,
    },
    runtime::scroll::{
        DEFAULT_SCROLLBAR_THUMB_COLOR, DEFAULT_SCROLLBAR_THUMB_RADIUS,
        DEFAULT_SCROLLBAR_TRACK_COLOR, DEFAULT_SCROLLBAR_TRACK_RADIUS, ScrollStateManager,
        ScrollbarGeom, compute_scrollbar_geom,
    },
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
                    // Draw outset drop shadows first (behind the element)
                    for shadow in &style.drop_shadows {
                        if !shadow.inset {
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

                    // Draw inset drop shadows after background (on top of the element)
                    for shadow in &style.drop_shadows {
                        if shadow.inset {
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
                        &mut shell.borrow_mut(),
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
