use crate::{
    layout::{
        BorrowedUITree,
        model::{Axis, Direction, ElementContent, Sizing, UIElement, UIKey},
        visitors,
    },
    widgets::limit_response,
};

pub fn fit_along_axis(ui_tree: BorrowedUITree<'_>, root: UIKey, axis: Axis) {
    // Helper to check scroll flag for an axis
    fn is_scroll_enabled(el: &UIElement, axis: Axis) -> bool {
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

        // Mutable borrow after we compute padding
        // let element = &slots[key];
        macro_rules! element {
            () => {
                ui_tree.slots[key]
            };
        }

        if element!().content.is_some()
            && matches!(
                element!().content.as_ref().unwrap(),
                ElementContent::Text { .. }
            )
        {
            // Text sizing
            if x_axis {
                // Max width is infinite for now, this will get adjusted during the Text Wrap phase
                let metrics = unsafe {
                    let layout = element!()
                        .content
                        .as_ref()
                        .unwrap()
                        .unwrap_text()
                        .as_ref()
                        .unwrap();

                    layout.SetMaxWidth(f32::INFINITY).unwrap();

                    let mut metrics =
                        windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();

                    layout.GetMetrics(&mut metrics).unwrap();

                    metrics
                };

                element!().computed_width = metrics.width + axis_padding;

                let layout = element!()
                    .content
                    .as_ref()
                    .unwrap()
                    .unwrap_text()
                    .as_ref()
                    .unwrap();

                // Minimum width
                let min_base = unsafe { layout.DetermineMinWidth().unwrap() };
                element!().min_width = min_base + axis_padding;
            } else {
                let metrics = unsafe {
                    let layout = element!()
                        .content
                        .as_ref()
                        .unwrap()
                        .unwrap_text()
                        .as_ref()
                        .unwrap();

                    let mut metrics =
                        windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();

                    layout.GetMetrics(&mut metrics).unwrap();

                    metrics
                };

                // Sum of wrapped line heights
                element!().computed_height = metrics.height + axis_padding;
                element!().min_height = metrics.height + axis_padding;
            }
        } else {
            // Container sizing (applies to both regular containers and widgets)
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

            if element!().direction == axis_direction {
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
            } else if element!().direction == off_axis_direction {
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

            // Apply widget limits as additional constraints if this is a widget
            if let Some((widget, instance)) =
                element!()
                    .content
                    .as_ref()
                    .and_then(|content| match content {
                        ElementContent::Widget(widget) => element!().id.and_then(|id| {
                            ui_tree.state.get(&id).map(|instance| (widget, instance))
                        }),
                        _ => None,
                    })
            {
                if x_axis {
                    let limit_response::SizingForX {
                        min_width,
                        preferred_width,
                    } = widget.limits_x(instance);

                    // Apply widget limits as constraints to the computed container size
                    element!().computed_width = element!().computed_width.max(preferred_width);
                    element!().min_width = element!().min_width.max(min_width);
                } else {
                    let limit_response::SizingForY {
                        min_height,
                        preferred_height,
                    } = widget.limits_y(instance, element!().computed_width);

                    // Apply widget limits as constraints to the computed container size
                    element!().computed_height = element!().computed_height.max(preferred_height);
                    element!().min_height = element!().min_height.max(min_height);
                }
            }
        }

        // Clamp to sizing (unless Percent, which is deferred to grow/shrink)
        match (axis, element!().width, element!().height) {
            (Axis::X, Sizing::Percent { .. }, _) => {
                element!().computed_width = 0.0;
            }
            (Axis::X, sizing, _) => {
                let min = sizing.min();
                let max = sizing.max();
                element!().computed_width = element!().computed_width.clamp(min, max);
                element!().min_width = element!().min_width.clamp(min, max);
            }
            (Axis::Y, _, Sizing::Percent { .. }) => {
                element!().computed_height = 0.0;
            }
            (Axis::Y, _, sizing) => {
                let min = sizing.min();
                let max = sizing.max();
                element!().computed_height = element!().computed_height.clamp(min, max);
                element!().min_height = element!().min_height.clamp(min, max);
            }
        }
    });
}
