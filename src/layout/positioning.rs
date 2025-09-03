use crate::{
    layout::{
        BorrowedUITree, ScrollStateManager,
        model::{Direction, HorizontalAlignment, UIKey, VerticalAlignment},
        visitors,
    },
    runtime::scroll::ScrollPosition,
};

pub fn position_elements<Message>(
    ui_tree: BorrowedUITree<'_, Message>,
    root: UIKey,
    scroll_state_manager: &mut ScrollStateManager,
) {
    // Clone root id_map to allow easy lookup while mutably borrowing slots in closures
    let root_id_map = ui_tree.slots[root].id_map.clone();

    visitors::visit_deferring_bfs(
        ui_tree,
        root,
        |ui_tree, key, parent| {
            let slots = &ui_tree.slots;
            let floating = slots[key].floating.clone();
            if floating.is_none() {
                return false;
            }

            // Determine anchor element: default to parent; if anchor_id present, try lookup on root id_map
            let mut anchor = parent;
            if let Some(f) = &floating {
                if let Some(anchor_id) = &f.anchor_id {
                    if let Some(found) = root_id_map.get(anchor_id).copied() {
                        anchor = Some(found);
                    } else {
                        // anchor = Some(key);
                        panic!("Floating element with unknown anchor_id {}", anchor_id);
                    }
                }
            }

            if let Some(a) = anchor {
                !slots[a].__positioned
            } else {
                // No anchor found -> do not defer
                false
            }
        },
        |ui_tree, key, parent| {
            let slots = &mut ui_tree.slots;

            // Scroll handling
            let mut scroll_x: f32 = 0.0;
            let mut scroll_y: f32 = 0.0;

            let element_id_opt = slots[key].id;
            if let (Some(scroll_cfg), Some(element_id)) =
                (slots[key].scroll.clone(), element_id_opt)
            {
                let max_scroll_x =
                    (slots[key].computed_content_width - slots[key].computed_width).max(0.0);
                let max_scroll_y =
                    (slots[key].computed_content_height - slots[key].computed_height).max(0.0);

                let sticky_bottom = scroll_cfg.sticky_bottom.unwrap_or(false);
                let sticky_right = scroll_cfg.sticky_right.unwrap_or(false);

                let (prev_w, prev_h) =
                    scroll_state_manager.get_previous_content_dimensions(element_id);
                let current = scroll_state_manager.get_scroll_position(element_id);

                if sticky_bottom
                    && slots[key].computed_content_height > prev_h
                    && scroll_state_manager.was_at_bottom(element_id)
                {
                    scroll_y = max_scroll_y;
                    scroll_state_manager.set_scroll_position(
                        element_id,
                        ScrollPosition {
                            x: current.x,
                            y: scroll_y,
                        },
                    );
                } else {
                    scroll_y = current.y;
                }

                if sticky_right
                    && slots[key].computed_content_width > prev_w
                    && scroll_state_manager.was_at_right(element_id)
                {
                    scroll_x = max_scroll_x;
                    scroll_state_manager.set_scroll_position(
                        element_id,
                        ScrollPosition {
                            x: scroll_x,
                            y: scroll_y,
                        },
                    );
                } else {
                    scroll_x = current.x;
                }

                if let Some(user_max_x) = scroll_cfg.max_horizontal_scroll {
                    scroll_x = scroll_x.min(max_scroll_x.min(user_max_x));
                } else {
                    scroll_x = scroll_x.min(max_scroll_x);
                }

                if let Some(user_max_y) = scroll_cfg.max_vertical_scroll {
                    scroll_y = scroll_y.min(max_scroll_y.min(user_max_y));
                } else {
                    scroll_y = scroll_y.min(max_scroll_y);
                }

                if scroll_x < 0.0 {
                    scroll_x = 0.0;
                }
                if scroll_y < 0.0 {
                    scroll_y = 0.0;
                }

                scroll_state_manager.update_scroll_metadata(
                    element_id,
                    ScrollPosition {
                        x: scroll_x,
                        y: scroll_y,
                    },
                    max_scroll_x,
                    max_scroll_y,
                    slots[key].computed_content_width,
                    slots[key].computed_content_height,
                );
            }

            // Floating anchoring
            if let Some(floating) = slots[key].floating.clone() {
                // Determine anchor element (default to parent)
                let mut anchor = parent;
                if let Some(anchor_id) = floating.anchor_id.as_ref() {
                    if let Some(found) = root_id_map.get(anchor_id).copied() {
                        anchor = Some(found);
                    } else {
                        anchor = Some(key);
                    }
                }

                if let Some(anchor_key) = anchor {
                    let anchor_x = slots[anchor_key].x;
                    let anchor_y = slots[anchor_key].y;
                    let anchor_w = slots[anchor_key].computed_width;
                    let anchor_h = slots[anchor_key].computed_height;

                    let mut anchor_point_x = anchor_x;
                    let mut anchor_point_y = anchor_y;

                    if let Some(anchor_align) = floating.anchor.as_ref() {
                        if let Some(ax) = anchor_align.x {
                            match ax {
                                HorizontalAlignment::Left => {}
                                HorizontalAlignment::Center => {
                                    anchor_point_x = anchor_x + anchor_w / 2.0
                                }
                                HorizontalAlignment::Right => anchor_point_x = anchor_x + anchor_w,
                            }
                        }
                        if let Some(ay) = anchor_align.y {
                            match ay {
                                VerticalAlignment::Top => {}
                                VerticalAlignment::Center => {
                                    anchor_point_y = anchor_y + anchor_h / 2.0
                                }
                                VerticalAlignment::Bottom => anchor_point_y = anchor_y + anchor_h,
                            }
                        }
                    }

                    let element_w = slots[key].computed_width;
                    let element_h = slots[key].computed_height;

                    let mut align_offset_x = 0.0;
                    let mut align_offset_y = 0.0;

                    if let Some(align) = floating.align.as_ref() {
                        if let Some(ax) = align.x {
                            match ax {
                                HorizontalAlignment::Left => {}
                                HorizontalAlignment::Center => align_offset_x = -element_w / 2.0,
                                HorizontalAlignment::Right => align_offset_x = -element_w,
                            }
                        }
                        if let Some(ay) = align.y {
                            match ay {
                                VerticalAlignment::Top => {}
                                VerticalAlignment::Center => align_offset_y = -element_h / 2.0,
                                VerticalAlignment::Bottom => align_offset_y = -element_h,
                            }
                        }
                    }

                    let offset_x = floating.offset.as_ref().and_then(|o| o.x).unwrap_or(0.0);
                    let offset_y = floating.offset.as_ref().and_then(|o| o.y).unwrap_or(0.0);

                    slots[key].x = anchor_point_x + align_offset_x + offset_x;
                    slots[key].y = anchor_point_y + align_offset_y + offset_y;
                } else {
                    // No anchor present; nothing to do
                }
            }

            // Position non-floating children according to direction and alignment
            let children_keys = slots[key].children.clone();
            let non_floating: Vec<UIKey> = children_keys
                .into_iter()
                .filter(|&c| slots[c].floating.is_none())
                .collect();

            let (scroll_h, scroll_v) = if let Some(cfg) = slots[key].scroll.as_ref() {
                (
                    cfg.horizontal.unwrap_or(false),
                    cfg.vertical.unwrap_or(false),
                )
            } else {
                (false, false)
            };

            let scroll_offset_x = if scroll_h { scroll_x } else { 0.0 };
            let scroll_offset_y = if scroll_v { scroll_y } else { 0.0 };

            let content_start_x = slots[key].x + slots[key].padding.left - scroll_offset_x;
            let content_start_y = slots[key].y + slots[key].padding.top - scroll_offset_y;
            let available_width =
                slots[key].computed_width - slots[key].padding.left - slots[key].padding.right;
            let available_height =
                slots[key].computed_height - slots[key].padding.top - slots[key].padding.bottom;

            match slots[key].direction {
                Direction::LeftToRight => {
                    let mut total_children_width = 0.0;
                    if !non_floating.is_empty() {
                        for c in &non_floating {
                            total_children_width += slots[*c].computed_width;
                        }
                        total_children_width +=
                            slots[key].child_gap * (non_floating.len() as f32 - 1.0);
                    }

                    let remaining_width = available_width - total_children_width;
                    let mut start_x = content_start_x;
                    if remaining_width > 0.0 {
                        match slots[key].horizontal_alignment {
                            HorizontalAlignment::Left => {}
                            HorizontalAlignment::Center => start_x += remaining_width / 2.0,
                            HorizontalAlignment::Right => start_x += remaining_width,
                        }
                    }

                    let mut current_x = start_x;
                    for c in non_floating {
                        slots[c].x = current_x;
                        match slots[c].vertical_alignment {
                            VerticalAlignment::Top => {
                                slots[c].y = content_start_y;
                            }
                            VerticalAlignment::Center => {
                                slots[c].y = content_start_y
                                    + (available_height - slots[c].computed_height).max(0.0) / 2.0;
                            }
                            VerticalAlignment::Bottom => {
                                slots[c].y = content_start_y
                                    + (available_height - slots[c].computed_height).max(0.0);
                            }
                        }

                        current_x += slots[c].computed_width + slots[key].child_gap;
                    }
                }
                Direction::TopToBottom => {
                    let mut total_children_height = 0.0;
                    if !non_floating.is_empty() {
                        for c in &non_floating {
                            total_children_height += slots[*c].computed_height;
                        }
                        total_children_height +=
                            slots[key].child_gap * (non_floating.len() as f32 - 1.0);
                    }

                    let remaining_height = available_height - total_children_height;
                    let mut start_y = content_start_y;
                    if remaining_height > 0.0 {
                        match slots[key].vertical_alignment {
                            VerticalAlignment::Top => {}
                            VerticalAlignment::Center => start_y += remaining_height / 2.0,
                            VerticalAlignment::Bottom => start_y += remaining_height,
                        }
                    }

                    let mut current_y = start_y;
                    for c in non_floating {
                        match slots[c].horizontal_alignment {
                            HorizontalAlignment::Left => {
                                slots[c].x = content_start_x;
                            }
                            HorizontalAlignment::Center => {
                                slots[c].x = content_start_x
                                    + (available_width - slots[c].computed_width).max(0.0) / 2.0;
                            }
                            HorizontalAlignment::Right => {
                                slots[c].x = content_start_x
                                    + (available_width - slots[c].computed_width).max(0.0);
                            }
                        }

                        slots[c].y = current_y;
                        current_y += slots[c].computed_height + slots[key].child_gap;
                    }
                }
            }

            // Mark as positioned
            slots[key].__positioned = true;
        },
    );
}
