use crate::layout::{
    BorrowedUITree,
    float::EpsFloatCmp,
    model::{Axis, Direction, Sizing, UIKey},
    visitors,
};

pub fn grow_and_shrink_along_axis(slots: BorrowedUITree<'_>, root: UIKey, axis: Axis) {
    let x_axis = matches!(axis, Axis::X);

    visitors::visit_bfs(slots, root, |slots, key, _parent| {
        // Use a macro to safely obtain multiple mutable reads/writes separated by operations
        macro_rules! element {
            () => {
                slots[key]
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
            .filter(|c| slots[*c].floating.is_none())
            .collect();

        // Children which are resizable in this pass (i.e., not Percent sizing)
        let resizable_children: Vec<UIKey> = non_floating_children
            .iter()
            .copied()
            .filter(|c| {
                let s = if x_axis {
                    slots[*c].width
                } else {
                    slots[*c].height
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

        remaining_size -= size_child_gap;

        let mut inner_content_size = total_padding + size_child_gap;

        // Precompute inner content size from resizable non-percent children
        for child in &resizable_children {
            let c = &slots[*child];
            if element!().direction == axis_direction {
                if x_axis {
                    inner_content_size += c.computed_width;
                } else {
                    inner_content_size += c.computed_height;
                }
            } else {
                if x_axis {
                    inner_content_size = inner_content_size.max(c.computed_width);
                } else {
                    inner_content_size = inner_content_size.max(c.computed_height);
                }
            }
        }

        let available_size = remaining_size;

        // Size Percent children
        let children_all = element!().children.clone();
        for child in &children_all {
            let sizing = if x_axis {
                slots[*child].width
            } else {
                slots[*child].height
            };
            if let Sizing::Percent { percent } = sizing {
                let assign = available_size * percent;
                if x_axis {
                    slots[*child].computed_width = assign;
                } else {
                    slots[*child].computed_height = assign;
                }

                // For non-floating children, subtract from remaining and add to content size
                if slots[*child].floating.is_none() {
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
                        slots[*c].width
                    } else {
                        slots[*c].height
                    },
                    Sizing::Grow { .. }
                )
            })
            .collect();

        if element!().direction == axis_direction {
            // Subtract current sizes of resizable children
            remaining_size -= resizable_children.iter().fold(0.0_f32, |acc, ckey| {
                let c = &slots[*ckey];
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
                    let c = &slots[*ck];
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
                        slots[ck].computed_width
                    } else {
                        slots[ck].computed_height
                    };
                    if size.eq_eps(&smallest_size) {
                        if x_axis {
                            slots[ck].computed_width += size_to_add;
                        } else {
                            slots[ck].computed_height += size_to_add;
                        }
                        remaining_size -= size_to_add;

                        // Remove if reached max
                        let sizing = if x_axis {
                            slots[ck].width
                        } else {
                            slots[ck].height
                        };
                        let max_allowed = match sizing {
                            Sizing::Grow { max, .. } => max,
                            _ => f32::INFINITY,
                        };
                        let new_size = if x_axis {
                            slots[ck].computed_width
                        } else {
                            slots[ck].computed_height
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
                    let e = &slots[*c];
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
                    let c = &slots[*ck];
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

                let mut size_to_sub =
                    (largest_size - second_largest_size).min(largest_size - largest_min_for_group);
                size_to_sub = size_to_sub.min((-remaining_size) / count_largest as f32);

                let mut i = 0;
                while i < shrinkable_children.len() {
                    let ck = shrinkable_children[i];
                    let size = if x_axis {
                        slots[ck].computed_width
                    } else {
                        slots[ck].computed_height
                    };
                    if size.eq_eps(&largest_size) {
                        if x_axis {
                            slots[ck].computed_width -= size_to_sub;
                        } else {
                            slots[ck].computed_height -= size_to_sub;
                        }
                        remaining_size += size_to_sub;

                        // Remove if reached min
                        let min_allowed = if x_axis {
                            slots[ck].min_width
                        } else {
                            slots[ck].min_height
                        };
                        let new_size = if x_axis {
                            slots[ck].computed_width
                        } else {
                            slots[ck].computed_height
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
        } else {
            // Cross-axis behavior
            let mut grow_to_size = available_size;
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
                grow_to_size = inner_content_size.max(available_size);
            }

            // Grow
            for ck in &growable_children {
                let size = if x_axis {
                    slots[*ck].computed_width
                } else {
                    slots[*ck].computed_height
                };
                if size.lt_eps(&grow_to_size) {
                    // cap by max
                    let max_allowed = match if x_axis {
                        slots[*ck].width
                    } else {
                        slots[*ck].height
                    } {
                        Sizing::Grow { max, .. } => max,
                        _ => f32::INFINITY,
                    };
                    let new_size = grow_to_size.min(max_allowed);
                    if x_axis {
                        slots[*ck].computed_width = new_size;
                    } else {
                        slots[*ck].computed_height = new_size;
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
                    let e = &slots[*c];
                    let size = if x_axis {
                        e.computed_width
                    } else {
                        e.computed_height
                    };
                    let min_allowed = if x_axis { e.min_width } else { e.min_height };
                    size.gt_eps(&min_allowed) && size.gt_eps(&available_size)
                })
                .collect();

            for ck in shrinkable_children {
                let size = if x_axis {
                    slots[ck].computed_width
                } else {
                    slots[ck].computed_height
                };
                if size.gt_eps(&available_size) {
                    let min_allowed = if x_axis {
                        slots[ck].min_width
                    } else {
                        slots[ck].min_height
                    };
                    let new_size = available_size.max(min_allowed);
                    if x_axis {
                        slots[ck].computed_width = new_size;
                    } else {
                        slots[ck].computed_height = new_size;
                    }
                }
            }
        }
    });
}
