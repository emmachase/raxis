use std::{
    any::Any,
    cell::RefCell,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};

use log::{debug, trace};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Gdi::InvalidateRect,
    UI::{
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{KillTimer, SetTimer},
    },
};

use crate::{
    gfx::{PointDIP, RectDIP},
    layout::{
        BorrowedUITree,
        model::{Element, UIKey},
        visitors::{self, VisitAction},
    },
    math::easing::Easing,
    runtime::{focus::FocusManager, scroll::ScrollStateManager, task::Task},
    widgets::{DragData, DragEvent, DropResult, Event, Operation, dispatch_operation},
};

pub mod gfx;
pub mod layout;
pub mod math;
pub mod proc;
pub mod runtime;
pub mod util;
pub mod widgets;

pub use raxis_core::{PathCommand, SvgPathCommands};
pub use raxis_proc_macro::svg_path;

#[derive(Default)]
pub struct HookState {
    // TODO: Discriminate by TypeId?
    hooks: Vec<RefCell<Box<dyn Any>>>,
}

pub struct HookManager<'a, Message> {
    ui_tree: BorrowedUITree<'a, Message>,

    pub scroll_state_manager: &'a ScrollStateManager,
    pub focus_manager: &'a FocusManager,

    layout_invalidated: bool,
    requested_animation: bool,

    pub window_active: bool,
}

pub struct HookInstance<'a> {
    state: &'a mut HookState,
    position: usize,
}

impl<'a> HookInstance<'a> {
    pub fn use_hook<T: 'static>(&mut self, initializer: impl FnOnce() -> T) -> &mut T {
        if self.position >= self.state.hooks.len() {
            self.state.hooks.push(RefCell::new(Box::new(initializer())));
        }
        let hook = self.state.hooks[self.position]
            .get_mut()
            .downcast_mut::<T>()
            .unwrap();
        self.position += 1;
        hook
    }

    pub fn use_state<T: 'static>(&mut self, initializer: impl FnOnce() -> T) -> Rc<RefCell<T>> {
        self.use_hook(|| Rc::new(RefCell::new(initializer())))
            .clone()
    }
}

impl<Message> HookManager<'_, Message> {
    pub fn instance(&mut self, id: u64) -> HookInstance {
        let state = self.ui_tree.hook_state.entry(id).or_insert_with(|| {
            debug!("Creating hook state for {id}");
            HookState::default()
        });

        HookInstance { state, position: 0 }
    }

    pub fn invalidate_layout(&mut self) {
        self.layout_invalidated = true;
    }

    pub fn request_animation(&mut self) {
        self.requested_animation = true;
    }
}

// pub trait IntoKeyframe {
//     fn into_keyframe(&self) -> f32;
// }

// impl IntoKeyframe for bool {
//     fn into_keyframe(&self) -> f32 {
//         if *self { 1.0 } else { 0.0 }
//     }
// }

// impl IntoKeyframe for f32 {
//     fn into_keyframe(&self) -> f32 {
//         *self
//     }
// }

// impl IntoKeyframe for f64 {
//     fn into_keyframe(&self) -> f32 {
//         *self as f32
//     }
// }

pub trait Interpolate {
    fn interpolate(&self, other: Self, alpha: f32) -> Self;
}

impl Interpolate for f32 {
    fn interpolate(&self, other: Self, alpha: f32) -> Self {
        self + (other - self) * alpha
    }
}

impl Interpolate for f64 {
    fn interpolate(&self, other: Self, alpha: f32) -> Self {
        self + (other - self) * alpha as f64
    }
}

impl Interpolate for PointDIP {
    fn interpolate(&self, other: Self, alpha: f32) -> Self {
        PointDIP {
            x: self.x.interpolate(other.x, alpha),
            y: self.y.interpolate(other.y, alpha),
        }
    }
}

impl Interpolate for RectDIP {
    fn interpolate(&self, other: Self, alpha: f32) -> Self {
        RectDIP {
            x: self.x.interpolate(other.x, alpha),
            y: self.y.interpolate(other.y, alpha),
            width: self.width.interpolate(other.width, alpha),
            height: self.height.interpolate(other.height, alpha),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Animation<S: Clone + Copy + PartialEq> {
    active: bool,
    target: S,
    last_target: S,
    origin_ts: Instant,
    duration: Duration,
    easing: Easing,
}

impl<S: Clone + Copy + PartialEq> Animation<S> {
    pub fn new(initial: S) -> Self {
        Self {
            active: false,
            target: initial,
            last_target: initial,
            origin_ts: Instant::now(),
            duration: Duration::from_millis(100),
            easing: Easing::EaseOut,
        }
    }

    pub fn update(&mut self, state: S) {
        if state != self.target {
            self.active = true;
            self.last_target = self.target;
            self.target = state;
            self.origin_ts = Instant::now();
        }
    }

    pub fn value(&self) -> S {
        self.target
    }
}

impl<S: Clone + Copy + PartialEq + Default> Default for Animation<S> {
    fn default() -> Self {
        Self::new(S::default())
    }
}

pub fn use_animation<S: Clone + Copy + PartialEq + 'static>(
    hook: &mut HookInstance,
    state: S,
) -> Animation<S> {
    // let mut instance = hook.instance(id);
    let animation = hook.use_hook(|| Animation::new(state));

    animation.update(state);

    animation.clone()
}

impl<S: Clone + Copy + PartialEq> Animation<S> {
    pub fn duration(self, duration: Duration) -> Self {
        Self { duration, ..self }
    }

    pub fn easing(self, easing: Easing) -> Self {
        Self { easing, ..self }
    }

    pub fn interpolate_using<I: Interpolate>(
        &self,
        hook: &mut impl RequestAnimation,
        f: impl Fn(S) -> I,
        at: Instant,
    ) -> I {
        if !self.active {
            return f(self.target);
        }

        let alpha = (at.duration_since(self.origin_ts)).as_secs_f32() / self.duration.as_secs_f32();
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha < 1.0 {
            hook.request_animation();
        }

        // let alpha = self
        //     .last_target
        //     .into_keyframe()
        //     .interpolate(self.target.into_keyframe(), alpha);

        let alpha = self.easing.apply(alpha);

        f(self.last_target).interpolate(f(self.target), alpha)
    }
}

pub trait RequestAnimation {
    fn request_animation(&mut self);
}

impl<Message> RequestAnimation for HookManager<'_, Message> {
    fn request_animation(&mut self) {
        self.requested_animation = true;
    }
}

impl<Message> RequestAnimation for Shell<Message> {
    fn request_animation(&mut self) {
        self.redraw_request = RedrawRequest::Immediate;
    }
}

impl Animation<bool> {
    pub fn interpolate<I: Interpolate + Clone>(
        &self,
        hook: &mut impl RequestAnimation, //&mut HookManager<Message>,
        start: I,
        end: I,
        at: Instant,
    ) -> I {
        self.interpolate_using(hook, |b| if b { end.clone() } else { start.clone() }, at)
    }
}

pub type ViewFn<State, Message> = fn(&State, &mut HookManager<Message>) -> Element<Message>;
pub type UpdateFn<State, Message> =
    fn(&mut State, Message) -> Option<crate::runtime::task::Task<Message>>;
pub type EventMapperFn<Message> = fn(Event, Option<u64>) -> Option<Message>;

pub use runtime::Application;

pub enum DeferredControl {
    StartDrag { data: DragData, src_id: u64 },

    SetIMEPosition { position: gfx::PointDIP },
    DisableIME,

    SetClipboardText(String),
}

pub struct Shell<Message> {
    focus_manager: FocusManager,
    scroll_state_manager: ScrollStateManager,
    input_method: InputMethod,

    event_captured_by: Option<u64>,

    /// Track which widget currently has drag focus for proper drag_leave handling
    current_drag_widget: Option<layout::model::UIKey>,

    /// Track which element is currently active (from mouse down) for event dispatching
    active_element_id: Option<u64>,

    /// Track the ancestry of element IDs under the mouse from the previous event
    previous_mouse_ancestry: Vec<u64>,

    operation_queue: Vec<Box<dyn Operation>>,

    // requested_next_redraw: bool,
    // redraw_timers: Vec<usize>,
    // next_timer_id: usize,
    redraw_request: RedrawRequest,

    deferred_controls: Vec<DeferredControl>,

    event_mapper: EventMapperFn<Message>,
    message_sender: mpsc::Sender<Message>,
    pending_messages: bool,

    task_dispatcher: mpsc::Sender<Task<Message>>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum RedrawRequest {
    Immediate,
    At(Instant),
    Wait,
}

pub enum InputMethod {
    Disabled,
    Enabled { position: gfx::PointDIP },
}

pub const REDRAW_TIMER_ID: usize = 50;

impl<Message> Shell<Message> {
    pub fn new(
        message_sender: mpsc::Sender<Message>,
        task_dispatcher: mpsc::Sender<Task<Message>>,
        scroll_state_manager: ScrollStateManager,
        focus_manager: FocusManager,
        event_mapper: EventMapperFn<Message>,
    ) -> Self {
        Self {
            focus_manager,
            scroll_state_manager,
            input_method: InputMethod::Disabled,

            event_captured_by: None,
            current_drag_widget: None,
            active_element_id: None,
            previous_mouse_ancestry: Vec::new(),
            operation_queue: Vec::new(),
            redraw_request: RedrawRequest::Wait,
            deferred_controls: Vec::new(),
            message_sender,
            pending_messages: false,
            task_dispatcher,
            event_mapper,
        }
    }

    pub fn dispatch_task(&mut self, task: Task<Message>) {
        self.task_dispatcher.send(task).unwrap();
    }

    pub fn queue_operation(&mut self, operation: Box<dyn Operation>) {
        self.operation_queue.push(operation);
    }

    pub fn queue_deferred_control(&mut self, control: DeferredControl) {
        self.deferred_controls.push(control);
    }

    pub fn drain_deferred_controls(&mut self) -> Option<Vec<DeferredControl>> {
        if self.deferred_controls.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.deferred_controls))
        }
    }

    /// Replaces the current redraw request with the given request.
    /// This will override any existing request, you almost never want to use this.
    pub fn replace_redraw_request(&mut self, request: RedrawRequest) {
        self.redraw_request = request;
    }

    pub fn request_redraw(&mut self, hwnd: HWND, request: RedrawRequest) {
        if request < self.redraw_request {
            self.redraw_request = request;

            if matches!(request, RedrawRequest::Immediate) {
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            } else {
                let uelapse = match request {
                    RedrawRequest::Wait => return,
                    RedrawRequest::At(instant) => {
                        instant.duration_since(Instant::now()).as_millis() as u32
                    }
                    _ => 0,
                };
                unsafe { SetTimer(Some(hwnd), REDRAW_TIMER_ID, uelapse, None) };
            }
        }
    }

    pub fn kill_redraw_timer(&mut self, hwnd: HWND, timer_id: usize) {
        if timer_id == REDRAW_TIMER_ID {
            self.redraw_request = RedrawRequest::Wait;
            unsafe {
                let _ = KillTimer(Some(hwnd), timer_id);
            }
        }
    }

    /// Find a UIKey by element ID
    fn find_key_by_id(
        ui_tree: BorrowedUITree<Message>,
        target_id: u64,
    ) -> Option<layout::model::UIKey> {
        let mut found_key = None;
        visitors::visit_reverse_bfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
            let element = &ui_tree.slots[key];
            if let Some(id) = element.id {
                if id == target_id {
                    found_key = Some(key);
                    return VisitAction::Exit;
                }
            }
            VisitAction::Continue
        });
        found_key
    }

    /// Find the innermost element at a given position
    fn find_innermost_element_at(
        ui_tree: BorrowedUITree<Message>,
        x: f32,
        y: f32,
    ) -> Option<UIKey> {
        let point = gfx::PointDIP { x, y };
        let mut innermost_id = None;

        // Use reverse DFS to find the innermost element (last leaf that contains the point)
        visitors::visit_reverse_dfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
            let element = &ui_tree.slots[key];
            let bounds = element.bounds();

            // Check if point is within the border box (the full element including padding)
            if point.within(bounds.border_box) {
                // Additionally check if the point is within all scrollable ancestor viewports
                if Self::is_point_visible_in_scroll_ancestors(ui_tree, key, point) {
                    innermost_id = Some(key);
                    return VisitAction::Exit;
                }
            }

            VisitAction::Continue
        });

        innermost_id
    }

    /// Check if a point is visible within all scrollable ancestor viewports
    fn is_point_visible_in_scroll_ancestors(
        ui_tree: BorrowedUITree<Message>,
        element_key: UIKey,
        point: gfx::PointDIP,
    ) -> bool {
        let mut current_key = element_key;

        // Walk up the parent chain
        loop {
            let element = &ui_tree.slots[current_key];

            if let Some(parent_key) = element.parent {
                let parent = &ui_tree.slots[parent_key];

                // If parent is scrollable, check if point is within its content box (viewport)
                if parent.scroll.is_some() {
                    let parent_bounds = parent.bounds();
                    if !point.within(parent_bounds.content_box) {
                        return false;
                    }
                }

                current_key = parent_key;
            } else {
                break;
            }
        }

        true
    }

    /// Collect all ancestor keys from a given element ID up to the root
    fn collect_ancestry(
        ui_tree: BorrowedUITree<Message>,
        target_key: UIKey,
    ) -> Vec<layout::model::UIKey> {
        let mut ancestry = Vec::new();

        // Walk up the parent chain using UIElement.parent
        let mut current_key = target_key;
        loop {
            ancestry.push(current_key);

            let element = &ui_tree.slots[current_key];
            if let Some(parent_key) = element.parent {
                current_key = parent_key;
            } else {
                break;
            }
        }

        ancestry
    }

    /// If the target_key's ancestry converges with `shared_ancestry`, merge the children elements up to the divergence point
    fn try_extend_ancestry_to(
        ui_tree: BorrowedUITree<Message>,
        shared_ancestry: Vec<UIKey>,
        target_key: Option<UIKey>,
    ) -> Vec<UIKey> {
        let Some(first) = shared_ancestry.first() else {
            return shared_ancestry;
        };

        if target_key.is_none() {
            return shared_ancestry;
        }

        let mut ancestry = Vec::new();

        // Walk up the parent chain using UIElement.parent
        let mut current_key = target_key.unwrap();
        loop {
            if current_key == *first {
                ancestry.extend_from_slice(&shared_ancestry);
                return ancestry;
            }

            ancestry.push(current_key);

            let element = &ui_tree.slots[current_key];
            if let Some(parent_key) = element.parent {
                current_key = parent_key;
            } else {
                break;
            }
        }

        shared_ancestry
    }

    pub fn dispatch_event(&mut self, hwnd: HWND, ui_tree: BorrowedUITree<Message>, event: Event) {
        self.event_captured_by = None;

        // For mouse events, use targeted dispatching
        if event.is_mouse_event() {
            if let Some((x, y)) = event.mouse_position() {
                // Determine target element ID
                let innermost_key = Self::find_innermost_element_at(ui_tree, x, y);
                let target_key = if let Some(active_id) = self.active_element_id {
                    // If there's an active element, use it
                    Self::find_key_by_id(ui_tree, active_id).or(innermost_key)
                } else {
                    // Otherwise, find the innermost element at the mouse position
                    innermost_key
                };

                // Dispatch to target and its ancestry
                if let Some(target_key) = target_key {
                    let ancestry_keys = Self::collect_ancestry(ui_tree, target_key);

                    // Merge in any inner children if we share common ancestry
                    // This way child elements still receive mouse events if only the container is actually capturing the event
                    let ancestry_keys =
                        Self::try_extend_ancestry_to(ui_tree, ancestry_keys, innermost_key);

                    // Collect current ancestry IDs for enter/leave tracking
                    let mut current_ancestry_ids = Vec::new();
                    for &key in &ancestry_keys {
                        if let Some(id) = ui_tree.slots[key].id {
                            current_ancestry_ids.push(id);
                        }
                    }

                    if matches!(event, Event::MouseButtonDown { .. })
                        && let Some(id) = self.focus_manager.focused_widget
                        && !current_ancestry_ids.contains(&id)
                    {
                        self.focus_manager.release_focus(id);
                    }

                    // For MouseMove events, generate synthetic enter/leave events
                    if matches!(event, Event::MouseMove { .. }) {
                        // Find elements that were left (in previous but not in current)
                        // TODO: I don't like cloning here...
                        for &prev_id in &self.previous_mouse_ancestry.clone() {
                            if !current_ancestry_ids.contains(&prev_id) {
                                self.dispatch_event_to(
                                    hwnd,
                                    ui_tree,
                                    Event::MouseLeave { x, y },
                                    prev_id,
                                );
                            }
                        }

                        // Find elements that were entered (in current but not in previous)
                        for &curr_id in &current_ancestry_ids {
                            if !self.previous_mouse_ancestry.contains(&curr_id) {
                                self.dispatch_event_to(
                                    hwnd,
                                    ui_tree,
                                    Event::MouseEnter { x, y },
                                    curr_id,
                                );
                            }
                        }
                    }

                    // Dispatch the main event from innermost to outermost
                    for key in ancestry_keys {
                        let element = &mut ui_tree.slots[key];
                        let bounds = element.bounds();

                        if let Some(ref mut widget) = element.content {
                            if let Some(id) = element.id {
                                let instance = ui_tree.widget_state.get_mut(&id).unwrap();
                                widget.update(
                                    &mut ui_tree.arenas,
                                    instance,
                                    hwnd,
                                    self,
                                    &event,
                                    bounds,
                                );

                                if self.event_captured_by.is_some() {
                                    break;
                                }
                            }
                        }
                    }

                    // Update previous ancestry for next event
                    if matches!(event, Event::MouseMove { .. }) {
                        self.previous_mouse_ancestry = current_ancestry_ids;
                    }
                } else if matches!(event, Event::MouseMove { .. }) {
                    // Mouse is outside all elements - send leave events to all previously hovered elements
                    // TODO: I don't like cloning here...
                    for &prev_id in &self.previous_mouse_ancestry.clone() {
                        self.dispatch_event_to(hwnd, ui_tree, Event::MouseLeave { x, y }, prev_id);
                    }
                    self.previous_mouse_ancestry.clear();
                }

                // Track active element on mouse down/up
                if matches!(event, Event::MouseButtonDown { .. }) {
                    // let key = Self::find_innermost_element_at(ui_tree, x, y);
                    // if let Some(id) = self.event_captured_by {
                    //     self.active_element_id = Some(id); //target_key.and_then(|key| ui_tree.slots[key].id); // key.and_then(|key| ui_tree.slots[key].id);
                    // }
                    self.active_element_id = self.event_captured_by;
                } else if matches!(event, Event::MouseButtonUp { .. }) {
                    self.active_element_id = None;
                }

                if let Some(message) = (self.event_mapper)(event, self.event_captured_by) {
                    self.publish(message);
                }
                return;
            }
        }

        // For non-mouse events, use the original broadcast behavior
        visitors::visit_reverse_bfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
            let element = &mut ui_tree.slots[key];
            let bounds = element.bounds();
            if let Some(ref mut widget) = element.content {
                if let Some(id) = element.id {
                    let instance = ui_tree.widget_state.get_mut(&id).unwrap();
                    widget.update(&mut ui_tree.arenas, instance, hwnd, self, &event, bounds);
                }

                if self.event_captured_by.is_some() {
                    return VisitAction::Exit;
                }
            }

            VisitAction::Continue
        });

        if let Some(message) = (self.event_mapper)(event, self.event_captured_by) {
            self.publish(message);
        }
    }

    pub fn dispatch_event_to(
        &mut self,
        hwnd: HWND,
        ui_tree: BorrowedUITree<Message>,
        event: Event,
        target_id: u64,
    ) {
        self.event_captured_by = None;

        // Find the element and dispatch directly
        if let Some(key) = Self::find_key_by_id(ui_tree, target_id) {
            let element = &mut ui_tree.slots[key];
            let bounds = element.bounds();
            if let Some(ref mut widget) = element.content {
                let instance = ui_tree.widget_state.get_mut(&target_id).unwrap();
                widget.update(&mut ui_tree.arenas, instance, hwnd, self, &event, bounds);
            }
        }
    }

    pub fn publish(&mut self, message: Message) {
        self.message_sender.send(message).unwrap();
        self.pending_messages = true;
    }

    pub fn dispatch_operations(&mut self, ui_tree: BorrowedUITree<Message>) {
        for operation in self.operation_queue.drain(..) {
            dispatch_operation(ui_tree, &*operation);
        }
    }

    /// Debug function to print the UI tree structure
    pub fn debug_print_tree(ui_tree: BorrowedUITree<Message>) {
        trace!("\n┌─ UI Tree ─────────────────────────────────────");
        Self::debug_print_tree_recursive(ui_tree, ui_tree.root, "", true);
        trace!("└───────────────────────────────────────────────\n");
    }

    fn debug_print_tree_recursive(
        ui_tree: BorrowedUITree<Message>,
        key: UIKey,
        prefix: &str,
        is_last: bool,
    ) {
        let element = &ui_tree.slots[key];

        // Determine the branch character
        let branch = if is_last { "└──" } else { "├──" };

        // Print the current node
        let id_str = element
            .id
            .map(|id| format!("id:{id}"))
            .unwrap_or_else(|| "id:None".to_string());
        let widget_str = element
            .content
            .as_ref()
            .map(|w| {
                let type_name = w.type_name();
                // Simplify the type name by taking only the last part after ::
                type_name
                    .split("::")
                    .last()
                    .unwrap_or(type_name)
                    .to_string()
            })
            .unwrap_or_else(|| "Container".to_string());

        trace!("{prefix}{branch} {key:?} {id_str} [{widget_str}]");

        // Prepare prefix for children
        let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });

        // Print children
        let children: Vec<UIKey> = element.children.to_vec();
        for (i, &child_key) in children.iter().enumerate() {
            let is_last_child = i == children.len() - 1;
            Self::debug_print_tree_recursive(ui_tree, child_key, &child_prefix, is_last_child);
        }
    }

    /// Dispatch drag/drop events to widgets based on position
    pub fn dispatch_drag_event(
        &mut self,
        ui_tree: BorrowedUITree<Message>,
        event: &DragEvent,
        position: gfx::PointDIP,
    ) -> Option<DropResult> {
        let mut result = None;
        let mut new_drag_widget = None;
        let prev_drag_widget = self.current_drag_widget;

        // First, handle drag_leave if we're moving away from a widget
        if matches!(event, DragEvent::DragOver { .. }) && prev_drag_widget.is_some() {
            // Check if we need to call drag_leave on the previous widget
            let mut should_call_drag_leave = false;
            let mut found_new_widget = false;

            // Quick check to see if we're still over the same widget or moved to a new one
            visitors::visit_reverse_bfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
                let bounds = ui_tree.slots[key].bounds();

                if position.within(bounds.border_box)
                    && Shell::is_point_visible_in_scroll_ancestors(ui_tree, key, position)
                    && let element = &mut ui_tree.slots[key]
                    && element.content.is_some()
                {
                    if key != prev_drag_widget.unwrap() {
                        should_call_drag_leave = true;
                    }
                    found_new_widget = true;
                    return VisitAction::Exit;
                }
                VisitAction::Continue
            });

            if should_call_drag_leave || !found_new_widget {
                // Call drag_leave on the previous widget
                if let Some(prev_key) = prev_drag_widget
                    && let Some(prev_element) = ui_tree.slots.get_mut(prev_key)
                    && let prev_bounds = prev_element.bounds()
                    && let Some(ref mut prev_widget) = prev_element.content
                    && let Some(prev_text_input) = prev_widget.as_drop_target()
                    && let Some(id) = prev_element.id
                    && let Some(instance) = ui_tree.widget_state.get_mut(&id)
                {
                    prev_text_input.drag_leave(instance, prev_bounds);
                }
            }
        }

        // Now find the widget under the current position and handle the event
        visitors::visit_reverse_bfs(ui_tree, ui_tree.root, |ui_tree, key, _| {
            let bounds = ui_tree.slots[key].bounds();

            // Check if point is within widget bounds (except for DragLeave, which should be handled by all)
            if (position.within(bounds.border_box)
                && Shell::is_point_visible_in_scroll_ancestors(ui_tree, key, position))
                || matches!(event, DragEvent::DragLeave)
            {
                let element = &mut ui_tree.slots[key];
                if let Some(ref mut widget) = element.content {
                    if let Some(text_input) = widget.as_drop_target()
                        && let Some(id) = element.id
                        && let Some(instance) = ui_tree.widget_state.get_mut(&id)
                    {
                        new_drag_widget = Some(key);

                        match event {
                            DragEvent::DragEnter { drag_info } => {
                                let effect = text_input.drag_enter(instance, drag_info, bounds);
                                result = Some(DropResult {
                                    effect,
                                    handled: true,
                                });
                            }
                            DragEvent::DragOver { drag_info } => {
                                if prev_drag_widget != Some(key) {
                                    // Moving to a new widget, call drag_enter
                                    let effect = text_input.drag_enter(instance, drag_info, bounds);
                                    result = Some(DropResult {
                                        effect,
                                        handled: true,
                                    });
                                } else {
                                    // Same widget, call drag_over
                                    let effect = text_input.drag_over(instance, drag_info, bounds);
                                    result = Some(DropResult {
                                        effect,
                                        handled: true,
                                    });
                                }
                            }
                            DragEvent::Drop { drag_info } => {
                                result = Some(text_input.drop(instance, self, drag_info, bounds));

                                // Make sure to reset active element after drop as we dont get mouse-up for this.
                                self.active_element_id = None;
                            }
                            DragEvent::DragLeave => {
                                text_input.drag_leave(instance, bounds);
                            }
                        }

                        if !matches!(event, DragEvent::DragLeave) {
                            return VisitAction::Exit;
                        }
                    }
                }
            }
            VisitAction::Continue
        });

        // Update the current drag widget
        self.current_drag_widget = new_drag_widget;

        result
    }

    /// Captures the event, preventing further traversal.
    ///
    /// No ancestor widget will receive the event.
    ///
    /// Returns true if the event was captured.
    pub fn capture_event(&mut self, id: u64) -> bool {
        if self.event_captured_by.is_some() {
            return false;
        }

        self.event_captured_by = Some(id);
        true
    }

    pub fn request_input_method(&mut self, ime: InputMethod) {
        match self.input_method {
            InputMethod::Disabled => match ime {
                InputMethod::Disabled => { /* Nothing to do */ }
                InputMethod::Enabled { position } => {
                    self.deferred_controls
                        .push(DeferredControl::SetIMEPosition { position });

                    self.input_method = ime;
                }
            },
            InputMethod::Enabled { position } => match ime {
                InputMethod::Disabled => {
                    self.deferred_controls.push(DeferredControl::DisableIME);
                }
                InputMethod::Enabled {
                    position: new_position,
                } => {
                    if position != new_position {
                        self.deferred_controls
                            .push(DeferredControl::SetIMEPosition {
                                position: new_position,
                            });
                    }
                }
            },
        }
    }
}

pub fn current_dpi(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) as f32 }
}

pub fn dips_scale(hwnd: HWND) -> f32 {
    dips_scale_for_dpi(current_dpi(hwnd))
}

pub fn dips_scale_for_dpi(dpi: f32) -> f32 {
    96.0f32 / dpi.max(1.0)
}
