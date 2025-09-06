use crate::layout::{
    BorrowedUITree,
    float::EpsFloatCmp,
    model::{Axis, Direction, Sizing, UIKey},
    visitors,
};

/// Calculate wrap breaks for LeftToRight wrapping layout
/// Returns the indices where new rows should start
fn calculate_wrap_breaks<Message>(
    ui_tree: BorrowedUITree<'_, Message>,
    children: &[UIKey],
    available_width: f32,
    child_gap: f32,
) -> Vec<usize> {
    let mut breaks = Vec::new();
    let mut current_width = 0.0;
    let mut is_first_in_row = true;

    for (i, &child) in children.iter().enumerate() {
        let child_width = ui_tree.slots[child].computed_width;
        let gap_width = if is_first_in_row { 0.0 } else { child_gap };

        if !is_first_in_row && current_width + gap_width + child_width > available_width {
            // Start a new row
            breaks.push(i);
            current_width = child_width;
        } else {
            current_width += gap_width + child_width;
        }

        is_first_in_row = false;
    }

    breaks
}

pub fn grow_and_shrink_along_axis<Message>(
    ui_tree: BorrowedUITree<'_, Message>,
    root: UIKey,
    axis: Axis,
) {
    let x_axis = matches!(axis, Axis::X);

    visitors::visit_bfs(ui_tree, root, |ui_tree, key, _parent| {
        // Use a macro to safely obtain multiple mutable reads/writes separated by operations
        macro_rules! element {
            () => {
                ui_tree.slots[key]
            };
        }

        let total_padding = if x_axis {
            element!().padding.left + element!().padding.right
        } else {
            element!().padding.top + element!().padding.bottom
        };

        // Remaining size is the available content size before child gaps and child sizes
        let mut remaining_size = if x_axis {
            element!().computed_width
        } else {
            element!().computed_height
        } - total_padding;

        // Determine axis directions
        let axis_direction = if x_axis {
            Direction::LeftToRight
        } else {
            Direction::TopToBottom
        };

        // Non-floating children keys
        let non_floating_children: Vec<UIKey> = element!()
            .children
            .iter()
            .copied()
            .filter(|c| ui_tree.slots[*c].floating.is_none())
            .collect();

        // Children which are resizable in this pass (i.e., not Percent sizing)
        let resizable_children: Vec<UIKey> = non_floating_children
            .iter()
            .copied()
            .filter(|c| {
                let s = if x_axis {
                    ui_tree.slots[*c].width
                } else {
                    ui_tree.slots[*c].height
                };
                !matches!(s, Sizing::Percent { .. })
            })
            .collect();

        // Child gap only applies when flowing along axis direction
        let size_child_gap = if element!().direction == axis_direction {
            element!().child_gap * (non_floating_children.len().saturating_sub(1)) as f32
        } else {
            0.0
        };

        // Store the full available size before subtracting child gaps (for wrap calculations)
        let full_available_size = remaining_size;
        remaining_size -= size_child_gap;

        let mut inner_content_size = total_padding + size_child_gap;

        // Precompute inner content size from resizable non-percent children
        for child in &resizable_children {
            let c = &ui_tree.slots[*child];
            if element!().direction == axis_direction {
                if x_axis {
                    inner_content_size += c.computed_width;
                } else {
                    inner_content_size += c.computed_height;
                }
            } else if x_axis {
                inner_content_size = inner_content_size.max(c.computed_width);
            } else {
                inner_content_size = inner_content_size.max(c.computed_height);
            }
        }

        let non_wrap_available_size = remaining_size;

        // Size Percent children
        let children_all = element!().children.clone();
        for child in &children_all {
            let sizing = if x_axis {
                ui_tree.slots[*child].width
            } else {
                ui_tree.slots[*child].height
            };
            if let Sizing::Percent { percent } = sizing {
                let assign = non_wrap_available_size * percent;
                if x_axis {
                    ui_tree.slots[*child].computed_width = assign;
                } else {
                    ui_tree.slots[*child].computed_height = assign;
                }

                // For non-floating children, subtract from remaining and add to content size
                if ui_tree.slots[*child].floating.is_none() {
                    remaining_size -= assign;
                    inner_content_size += assign;
                }
            }
        }

        // Set computed content size
        if x_axis {
            element!().computed_content_width = inner_content_size;
        } else {
            element!().computed_content_height = inner_content_size;
        }

        // Growable children are resizable with Grow sizing
        let mut growable_children: Vec<UIKey> = resizable_children
            .iter()
            .copied()
            .filter(|c| {
                matches!(
                    if x_axis {
                        ui_tree.slots[*c].width
                    } else {
                        ui_tree.slots[*c].height
                    },
                    Sizing::Grow { .. }
                )
            })
            .collect();

        if element!().direction == axis_direction {
            // Handle wrapping for LeftToRight direction
            let use_wrapping = x_axis && element!().wrap;

            if use_wrapping {
                // Calculate breaks and store them in the element
                let available_width = full_available_size;
                let breaks = calculate_wrap_breaks(
                    ui_tree,
                    &non_floating_children,
                    available_width,
                    element!().child_gap,
                );
                element!().wrap_breaks = breaks.clone();

                // Process each row individually
                let mut start_idx = 0;
                for &break_idx in breaks
                    .iter()
                    .chain(std::iter::once(&non_floating_children.len()))
                {
                    let row_children: Vec<UIKey> =
                        non_floating_children[start_idx..break_idx].to_vec();
                    if row_children.is_empty() {
                        start_idx = break_idx;
                        continue;
                    }

                    // Calculate row available width
                    let row_current_width: f32 = row_children
                        .iter()
                        .fold(0.0, |acc, &child| acc + ui_tree.slots[child].computed_width);
                    let row_gaps =
                        (row_children.len().saturating_sub(1)) as f32 * element!().child_gap;
                    let mut row_remaining_size = available_width - row_current_width - row_gaps;

                    // Get growable children in this row
                    let mut row_growable: Vec<UIKey> = row_children
                        .iter()
                        .copied()
                        .filter(|c| matches!(ui_tree.slots[*c].width, Sizing::Grow { .. }))
                        .collect();

                    // Row grow pass
                    while row_remaining_size.gt_eps(&0.01) && !row_growable.is_empty() {
                        // Same grow logic but for this row only
                        let mut smallest_size = f32::INFINITY;
                        let mut second_smallest_size = f32::INFINITY;
                        let mut smallest_max_for_group = f32::INFINITY;
                        let mut count_smallest = 0usize;

                        for ck in &row_growable {
                            let size = ui_tree.slots[*ck].computed_width;
                            if size.lt_eps(&smallest_size) {
                                second_smallest_size = smallest_size;
                                smallest_size = size;
                                let s = ui_tree.slots[*ck].width;
                                smallest_max_for_group = match s {
                                    Sizing::Grow { max, .. } => max,
                                    _ => f32::INFINITY,
                                };
                                count_smallest = 0;
                            }
                            if size.eq_eps(&smallest_size) {
                                let s = ui_tree.slots[*ck].width;
                                let max_this = match s {
                                    Sizing::Grow { max, .. } => max,
                                    _ => f32::INFINITY,
                                };
                                smallest_max_for_group = smallest_max_for_group.min(max_this);
                                count_smallest += 1;
                            }
                            if size.gt_eps(&smallest_size) && size.lt_eps(&second_smallest_size) {
                                second_smallest_size = size;
                            }
                        }

                        let mut size_to_add = (second_smallest_size - smallest_size)
                            .min(smallest_max_for_group - smallest_size);
                        size_to_add = size_to_add.min(row_remaining_size / count_smallest as f32);

                        // Add to all children in smallest group, remove those that hit max
                        let mut i = 0;
                        while i < row_growable.len() {
                            let ck = row_growable[i];
                            let size = ui_tree.slots[ck].computed_width;
                            if size.eq_eps(&smallest_size) {
                                ui_tree.slots[ck].computed_width += size_to_add;
                                row_remaining_size -= size_to_add;

                                // Remove if reached max
                                let sizing = ui_tree.slots[ck].width;
                                let max_allowed = match sizing {
                                    Sizing::Grow { max, .. } => max,
                                    _ => f32::INFINITY,
                                };
                                let new_size = ui_tree.slots[ck].computed_width;
                                if new_size.eq_eps(&max_allowed) {
                                    row_growable.remove(i);
                                    continue;
                                }
                            }
                            i += 1;
                            if !row_remaining_size.gt_eps(&0.0) {
                                break;
                            }
                        }
                    }

                    // Row shrink pass (if scroll not enabled)
                    let scroll_enabled = element!()
                        .scroll
                        .as_ref()
                        .map(|s| s.horizontal.unwrap_or(false))
                        .unwrap_or(false);

                    if !scroll_enabled && row_remaining_size.lt_eps(&-0.01) {
                        let mut row_shrinkable: Vec<UIKey> = row_children
                            .iter()
                            .copied()
                            .filter(|c| {
                                let e = &ui_tree.slots[*c];
                                let size = e.computed_width;
                                let min_allowed = e.min_width;
                                size.gt_eps(&min_allowed)
                            })
                            .collect();

                        while row_remaining_size.lt_eps(&-0.01) && !row_shrinkable.is_empty() {
                            let mut largest_size = -f32::INFINITY;
                            let mut second_largest_size = -f32::INFINITY;
                            let mut largest_min_for_group = -f32::INFINITY;
                            let mut count_largest = 0usize;

                            for ck in &row_shrinkable {
                                let c = &ui_tree.slots[*ck];
                                let size = c.computed_width;
                                if size.gt_eps(&largest_size) {
                                    second_largest_size = largest_size;
                                    largest_size = size;
                                    largest_min_for_group = c.min_width;
                                    count_largest = 0;
                                }
                                if size.eq_eps(&largest_size) {
                                    let min_this = c.min_width;
                                    largest_min_for_group = largest_min_for_group.max(min_this);
                                    count_largest += 1;
                                }
                                if size.lt_eps(&largest_size) && size.gt_eps(&second_largest_size) {
                                    second_largest_size = size;
                                }
                            }

                            let mut size_to_sub = (largest_size - second_largest_size)
                                .min(largest_size - largest_min_for_group);
                            size_to_sub =
                                size_to_sub.min((-row_remaining_size) / count_largest as f32);

                            let mut i = 0;
                            while i < row_shrinkable.len() {
                                let ck = row_shrinkable[i];
                                let size = ui_tree.slots[ck].computed_width;
                                if size.eq_eps(&largest_size) {
                                    ui_tree.slots[ck].computed_width -= size_to_sub;
                                    row_remaining_size += size_to_sub;

                                    // Remove if reached min
                                    let min_allowed = ui_tree.slots[ck].min_width;
                                    let new_size = ui_tree.slots[ck].computed_width;
                                    if new_size.eq_eps(&min_allowed) {
                                        row_shrinkable.remove(i);
                                        continue;
                                    }
                                }
                                i += 1;
                                if !row_remaining_size.lt_eps(&0.0) {
                                    break;
                                }
                            }
                        }
                    }

                    start_idx = break_idx;
                }
            } else {
                // Non-wrapping behavior
                // Subtract current sizes of resizable children
                remaining_size -= resizable_children.iter().fold(0.0_f32, |acc, ckey| {
                    let c = &ui_tree.slots[*ckey];
                    if x_axis {
                        acc + c.computed_width
                    } else {
                        acc + c.computed_height
                    }
                });

                // Grow pass
                while remaining_size.gt_eps(&0.01) && !growable_children.is_empty() {
                    // Find smallest group among growable children
                    let mut smallest_size = f32::INFINITY;
                    let mut second_smallest_size = f32::INFINITY;
                    let mut smallest_max_for_group = f32::INFINITY;
                    let mut count_smallest = 0usize;

                    for ck in &growable_children {
                        let c = &ui_tree.slots[*ck];
                        let size = if x_axis {
                            c.computed_width
                        } else {
                            c.computed_height
                        };
                        if size.lt_eps(&smallest_size) {
                            second_smallest_size = smallest_size;
                            smallest_size = size;
                            // reset group info
                            let s = if x_axis { c.width } else { c.height };
                            smallest_max_for_group = match s {
                                Sizing::Grow { max, .. } => max,
                                _ => f32::INFINITY,
                            };
                            count_smallest = 0;
                        }
                        if size.eq_eps(&smallest_size) {
                            let s = if x_axis { c.width } else { c.height };
                            let max_this = match s {
                                Sizing::Grow { max, .. } => max,
                                _ => f32::INFINITY,
                            };
                            smallest_max_for_group = smallest_max_for_group.min(max_this);
                            count_smallest += 1;
                        }
                        if size.gt_eps(&smallest_size) && size.lt_eps(&second_smallest_size) {
                            second_smallest_size = size;
                        }
                    }

                    let mut size_to_add = (second_smallest_size - smallest_size)
                        .min(smallest_max_for_group - smallest_size);
                    size_to_add = size_to_add.min(remaining_size / count_smallest as f32);

                    // Add to all children in smallest group, remove those that hit max
                    let mut i = 0;
                    while i < growable_children.len() {
                        let ck = growable_children[i];
                        let size = if x_axis {
                            ui_tree.slots[ck].computed_width
                        } else {
                            ui_tree.slots[ck].computed_height
                        };
                        if size.eq_eps(&smallest_size) {
                            if x_axis {
                                ui_tree.slots[ck].computed_width += size_to_add;
                            } else {
                                ui_tree.slots[ck].computed_height += size_to_add;
                            }
                            remaining_size -= size_to_add;

                            // Remove if reached max
                            let sizing = if x_axis {
                                ui_tree.slots[ck].width
                            } else {
                                ui_tree.slots[ck].height
                            };
                            let max_allowed = match sizing {
                                Sizing::Grow { max, .. } => max,
                                _ => f32::INFINITY,
                            };
                            let new_size = if x_axis {
                                ui_tree.slots[ck].computed_width
                            } else {
                                ui_tree.slots[ck].computed_height
                            };
                            if new_size.eq_eps(&max_allowed) {
                                growable_children.remove(i);
                                continue;
                            }
                        }
                        i += 1;
                        if !remaining_size.gt_eps(&0.0) {
                            break;
                        }
                    }
                }

                // If scroll enabled along axis, skip shrink
                let scroll_enabled = element!()
                    .scroll
                    .as_ref()
                    .map(|s| {
                        if x_axis {
                            s.horizontal.unwrap_or(false)
                        } else {
                            s.vertical.unwrap_or(false)
                        }
                    })
                    .unwrap_or(false);
                if scroll_enabled {
                    return;
                }

                // Shrink pass
                let mut shrinkable_children: Vec<UIKey> = resizable_children
                    .iter()
                    .copied()
                    .filter(|c| {
                        let e = &ui_tree.slots[*c];
                        let size = if x_axis {
                            e.computed_width
                        } else {
                            e.computed_height
                        };
                        let min_allowed = if x_axis { e.min_width } else { e.min_height };
                        size.gt_eps(&min_allowed)
                    })
                    .collect();

                while remaining_size.lt_eps(&-0.01) && !shrinkable_children.is_empty() {
                    let mut largest_size = -f32::INFINITY;
                    let mut second_largest_size = -f32::INFINITY;
                    let mut largest_min_for_group = -f32::INFINITY;
                    let mut count_largest = 0usize;

                    for ck in &shrinkable_children {
                        let c = &ui_tree.slots[*ck];
                        let size = if x_axis {
                            c.computed_width
                        } else {
                            c.computed_height
                        };
                        if size.gt_eps(&largest_size) {
                            second_largest_size = largest_size;
                            largest_size = size;
                            largest_min_for_group = if x_axis { c.min_width } else { c.min_height };
                            count_largest = 0;
                        }
                        if size.eq_eps(&largest_size) {
                            let min_this = if x_axis { c.min_width } else { c.min_height };
                            largest_min_for_group = largest_min_for_group.max(min_this);
                            count_largest += 1;
                        }
                        if size.lt_eps(&largest_size) && size.gt_eps(&second_largest_size) {
                            second_largest_size = size;
                        }
                    }

                    let mut size_to_sub = (largest_size - second_largest_size)
                        .min(largest_size - largest_min_for_group);
                    size_to_sub = size_to_sub.min((-remaining_size) / count_largest as f32);

                    let mut i = 0;
                    while i < shrinkable_children.len() {
                        let ck = shrinkable_children[i];
                        let size = if x_axis {
                            ui_tree.slots[ck].computed_width
                        } else {
                            ui_tree.slots[ck].computed_height
                        };
                        if size.eq_eps(&largest_size) {
                            if x_axis {
                                ui_tree.slots[ck].computed_width -= size_to_sub;
                            } else {
                                ui_tree.slots[ck].computed_height -= size_to_sub;
                            }
                            remaining_size += size_to_sub;

                            // Remove if reached min
                            let min_allowed = if x_axis {
                                ui_tree.slots[ck].min_width
                            } else {
                                ui_tree.slots[ck].min_height
                            };
                            let new_size = if x_axis {
                                ui_tree.slots[ck].computed_width
                            } else {
                                ui_tree.slots[ck].computed_height
                            };
                            if new_size.eq_eps(&min_allowed) {
                                shrinkable_children.remove(i);
                                continue;
                            }
                        }
                        i += 1;
                        if !remaining_size.lt_eps(&0.0) {
                            break;
                        }
                    }
                }
            }
        } else {
            // Cross-axis behavior
            // Check if we need to handle wrapping breaks for off-axis
            let has_breaks = !x_axis
                && element!().direction == Direction::LeftToRight
                && element!().wrap
                && !element!().wrap_breaks.is_empty();

            if has_breaks {
                // For wrapping LeftToRight when calculating Y-axis (height),
                // we need to process each row for off-axis grow/shrink
                let breaks = element!().wrap_breaks.clone();
                let mut start_idx = 0;

                for &break_idx in breaks
                    .iter()
                    .chain(std::iter::once(&non_floating_children.len()))
                {
                    let row_children: Vec<UIKey> =
                        non_floating_children[start_idx..break_idx].to_vec();
                    if row_children.is_empty() {
                        start_idx = break_idx;
                        continue;
                    }

                    // Calculate the maximum height needed for this row
                    let max_row_height = row_children.iter().fold(0.0_f32, |acc, &child| {
                        acc.max(ui_tree.slots[child].computed_height)
                    });

                    // Get growable children in this row for Y-axis
                    let row_growable: Vec<UIKey> = row_children
                        .iter()
                        .copied()
                        .filter(|c| matches!(ui_tree.slots[*c].height, Sizing::Grow { .. }))
                        .collect();

                    // Grow all growable children in this row to the max height
                    for ck in &row_growable {
                        let current_height = ui_tree.slots[*ck].computed_height;
                        if current_height.lt_eps(&max_row_height) {
                            let max_allowed = match ui_tree.slots[*ck].height {
                                Sizing::Grow { max, .. } => max,
                                _ => f32::INFINITY,
                            };
                            let new_height = max_row_height.min(max_allowed);
                            ui_tree.slots[*ck].computed_height = new_height;
                        }
                    }

                    start_idx = break_idx;
                }
            } else {
                // Original cross-axis behavior for non-wrapping layouts
                let mut grow_to_size = non_wrap_available_size;
                let scroll_enabled = element!()
                    .scroll
                    .as_ref()
                    .map(|s| {
                        if x_axis {
                            s.horizontal.unwrap_or(false)
                        } else {
                            s.vertical.unwrap_or(false)
                        }
                    })
                    .unwrap_or(false);
                if scroll_enabled {
                    grow_to_size = inner_content_size.max(non_wrap_available_size);
                }

                // Grow
                for ck in &growable_children {
                    let size = if x_axis {
                        ui_tree.slots[*ck].computed_width
                    } else {
                        ui_tree.slots[*ck].computed_height
                    };
                    if size.lt_eps(&grow_to_size) {
                        // cap by max
                        let max_allowed = match if x_axis {
                            ui_tree.slots[*ck].width
                        } else {
                            ui_tree.slots[*ck].height
                        } {
                            Sizing::Grow { max, .. } => max,
                            _ => f32::INFINITY,
                        };
                        let new_size = grow_to_size.min(max_allowed);
                        if x_axis {
                            ui_tree.slots[*ck].computed_width = new_size;
                        } else {
                            ui_tree.slots[*ck].computed_height = new_size;
                        }
                    }
                }

                // If scroll enabled, don't shrink
                if scroll_enabled {
                    return;
                }

                // Shrink
                let shrinkable_children: Vec<UIKey> = resizable_children
                    .iter()
                    .copied()
                    .filter(|c| {
                        let e = &ui_tree.slots[*c];
                        let size = if x_axis {
                            e.computed_width
                        } else {
                            e.computed_height
                        };
                        let min_allowed = if x_axis { e.min_width } else { e.min_height };
                        size.gt_eps(&min_allowed) && size.gt_eps(&non_wrap_available_size)
                    })
                    .collect();

                for ck in shrinkable_children {
                    let size = if x_axis {
                        ui_tree.slots[ck].computed_width
                    } else {
                        ui_tree.slots[ck].computed_height
                    };
                    if size.gt_eps(&non_wrap_available_size) {
                        let min_allowed = if x_axis {
                            ui_tree.slots[ck].min_width
                        } else {
                            ui_tree.slots[ck].min_height
                        };
                        let new_size = non_wrap_available_size.max(min_allowed);
                        if x_axis {
                            ui_tree.slots[ck].computed_width = new_size;
                        } else {
                            ui_tree.slots[ck].computed_height = new_size;
                        }
                    }
                }
            }
        }
    });
}
