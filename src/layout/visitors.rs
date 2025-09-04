use std::collections::VecDeque;

use crate::layout::{BorrowedUITree, model::UIKey};

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
pub fn visit_reverse_bfs<Message, F, R>(
    ui_tree: BorrowedUITree<'_, Message>,
    element: UIKey,
    mut visitor: F,
) where
    F: FnMut(BorrowedUITree<'_, Message>, UIKey, Option<UIKey>) -> R,
    R: Into<VisitAction>,
{
    let mut queue: VecDeque<(UIKey, Option<UIKey>)> = VecDeque::from([(element, None)]);
    let mut stack: Vec<(UIKey, Option<UIKey>)> = Vec::new();

    // Collect in BFS order
    while let Some((current, parent)) = queue.pop_front() {
        stack.push((current, parent));
        for &child in ui_tree.slots[current].children.iter() {
            queue.push_back((child, Some(current)));
        }
    }

    // Visit in reverse order (from leaves to root)
    while let Some((current, parent)) = stack.pop() {
        if visitor(ui_tree, current, parent).into().is_exit() {
            break;
        }
    }
}

/// Standard breadth-first traversal.
pub fn visit_bfs<Message, F, R>(
    ui_tree: BorrowedUITree<'_, Message>,
    element: UIKey,
    mut visitor: F,
) where
    F: FnMut(BorrowedUITree<'_, Message>, UIKey, Option<UIKey>) -> R,
    R: Into<VisitAction>,
{
    let mut queue: VecDeque<(UIKey, Option<UIKey>)> = VecDeque::from([(element, None)]);

    while let Some((current, parent)) = queue.pop_front() {
        if visitor(ui_tree, current, parent).into().is_exit() {
            break;
        }
        for &child in ui_tree.slots[current].children.iter().rev() {
            queue.push_back((child, Some(current)));
        }
    }
}

/// Breadth-first traversal that defers visiting nodes based on a predicate.
/// Nodes that are deferred are revisited in subsequent passes until none are deferred.
pub fn visit_deferring_bfs<Message, S, F, R>(
    ui_tree: BorrowedUITree<'_, Message>,
    element: UIKey,
    mut should_defer: S,
    mut visitor: F,
) where
    S: FnMut(BorrowedUITree<'_, Message>, UIKey, Option<UIKey>) -> bool,
    F: FnMut(BorrowedUITree<'_, Message>, UIKey, Option<UIKey>) -> R,
    R: Into<VisitAction>,
{
    let mut queue: VecDeque<(UIKey, Option<UIKey>)> = VecDeque::from([(element, None)]);
    let mut deferred: Vec<(UIKey, Option<UIKey>)> = Vec::new();

    'exit: loop {
        while let Some((current, parent)) = queue.pop_front() {
            if should_defer(ui_tree, current, parent) {
                deferred.push((current, parent));
            } else if visitor(ui_tree, current, parent).into().is_exit() {
                break 'exit;
            } else {
                // Did not defer, enqueue children
                for &child in ui_tree.slots[current].children.iter().rev() {
                    queue.push_back((child, Some(current)));
                }
            }
        }

        if deferred.is_empty() {
            break;
        }

        // Next pass processes previously deferred items
        queue = deferred.drain(..).collect();
    }
}

#[derive(Clone, Copy)]
pub struct VisitFrame {
    pub element: UIKey,
    pub parent: Option<UIKey>,
    pub exit: bool,
}

/// Depth-first traversal that defers visiting nodes based on a predicate.
/// Nodes that are deferred are revisited in subsequent passes until none are deferred.
pub fn visit_deferring_dfs<Message, S, F, E, A, R>(
    ui_tree: BorrowedUITree<'_, Message>,
    element: UIKey,
    mut should_defer: S,
    mut visitor: F,
    mut exit_children_visitor: Option<E>,
    mut after_pass: Option<A>,
) where
    S: FnMut(BorrowedUITree<'_, Message>, UIKey, Option<UIKey>) -> bool,
    F: FnMut(BorrowedUITree<'_, Message>, UIKey, Option<UIKey>) -> R,
    E: FnMut(BorrowedUITree<'_, Message>, UIKey, Option<UIKey>),
    A: FnMut(BorrowedUITree<'_, Message>, &[VisitFrame]),
    R: Into<VisitAction>,
{
    let mut stack: Vec<VisitFrame> = vec![VisitFrame {
        element,
        parent: None,
        exit: false,
    }];
    let mut deferred: Vec<VisitFrame> = Vec::new();

    'exit: loop {
        while let Some(frame) = stack.pop() {
            if frame.exit {
                // This is the "exit children" phase, skip deferred check
                if let Some(f) = exit_children_visitor.as_mut() {
                    f(ui_tree, frame.element, frame.parent);
                }
                continue;
            }

            if should_defer(ui_tree, frame.element, frame.parent) {
                deferred.push(frame);
            } else if visitor(ui_tree, frame.element, frame.parent)
                .into()
                .is_exit()
            {
                break 'exit;
            } else {
                // Schedule exit after children
                stack.push(VisitFrame {
                    element: frame.element,
                    parent: frame.parent,
                    exit: true,
                });

                // Push children in reverse so they are processed in original order
                for &child in ui_tree.slots[frame.element].children.iter().rev() {
                    stack.push(VisitFrame {
                        element: child,
                        parent: Some(frame.element),
                        exit: false,
                    });
                }
            }
        }

        if deferred.is_empty() {
            break;
        }

        // Call after-pass callback before starting next pass
        if let Some(f) = after_pass.as_mut() {
            f(ui_tree, &deferred);
        }

        // Next pass processes previously deferred items
        stack = deferred.drain(..).collect();
    }
}

/// Depth-first traversal with optional "exit-children" callback.
pub fn visit_dfs<Message, F, E, R>(
    ui_tree: BorrowedUITree<'_, Message>,
    element: UIKey,
    mut visitor: F,
    mut exit_children_visitor: Option<E>,
) where
    F: FnMut(BorrowedUITree<'_, Message>, UIKey, Option<UIKey>) -> R,
    E: FnMut(BorrowedUITree<'_, Message>, UIKey, Option<UIKey>),
    R: Into<VisitAction>,
{
    let mut stack: Vec<VisitFrame> = vec![VisitFrame {
        element,
        parent: None,
        exit: false,
    }];

    while let Some(frame) = stack.pop() {
        if frame.exit {
            if let Some(f) = exit_children_visitor.as_mut() {
                f(ui_tree, frame.element, frame.parent);
            }
            continue;
        } else {
            if visitor(ui_tree, frame.element, frame.parent)
                .into()
                .is_exit()
            {
                break;
            }

            // Schedule exit after children
            stack.push(VisitFrame {
                element: frame.element,
                parent: frame.parent,
                exit: true,
            });
        }

        // Push children in reverse so they are processed in original order
        for &child in ui_tree.slots[frame.element].children.iter().rev() {
            stack.push(VisitFrame {
                element: child,
                parent: Some(frame.element),
                exit: false,
            });
        }
    }
}
