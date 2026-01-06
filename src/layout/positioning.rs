use crate::{
    layout::{
        BorrowedUITree, ScrollStateManager,
        model::{Alignment, Direction, UIKey},
        visitors,
    },
    runtime::scroll::ScrollPosition,
};

pub fn position_elements<Message>(
    ui_tree: BorrowedUITree<'_, Message>,
    root: UIKey,
    scroll_state_manager: &mut ScrollStateManager,
    dip_scale: f32,
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
            if let Some(f) = &floating
                && let Some(anchor_id) = &f.anchor_id
            {
                if let Some(found) = root_id_map.get(anchor_id).copied() {
                    anchor = Some(found);
                } else {
                    // anchor = Some(key);
                    panic!("Floating element with unknown anchor_id {anchor_id}");
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
                                Alignment::Start => {}
                                Alignment::Center => anchor_point_x = anchor_x + anchor_w / 2.0,
                                Alignment::End => anchor_point_x = anchor_x + anchor_w,
                            }
                        }
                        if let Some(ay) = anchor_align.y {
                            match ay {
                                Alignment::Start => {}
                                Alignment::Center => anchor_point_y = anchor_y + anchor_h / 2.0,
                                Alignment::End => anchor_point_y = anchor_y + anchor_h,
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
                                Alignment::Start => {}
                                Alignment::Center => align_offset_x = -element_w / 2.0,
                                Alignment::End => align_offset_x = -element_w,
                            }
                        }
                        if let Some(ay) = align.y {
                            match ay {
                                Alignment::Start => {}
                                Alignment::Center => align_offset_y = -element_h / 2.0,
                                Alignment::End => align_offset_y = -element_h,
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

            // First, snap if snap is enabled
            if slots[key].snap {
                // Round to nearest dip_scale

                slots[key].x = (slots[key].x / dip_scale).round() * dip_scale;
                slots[key].y = (slots[key].y / dip_scale).round() * dip_scale;
                slots[key].computed_width =
                    (slots[key].computed_width / dip_scale).round() * dip_scale;
                slots[key].computed_height =
                    (slots[key].computed_height / dip_scale).round() * dip_scale;
            }

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
                    slots[key].computed_width,
                    slots[key].computed_height,
                );
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
                Direction::ZStack => {
                    // All children are stacked at the same position, respecting individual alignment
                    // ZStack: axis_align_self for horizontal, cross_align_self for vertical
                    for c in non_floating {
                        // Horizontal position: use child's axis_align_self, fallback to parent's axis_align_items
                        let horizontal_align = slots[c]
                            .axis_align_self
                            .unwrap_or(slots[key].axis_align_items);
                        match horizontal_align {
                            Alignment::Start => {
                                slots[c].x = content_start_x;
                            }
                            Alignment::Center => {
                                slots[c].x = content_start_x
                                    + (available_width - slots[c].computed_width).max(0.0) / 2.0;
                            }
                            Alignment::End => {
                                slots[c].x = content_start_x
                                    + (available_width - slots[c].computed_width).max(0.0);
                            }
                        }

                        // Vertical position: use child's cross_align_self, fallback to parent's cross_align_items
                        let vertical_align = slots[c]
                            .cross_align_self
                            .unwrap_or(slots[key].cross_align_items);
                        match vertical_align {
                            Alignment::Start => {
                                slots[c].y = content_start_y;
                            }
                            Alignment::Center => {
                                slots[c].y = content_start_y
                                    + (available_height - slots[c].computed_height).max(0.0) / 2.0;
                            }
                            Alignment::End => {
                                slots[c].y = content_start_y
                                    + (available_height - slots[c].computed_height).max(0.0);
                            }
                        }
                    }
                }
                Direction::LeftToRight => {
                    // Check if this is a wrapping layout with breaks
                    if slots[key].wrap {
                        // Handle wrapping layout with multiple rows
                        let breaks = slots[key].wrap_breaks.clone();

                        // First pass: calculate total height of all rows
                        let mut total_rows_height = 0.0;
                        let mut row_count = 0;
                        let mut start_idx = 0;
                        for &break_idx in breaks.iter().chain(std::iter::once(&non_floating.len()))
                        {
                            let row_children: Vec<UIKey> =
                                non_floating[start_idx..break_idx].to_vec();
                            if !row_children.is_empty() {
                                let row_height = row_children
                                    .iter()
                                    .fold(0.0_f32, |acc, &c| acc.max(slots[c].computed_height));
                                total_rows_height += row_height;
                                row_count += 1;
                            }
                            start_idx = break_idx;
                        }
                        // Add gaps between rows
                        if row_count > 1 {
                            total_rows_height += slots[key].child_gap * (row_count as f32 - 1.0);
                        }

                        // Apply cross_align_content to determine starting Y position
                        let remaining_height = available_height - total_rows_height;
                        let mut current_y = content_start_y;
                        if remaining_height > 0.0 {
                            match slots[key].cross_align_content {
                                Alignment::Start => {}
                                Alignment::Center => current_y += remaining_height / 2.0,
                                Alignment::End => current_y += remaining_height,
                            }
                        }

                        // Second pass: position rows
                        start_idx = 0;
                        for &break_idx in breaks.iter().chain(std::iter::once(&non_floating.len()))
                        {
                            let row_children: Vec<UIKey> =
                                non_floating[start_idx..break_idx].to_vec();
                            if row_children.is_empty() {
                                start_idx = break_idx;
                                continue;
                            }

                            // Calculate total width for this row
                            let mut total_row_width = 0.0;
                            for &c in &row_children {
                                total_row_width += slots[c].computed_width;
                            }
                            total_row_width +=
                                slots[key].child_gap * (row_children.len() as f32 - 1.0);

                            // Calculate row height (max of children in this row)
                            let row_height = row_children
                                .iter()
                                .fold(0.0_f32, |acc, &c| acc.max(slots[c].computed_height));

                            // Calculate starting X position for this row based on axis_align_content
                            let remaining_width = available_width - total_row_width;
                            let mut start_x = content_start_x;
                            if remaining_width > 0.0 {
                                match slots[key].axis_align_content {
                                    Alignment::Start => {}
                                    Alignment::Center => start_x += remaining_width / 2.0,
                                    Alignment::End => start_x += remaining_width,
                                }
                            }

                            // Position children in this row
                            let mut current_x = start_x;
                            for &c in &row_children {
                                slots[c].x = current_x;

                                // Vertical alignment: use child's cross_align_self, fallback to parent's cross_align_items
                                let vertical_align = slots[c]
                                    .cross_align_self
                                    .unwrap_or(slots[key].cross_align_items);
                                match vertical_align {
                                    Alignment::Start => {
                                        slots[c].y = current_y;
                                    }
                                    Alignment::Center => {
                                        slots[c].y = current_y
                                            + (row_height - slots[c].computed_height).max(0.0)
                                                / 2.0;
                                    }
                                    Alignment::End => {
                                        slots[c].y = current_y
                                            + (row_height - slots[c].computed_height).max(0.0);
                                    }
                                }

                                current_x += slots[c].computed_width + slots[key].child_gap;
                            }

                            // Move to next row
                            current_y += row_height;
                            if break_idx < non_floating.len() {
                                current_y += slots[key].child_gap; // Row gap
                            }

                            start_idx = break_idx;
                        }
                    } else {
                        // Original single-row LeftToRight behavior
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
                            match slots[key].axis_align_content {
                                Alignment::Start => {}
                                Alignment::Center => start_x += remaining_width / 2.0,
                                Alignment::End => start_x += remaining_width,
                            }
                        }

                        let mut current_x = start_x;
                        for c in non_floating {
                            slots[c].x = current_x;
                            // Vertical alignment: use child's cross_align_self, fallback to parent's cross_align_items
                            let vertical_align = slots[c]
                                .cross_align_self
                                .unwrap_or(slots[key].cross_align_items);
                            match vertical_align {
                                Alignment::Start => {
                                    slots[c].y = content_start_y;
                                }
                                Alignment::Center => {
                                    slots[c].y = content_start_y
                                        + (available_height - slots[c].computed_height).max(0.0)
                                            / 2.0;
                                }
                                Alignment::End => {
                                    slots[c].y = content_start_y
                                        + (available_height - slots[c].computed_height).max(0.0);
                                }
                            }

                            current_x += slots[c].computed_width + slots[key].child_gap;
                        }
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
                        match slots[key].axis_align_content {
                            Alignment::Start => {}
                            Alignment::Center => start_y += remaining_height / 2.0,
                            Alignment::End => start_y += remaining_height,
                        }
                    }

                    let mut current_y = start_y;
                    for c in non_floating {
                        // Horizontal alignment: use child's cross_align_self, fallback to parent's cross_align_items
                        let horizontal_align = slots[c]
                            .cross_align_self
                            .unwrap_or(slots[key].cross_align_items);
                        match horizontal_align {
                            Alignment::Start => {
                                slots[c].x = content_start_x;
                            }
                            Alignment::Center => {
                                slots[c].x = content_start_x
                                    + (available_width - slots[c].computed_width).max(0.0) / 2.0;
                            }
                            Alignment::End => {
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
