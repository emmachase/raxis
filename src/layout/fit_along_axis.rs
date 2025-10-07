use crate::{
    layout::{
        BorrowedUITree,
        model::{Axis, Direction, Sizing, UIElement, UIKey},
        visitors,
    },
    widgets::limit_response,
};

pub fn fit_along_axis<Message>(ui_tree: BorrowedUITree<'_, Message>, root: UIKey, axis: Axis) {
    // Helper to check scroll flag for an axis
    fn is_scroll_enabled<Message>(el: &UIElement<Message>, axis: Axis) -> bool {
        el.scroll
            .as_ref()
            .map(|s| match axis {
                Axis::X => s.horizontal.unwrap_or(false),
                Axis::Y => s.vertical.unwrap_or(false),
            })
            .unwrap_or(false)
    }

    let x_axis = matches!(axis, Axis::X);

    visitors::visit_reverse_bfs(ui_tree, root, |ui_tree, key, _parent| {
        let element = &ui_tree.slots[key];
        let axis_padding = if x_axis {
            element.padding.left + element.padding.right
        } else {
            element.padding.top + element.padding.bottom
        };
        let cross_axis_padding = if x_axis {
            element.padding.top + element.padding.bottom
        } else {
            element.padding.left + element.padding.right
        };

        // Mutable borrow after we compute padding
        // let element = &slots[key];
        macro_rules! element {
            () => {
                ui_tree.slots[key]
            };
        }

        // Container sizing
        let axis_direction = if x_axis {
            Direction::LeftToRight
        } else {
            Direction::TopToBottom
        };
        let off_axis_direction = if x_axis {
            Direction::TopToBottom
        } else {
            Direction::LeftToRight
        };

        // Filter out floating children
        let non_floating_children: Vec<UIKey> = element!()
            .children
            .iter()
            .copied()
            .filter(|child| ui_tree.slots[*child].floating.is_none())
            .collect();

        // For ZStack, always use max of children (like cross-axis)
        if element!().direction == Direction::ZStack {
            // Cross-axis sizing: max of child sizes + padding
            let (children_max_size, children_max_min_size) = if non_floating_children.is_empty()
            {
                (0.0_f32, 0.0_f32)
            } else {
                non_floating_children
                    .iter()
                    .fold((0.0_f32, 0.0_f32), |acc, child| {
                        let c = &ui_tree.slots[*child];
                        if x_axis {
                            (acc.0.max(c.computed_width), acc.1.max(c.min_width))
                        } else {
                            (acc.0.max(c.computed_height), acc.1.max(c.min_height))
                        }
                    })
            };

            if x_axis {
                element!().computed_width = children_max_size + axis_padding;
                if !is_scroll_enabled(&element!(), Axis::X) {
                    element!().min_width = children_max_min_size + axis_padding;
                }
            } else {
                element!().computed_height = children_max_size + axis_padding;
                if !is_scroll_enabled(&element!(), Axis::Y) {
                    element!().min_height = children_max_min_size + axis_padding;
                }
            }
        } else if element!().direction == axis_direction {
            // Check if wrapping is enabled for LeftToRight direction
            let use_wrapping = x_axis && element!().wrap;

            if use_wrapping {
                // For On-axis sizing with wrapping LeftToRight: use max child size (like TopToBottom)
                let (children_max_size, children_max_min_size) = if non_floating_children.is_empty()
                {
                    (0.0_f32, 0.0_f32)
                } else {
                    non_floating_children
                        .iter()
                        .fold((0.0_f32, 0.0_f32), |acc, child| {
                            let c = &ui_tree.slots[*child];
                            (acc.0.max(c.computed_width), acc.1.max(c.min_width))
                        })
                };

                element!().computed_width = children_max_size + axis_padding;
                if !is_scroll_enabled(&element!(), Axis::X) {
                    element!().min_width = children_max_min_size + axis_padding;
                }
            } else {
                // On-axis sizing: sum of child sizes + gaps + padding
                let (children_size_sum, children_min_size_sum) = non_floating_children.iter().fold(
                    (0.0_f32, 0.0_f32),
                    |(acc_size, acc_min), child| {
                        let c = &ui_tree.slots[*child];
                        if x_axis {
                            (acc_size + c.computed_width, acc_min + c.min_width)
                        } else {
                            (acc_size + c.computed_height, acc_min + c.min_height)
                        }
                    },
                );

                let child_gap_sum =
                    (non_floating_children.len().saturating_sub(1)) as f32 * element!().child_gap;

                if x_axis {
                    element!().computed_width = children_size_sum + axis_padding + child_gap_sum;
                    if !is_scroll_enabled(&element!(), Axis::X) {
                        element!().min_width = children_min_size_sum + axis_padding + child_gap_sum;
                    }
                } else {
                    element!().computed_height = children_size_sum + axis_padding + child_gap_sum;
                    if !is_scroll_enabled(&element!(), Axis::Y) {
                        element!().min_height =
                            children_min_size_sum + axis_padding + child_gap_sum;
                    }
                }
            }
        } else if element!().direction == off_axis_direction {
            // Handle wrapping for LeftToRight when calculating Y-axis (height)
            if !x_axis && element!().direction == Direction::LeftToRight && element!().wrap {
                // For Cross-axis sizing with wrapping LeftToRight: calculate height as sum of row heights
                // We need to determine how many rows and the max height of each row
                let breaks = element!().wrap_breaks.clone();

                let mut row_heights = Vec::new();
                let mut start_idx = 0;

                for &break_idx in breaks
                    .iter()
                    .chain(std::iter::once(&non_floating_children.len()))
                {
                    let row_children = &non_floating_children[start_idx..break_idx];
                    if !row_children.is_empty() {
                        let max_height = row_children.iter().fold(0.0_f32, |acc, &child| {
                            acc.max(ui_tree.slots[child].computed_height)
                        });
                        row_heights.push(max_height);
                    }
                    start_idx = break_idx;
                }

                let total_height = row_heights.iter().sum::<f32>();
                let row_gaps = (row_heights.len().saturating_sub(1)) as f32 * element!().child_gap;

                element!().computed_height = total_height + axis_padding + row_gaps;

                if !is_scroll_enabled(&element!(), Axis::Y) {
                    // Min height is also the sum of row heights for wrapping
                    let min_row_heights: Vec<f32> = {
                        let mut min_heights = Vec::new();
                        let mut start_idx = 0;
                        for &break_idx in breaks
                            .iter()
                            .chain(std::iter::once(&non_floating_children.len()))
                        {
                            let row_children = &non_floating_children[start_idx..break_idx];
                            if !row_children.is_empty() {
                                let max_min_height =
                                    row_children.iter().fold(0.0_f32, |acc, &child| {
                                        acc.max(ui_tree.slots[child].min_height)
                                    });
                                min_heights.push(max_min_height);
                            }
                            start_idx = break_idx;
                        }
                        min_heights
                    };
                    let total_min_height = min_row_heights.iter().sum::<f32>();
                    let min_row_gaps =
                        (min_row_heights.len().saturating_sub(1)) as f32 * element!().child_gap;
                    element!().min_height = total_min_height + axis_padding + min_row_gaps;
                }
            } else {
                // Cross-axis sizing: max of child sizes + padding
                let (children_max_size, children_max_min_size) = if non_floating_children.is_empty()
                {
                    (0.0_f32, 0.0_f32)
                } else {
                    non_floating_children
                        .iter()
                        .fold((0.0_f32, 0.0_f32), |acc, child| {
                            let c = &ui_tree.slots[*child];
                            if x_axis {
                                (acc.0.max(c.computed_width), acc.1.max(c.min_width))
                            } else {
                                (acc.0.max(c.computed_height), acc.1.max(c.min_height))
                            }
                        })
                };

                if x_axis {
                    element!().computed_width = children_max_size + axis_padding;
                    if !is_scroll_enabled(&element!(), Axis::X) {
                        element!().min_width = children_max_min_size + axis_padding;
                    }
                } else {
                    element!().computed_height = children_max_size + axis_padding;
                    if !is_scroll_enabled(&element!(), Axis::Y) {
                        element!().min_height = children_max_min_size + axis_padding;
                    }
                }
            }
        }

        // Apply widget limits as additional constraints if this is a widget
        if let Some((widget, instance)) = element!().content.as_ref().and_then(|widget| {
            element!().id.and_then(|id| {
                ui_tree
                    .widget_state
                    .get_mut(&id)
                    .map(|instance| (widget, instance))
            })
        }) {
            if x_axis {
                let limit_response::SizingForX {
                    min_width,
                    preferred_width,
                } = widget.limits_x(&ui_tree.arenas, instance);

                // Apply widget limits as constraints to the computed container size
                element!().computed_width = element!()
                    .computed_width
                    .max(preferred_width + axis_padding);
                element!().min_width = element!().min_width.max(min_width + axis_padding);
            } else {
                let limit_response::SizingForY {
                    min_height,
                    preferred_height,
                } = widget.limits_y(
                    &ui_tree.arenas,
                    instance,
                    element!().computed_width,
                    element!().computed_width - cross_axis_padding,
                );

                // Apply widget limits as constraints to the computed container size
                element!().computed_height = element!()
                    .computed_height
                    .max(preferred_height + axis_padding);
                element!().min_height = element!().min_height.max(min_height + axis_padding);
            }
        }

        // Clamp to sizing (unless Percent, which is deferred to grow/shrink)
        match (axis, element!().width, element!().height) {
            (Axis::X, Sizing::Percent { .. }, _) => {
                element!().computed_width = 0.0;
            }
            (Axis::X, sizing, _) => {
                let min = sizing.get_min();
                let max = sizing.get_max();
                element!().computed_width = element!().computed_width.clamp(min, max);
                element!().min_width = element!().min_width.clamp(min, max);
            }
            (Axis::Y, _, Sizing::Percent { .. }) => {
                element!().computed_height = 0.0;
            }
            (Axis::Y, _, sizing) => {
                let min = sizing.get_min();
                let max = sizing.get_max();
                element!().computed_height = element!().computed_height.clamp(min, max);
                element!().min_height = element!().min_height.clamp(min, max);
            }
        }
    });
}
