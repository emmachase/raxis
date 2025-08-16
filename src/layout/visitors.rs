use std::collections::VecDeque;

use slotmap::SlotMap;

use crate::layout::model::UIKey;

use super::model::UIElement;

pub enum VisitAction {
    Continue,
    Exit,
}

impl VisitAction {
    pub fn is_exit(&self) -> bool {
        matches!(self, VisitAction::Exit)
    }
}

impl From<bool> for VisitAction {
    fn from(val: bool) -> Self {
        if val {
            VisitAction::Continue
        } else {
            VisitAction::Exit
        }
    }
}

impl From<()> for VisitAction {
    fn from(_: ()) -> Self {
        VisitAction::Continue
    }
}

/// Breadth-first traversal that visits nodes from leaves back to the root.
pub fn visit_reverse_bfs<F, R>(
    slots: &mut SlotMap<UIKey, UIElement>,
    element: UIKey,
    mut visitor: F,
) where
    F: FnMut(&mut SlotMap<UIKey, UIElement>, UIKey, Option<UIKey>) -> R,
    R: Into<VisitAction>,
{
    let mut queue: VecDeque<(UIKey, Option<UIKey>)> = VecDeque::from([(element, None)]);
    let mut stack: Vec<(UIKey, Option<UIKey>)> = Vec::new();

    // Collect in BFS order
    while let Some((current, parent)) = queue.pop_front() {
        stack.push((current, parent));
        for &child in slots[current].children.iter() {
            queue.push_back((child, Some(current)));
        }
    }

    // Visit in reverse order (from leaves to root)
    while let Some((current, parent)) = stack.pop() {
        if visitor(slots, current, parent).into().is_exit() {
            break;
        }
    }
}

/// Standard breadth-first traversal.
pub fn visit_bfs<F, R>(slots: &mut SlotMap<UIKey, UIElement>, element: UIKey, mut visitor: F)
where
    F: FnMut(&mut SlotMap<UIKey, UIElement>, UIKey, Option<UIKey>) -> R,
    R: Into<VisitAction>,
{
    let mut queue: VecDeque<(UIKey, Option<UIKey>)> = VecDeque::from([(element, None)]);

    while let Some((current, parent)) = queue.pop_front() {
        if visitor(slots, current, parent).into().is_exit() {
            break;
        }
        for &child in slots[current].children.iter() {
            queue.push_back((child, Some(current)));
        }
    }
}

/// Breadth-first traversal that defers visiting nodes based on a predicate.
/// Nodes that are deferred are revisited in subsequent passes until none are deferred.
pub fn visit_deferring_bfs<S, F, R>(
    slots: &mut SlotMap<UIKey, UIElement>,
    element: UIKey,
    mut should_defer: S,
    mut visitor: F,
) where
    S: FnMut(&mut SlotMap<UIKey, UIElement>, UIKey, Option<UIKey>) -> bool,
    F: FnMut(&mut SlotMap<UIKey, UIElement>, UIKey, Option<UIKey>) -> R,
    R: Into<VisitAction>,
{
    let mut queue: VecDeque<(UIKey, Option<UIKey>)> = VecDeque::from([(element, None)]);
    let mut deferred: Vec<(UIKey, Option<UIKey>)> = Vec::new();

    'exit: loop {
        while let Some((current, parent)) = queue.pop_front() {
            if should_defer(slots, current, parent) {
                deferred.push((current, parent));
            } else if visitor(slots, current, parent).into().is_exit() {
                break 'exit;
            }

            // Always enqueue children
            for &child in slots[current].children.iter() {
                queue.push_back((child, Some(current)));
            }
        }

        if deferred.is_empty() {
            break;
        }

        // Next pass processes previously deferred items
        queue = deferred.drain(..).collect();
    }
}

/// Depth-first traversal with optional "exit-children" callback.
pub fn visit_dfs<F, R>(
    slots: &mut SlotMap<UIKey, UIElement>,
    element: UIKey,
    mut visitor: F,
    mut exit_children_visitor: Option<
        &mut dyn FnMut(&mut SlotMap<UIKey, UIElement>, UIKey, Option<UIKey>),
    >,
) where
    F: FnMut(&mut SlotMap<UIKey, UIElement>, UIKey, Option<UIKey>) -> R,
    R: Into<VisitAction>,
{
    #[derive(Clone, Copy)]
    struct Frame {
        element: UIKey,
        parent: Option<UIKey>,
        exit: bool,
    }

    let mut stack: Vec<Frame> = vec![Frame {
        element,
        parent: None,
        exit: false,
    }];

    while let Some(frame) = stack.pop() {
        if frame.exit {
            if let Some(f) = exit_children_visitor.as_mut() {
                f(slots, frame.element, frame.parent);
            }
            continue;
        } else {
            if visitor(slots, frame.element, frame.parent).into().is_exit() {
                break;
            }

            // Schedule exit after children
            stack.push(Frame {
                element: frame.element,
                parent: frame.parent,
                exit: true,
            });
        }

        // Push children in reverse so they are processed in original order
        for &child in slots[frame.element].children.iter().rev() {
            stack.push(Frame {
                element: child,
                parent: Some(frame.element),
                exit: false,
            });
        }
    }
}
