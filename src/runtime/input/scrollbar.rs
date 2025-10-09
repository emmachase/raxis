use crate::gfx::PointDIP;
use crate::layout::model::Axis;
use crate::layout::{OwnedUITree, compute_scrollbar_geom};
use crate::Shell;

/// State for active scrollbar dragging
#[derive(Clone, Copy, Debug)]
pub struct ScrollbarDragState {
    pub element_id: u64,
    pub axis: Axis,
    /// Offset within the thumb (along the drag axis) where the pointer grabbed, in DIPs
    pub grab_offset: f32,
}

/// Hit tests scrollbar thumbs following the same visibility logic as event dispatch.
/// This prevents scrolling through obscuring elements by checking:
/// 1. If the point is within an innermost element
/// 2. If the point is visible within all scroll ancestors
/// 3. If any element in the ancestry chain has a scrollbar at the hit point
pub fn hit_test_scrollbar_thumb<Message>(
    ui_tree: &mut OwnedUITree<Message>,
    shell: &mut Shell<Message>,
    x: f32,
    y: f32,
    only_thumb: bool,
) -> Option<ScrollbarDragState> {
    let point = PointDIP { x, y };

    // First, find the innermost element at this position (respecting scroll clipping)
    let innermost_key = Shell::find_innermost_element_at(ui_tree, x, y)?;

    // Collect the ancestry from innermost to root
    let ancestry = Shell::collect_ancestry(ui_tree, innermost_key);

    // Check scrollbars from innermost to outermost (matching event dispatch order)
    // This ensures that overlapping elements block scrollbars behind them
    for &key in &ancestry {
        let element = &ui_tree.slots[key];

        if element.id.is_none() {
            continue;
        }

        // Check Y-axis scrollbar
        if let Some(geom) = compute_scrollbar_geom(shell, element, Axis::Y) {
            let tr = if only_thumb {
                geom.thumb_rect
            } else {
                geom.track_rect
            };
            let thumb = geom.thumb_rect;

            // Check if point is within the track/thumb rect
            if point.within(tr) {
                let grab_offset = y - thumb.y;
                return Some(ScrollbarDragState {
                    element_id: element.id.unwrap(),
                    axis: Axis::Y,
                    grab_offset,
                });
            }
        }

        // Check X-axis scrollbar
        if let Some(geom) = compute_scrollbar_geom(shell, element, Axis::X) {
            let tr = if only_thumb {
                geom.thumb_rect
            } else {
                geom.track_rect
            };
            let thumb = geom.thumb_rect;

            // Check if point is within the track/thumb rect
            if point.within(tr) {
                let grab_offset = x - thumb.x;
                return Some(ScrollbarDragState {
                    element_id: element.id.unwrap(),
                    axis: Axis::X,
                    grab_offset,
                });
            }
        }
    }
    None
}
