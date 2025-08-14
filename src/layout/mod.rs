use slotmap::SlotMap;
use windows::Win32::Graphics::Direct2D::{
    Common::{D2D_RECT_F, D2D1_COLOR_F},
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows_numerics::Vector2;

use crate::layout::{
    model::{Axis, UIElement, UIKey},
    positioning::position_elements,
    scroll_manager::ScrollStateManager,
};

pub mod model;
pub mod scroll_manager;

mod float;
mod visitors;

mod fit_along_axis;
mod grow_and_shrink_along_axis;
mod positioning;

use fit_along_axis::fit_along_axis;
use grow_and_shrink_along_axis::grow_and_shrink_along_axis;

pub type OwnedUITree = SlotMap<UIKey, UIElement>;
pub type UITree<'a> = &'a mut OwnedUITree;

#[allow(dead_code)]
fn set_parent_references(slots: UITree<'_>, root: UIKey) {
    visitors::visit_bfs(slots, root, |slots, key, parent| {
        slots[key].parent = parent;
    });
}

#[allow(dead_code)]
fn propagate_inherited_properties(slots: UITree<'_>, root: UIKey) {
    visitors::visit_bfs(slots, root, |slots, key, parent| {
        if let Some(parent_key) = parent {
            if slots[key].color.is_none() && slots[parent_key].color.is_some() {
                slots[key].color = slots[parent_key].color;
            }
        }

        // TODO: propagate Font
    });
}

fn wrap_text(slots: UITree<'_>, root: UIKey) {
    visitors::visit_bfs(slots, root, |slots, key, _parent| {
        if slots[key].is_text_element() {
            let element = &slots[key];

            let available_width =
                element.computed_width - element.padding.left - element.padding.right;

            let content = slots[key].content.as_ref().unwrap();
            unsafe {
                content
                    .layout
                    .as_ref()
                    .unwrap()
                    .SetMaxWidth(available_width)
                    .unwrap();
            }
        }
    });
}

pub fn layout<SS: ScrollStateManager>(
    slots: UITree<'_>,
    root: UIKey,
    scroll_state_manager: &mut SS,
) {
    set_parent_references(slots, root);
    propagate_inherited_properties(slots, root);

    fit_along_axis(slots, root, Axis::X);
    grow_and_shrink_along_axis(slots, root, Axis::X);

    wrap_text(slots, root);

    fit_along_axis(slots, root, Axis::Y);
    grow_and_shrink_along_axis(slots, root, Axis::Y);

    position_elements(slots, root, scroll_state_manager);
}

pub fn paint<SS: ScrollStateManager>(
    rt: &ID2D1HwndRenderTarget,
    brush: &ID2D1SolidColorBrush,
    slots: UITree<'_>,
    root: UIKey,
    _scroll_state_manager: &mut SS,
    offset_x: f32,
    offset_y: f32,
) {
    visitors::visit_dfs(
        slots,
        root,
        |slots, key, _parent| {
            let element = &slots[key];
            let x = element.x + offset_x;
            let y = element.y + offset_y;
            let width = element.computed_width;
            let height = element.computed_height;

            if let Some(color) = element.background_color {
                unsafe {
                    brush.SetColor(&D2D1_COLOR_F {
                        r: (0xFF & (color >> 24)) as f32 / 255.0,
                        g: (0xFF & (color >> 16)) as f32 / 255.0,
                        b: (0xFF & (color >> 8)) as f32 / 255.0,
                        a: (0xFF & color) as f32 / 255.0,
                    });

                    let rect = D2D_RECT_F {
                        left: x,
                        top: y,
                        right: x + width,
                        bottom: y + height,
                    };
                    rt.FillRectangle(&rect, brush);
                }
            }

            if let Some(layout) = element.content.as_ref().and_then(|c| c.layout.as_ref()) {
                let color = element.color.unwrap_or(0x000000FF);

                unsafe {
                    brush.SetColor(&D2D1_COLOR_F {
                        r: (0xFF & (color >> 24)) as f32 / 255.0,
                        g: (0xFF & (color >> 16)) as f32 / 255.0,
                        b: (0xFF & (color >> 8)) as f32 / 255.0,
                        a: (0xFF & color) as f32 / 255.0,
                    });
                    rt.DrawTextLayout(
                        Vector2 { X: x, Y: y },
                        layout,
                        brush,
                        D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                    );
                }
            }
        },
        None,
    );
}
