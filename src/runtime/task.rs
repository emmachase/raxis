//! Create runtime tasks.

/*

Copyright 2019 Héctor Ramón, Iced contributors

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

*/

// use crate::Action;
// use crate::core::widget;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::channel::oneshot::Sender;
use futures::future::{self, FutureExt};
use futures::stream::BoxStream;
use futures::stream::{self, Stream, StreamExt};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::FOLDERID_LocalAppData;
use windows::Win32::UI::Shell::FOLDERID_RoamingAppData;
use windows::Win32::UI::Shell::KNOWN_FOLDER_FLAG;
use windows::Win32::UI::Shell::SHGetKnownFolderPath;
use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;
use windows::Win32::UI::WindowsAndMessaging::SC_CLOSE;
use windows::Win32::UI::WindowsAndMessaging::SC_MAXIMIZE;
use windows::Win32::UI::WindowsAndMessaging::SC_MINIMIZE;
use windows::Win32::UI::WindowsAndMessaging::SC_RESTORE;
use windows::Win32::UI::WindowsAndMessaging::WM_SYSCOMMAND;
// use futures::{BoxStream, Send, boxed_stream};

// pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + 'a>>;

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use crate::ContextMenuItem;

pub fn boxed_stream<T, S>(stream: S) -> BoxStream<'static, T>
where
    S: futures::Stream<Item = T> + Send + 'static,
{
    futures::stream::StreamExt::boxed(stream)
}

pub enum ClipboardAction {
    Set(String),
    Get(Sender<Option<String>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    Windowed,
    Hidden,
}

pub enum WindowAction {
    Activate,
    GetMode(Sender<WindowMode>),
    SetMode(WindowMode),
    Minimize,
    ToggleMaximizeRestore,
    Close,
}

pub enum SystemAction {
    GetRoamingAppData(Sender<Option<PathBuf>>),
    GetLocalAppData(Sender<Option<PathBuf>>),
}

pub enum ContextMenuAction {
    Show {
        items: Vec<ContextMenuItem>, // label, message, enabled, checked, is_separator
        position: Option<(i32, i32)>, // None = cursor position
        sender: Sender<Option<usize>>, // sender to send the selected message
    },
}

pub enum Action<T> {
    Output(T),
    Clipboard(ClipboardAction),
    Window(WindowAction),
    System(SystemAction),
    ContextMenu(ContextMenuAction),
    Exit,
}

impl<T> Action<T> {
    // /// Creates a new [`Action::Widget`] with the given [`widget::Operation`].
    // pub fn widget(operation: impl widget::Operation + 'static) -> Self {
    //     Self::Widget(Box::new(operation))
    // }

    fn output<O>(self) -> Result<T, Action<O>> {
        match self {
            Action::Output(output) => Ok(output),
            // Action::LoadFont { bytes, channel } => Err(Action::LoadFont { bytes, channel }),
            // Action::Widget(operation) => Err(Action::Widget(operation)),
            Action::Clipboard(action) => Err(Action::Clipboard(action)),
            Action::Window(action) => Err(Action::Window(action)),
            Action::ContextMenu(action) => Err(Action::ContextMenu(action)),
            Action::System(action) => Err(Action::System(action)),
            // Action::Reload => Err(Action::Reload),
            Action::Exit => Err(Action::Exit),
        }
    }
}

/// A set of concurrent actions to be performed by the iced runtime.
///
/// A [`Task`] _may_ produce a bunch of values of type `T`.
#[must_use = "`Task` must be returned to the runtime to take effect; normally in your `update` or `new` functions."]
pub struct Task<T> {
    stream: Option<BoxStream<'static, Action<T>>>,
    units: usize,
}

impl<T> Task<T> {
    /// Creates a [`Task`] that does nothing.
    pub fn none() -> Self {
        Self {
            stream: None,
            units: 0,
        }
    }

    /// Creates a new [`Task`] that instantly produces the given value.
    pub fn done(value: T) -> Self
    where
        T: Send + 'static,
    {
        Self::future(future::ready(value))
    }

    /// Creates a [`Task`] that runs the given [`Future`] to completion and maps its
    /// output with the given closure.
    pub fn perform<A>(
        future: impl Future<Output = A> + Send + 'static,
        f: impl FnOnce(A) -> T + Send + 'static,
    ) -> Self
    where
        T: Send + 'static,
        A: Send + 'static,
    {
        Self::future(future.map(f))
    }

    /// Creates a [`Task`] that runs the given [`Stream`] to completion and maps each
    /// item with the given closure.
    pub fn run<A>(
        stream: impl Stream<Item = A> + Send + 'static,
        f: impl Fn(A) -> T + Send + 'static,
    ) -> Self
    where
        T: 'static,
    {
        Self::stream(stream.map(f))
    }

    /// Combines the given tasks and produces a single [`Task`] that will run all of them
    /// in parallel.
    pub fn batch(tasks: impl IntoIterator<Item = Self>) -> Self
    where
        T: 'static,
    {
        let mut select_all = stream::SelectAll::new();
        let mut units = 0;

        for task in tasks.into_iter() {
            if let Some(stream) = task.stream {
                select_all.push(stream);
            }

            units += task.units;
        }

        Self {
            stream: Some(boxed_stream(select_all)),
            units,
        }
    }

    /// Maps the output of a [`Task`] with the given closure.
    pub fn map<O>(self, mut f: impl FnMut(T) -> O + Send + 'static) -> Task<O>
    where
        T: Send + 'static,
        O: Send + 'static,
    {
        self.then(move |output| Task::done(f(output)))
    }

    /// Performs a new [`Task`] for every output of the current [`Task`] using the
    /// given closure.
    ///
    /// This is the monadic interface of [`Task`]—analogous to [`Future`] and
    /// [`Stream`].
    pub fn then<O>(self, mut f: impl FnMut(T) -> Task<O> + Send + 'static) -> Task<O>
    where
        T: Send + 'static,
        O: Send + 'static,
    {
        Task {
            stream: match self.stream {
                None => None,
                Some(stream) => Some(boxed_stream(stream.flat_map(move |action| {
                    match action.output() {
                        Ok(output) => f(output)
                            .stream
                            .unwrap_or_else(|| boxed_stream(stream::empty())),
                        Err(action) => boxed_stream(stream::once(async move { action })),
                    }
                }))),
            },
            units: self.units,
        }
    }

    /// Chains a new [`Task`] to be performed once the current one finishes completely.
    pub fn chain(self, task: Self) -> Self
    where
        T: 'static,
    {
        match self.stream {
            None => task,
            Some(first) => match task.stream {
                None => Self {
                    stream: Some(first),
                    units: self.units,
                },
                Some(second) => Self {
                    stream: Some(boxed_stream(first.chain(second))),
                    units: self.units + task.units,
                },
            },
        }
    }

    /// Creates a new [`Task`] that collects all the output of the current one into a [`Vec`].
    pub fn collect(self) -> Task<Vec<T>>
    where
        T: Send + 'static,
    {
        match self.stream {
            None => Task::done(Vec::new()),
            Some(stream) => Task {
                stream: Some(boxed_stream(
                    stream::unfold(
                        (stream, Some(Vec::new())),
                        move |(mut stream, outputs)| async move {
                            let mut outputs = outputs?;

                            let Some(action) = stream.next().await else {
                                return Some((Some(Action::Output(outputs)), (stream, None)));
                            };

                            match action.output() {
                                Ok(output) => {
                                    outputs.push(output);

                                    Some((None, (stream, Some(outputs))))
                                }
                                Err(action) => Some((Some(action), (stream, Some(outputs)))),
                            }
                        },
                    )
                    .filter_map(future::ready),
                )),
                units: self.units,
            },
        }
    }

    /// Creates a new [`Task`] that discards the result of the current one.
    ///
    /// Useful if you only care about the side effects of a [`Task`].
    pub fn discard<O>(self) -> Task<O>
    where
        T: Send + 'static,
        O: Send + 'static,
    {
        self.then(|_| Task::none())
    }

    /// Creates a new [`Task`] that consumes the result of the current one and produces no output.
    pub fn consume<O>(self, f: impl Fn(T) + Send + 'static) -> Task<O>
    where
        T: Send + 'static,
        O: Send + 'static,
    {
        self.then(move |t| {
            (f)(t);
            Task::none()
        })
    }

    /// Creates a new [`Task`] that can be aborted with the returned [`Handle`].
    pub fn abortable(self) -> (Self, Handle)
    where
        T: 'static,
    {
        let (stream, handle) = match self.stream {
            Some(stream) => {
                let (stream, handle) = stream::abortable(stream);

                (Some(boxed_stream(stream)), InternalHandle::Manual(handle))
            }
            None => (
                None,
                InternalHandle::Manual(stream::AbortHandle::new_pair().0),
            ),
        };

        (
            Self {
                stream,
                units: self.units,
            },
            Handle { internal: handle },
        )
    }

    /// Creates a new [`Task`] that runs the given [`Future`] and produces
    /// its output.
    pub fn future(future: impl Future<Output = T> + Send + 'static) -> Self
    where
        T: 'static,
    {
        Self::stream(stream::once(future))
    }

    /// Creates a new [`Task`] that runs the given [`Stream`] and produces
    /// each of its items.
    pub fn stream(stream: impl Stream<Item = T> + Send + 'static) -> Self
    where
        T: 'static,
    {
        Self {
            stream: Some(boxed_stream(stream.map(Action::Output))),
            units: 1,
        }
    }

    /// Returns the amount of work "units" of the [`Task`].
    pub fn units(&self) -> usize {
        self.units
    }
}

impl<T> std::fmt::Debug for Task<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(&format!("Task<{}>", std::any::type_name::<T>()))
            .field("units", &self.units)
            .finish()
    }
}

/// A handle to a [`Task`] that can be used for aborting it.
#[derive(Debug, Clone)]
pub struct Handle {
    internal: InternalHandle,
}

#[derive(Debug, Clone)]
enum InternalHandle {
    Manual(stream::AbortHandle),
    AbortOnDrop(Arc<stream::AbortHandle>),
}

impl InternalHandle {
    pub fn as_ref(&self) -> &stream::AbortHandle {
        match self {
            InternalHandle::Manual(handle) => handle,
            InternalHandle::AbortOnDrop(handle) => handle.as_ref(),
        }
    }
}

impl Handle {
    /// Aborts the [`Task`] of this [`Handle`].
    pub fn abort(&self) {
        self.internal.as_ref().abort();
    }

    /// Returns a new [`Handle`] that will call [`Handle::abort`] whenever
    /// all of its instances are dropped.
    ///
    /// If a [`Handle`] is cloned, [`Handle::abort`] will only be called
    /// once all of the clones are dropped.
    ///
    /// This can be really useful if you do not want to worry about calling
    /// [`Handle::abort`] yourself.
    pub fn abort_on_drop(self) -> Self {
        match &self.internal {
            InternalHandle::Manual(handle) => Self {
                internal: InternalHandle::AbortOnDrop(Arc::new(handle.clone())),
            },
            InternalHandle::AbortOnDrop(_) => self,
        }
    }

    /// Returns `true` if the [`Task`] of this [`Handle`] has been aborted.
    pub fn is_aborted(&self) -> bool {
        self.internal.as_ref().is_aborted()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if let InternalHandle::AbortOnDrop(handle) = &mut self.internal {
            let handle = std::mem::replace(handle, Arc::new(stream::AbortHandle::new_pair().0));

            if let Some(handle) = Arc::into_inner(handle) {
                handle.abort();
            }
        }
    }
}

impl<T> Task<Option<T>> {
    /// Executes a new [`Task`] after this one, only when it produces `Some` value.
    ///
    /// The value is provided to the closure to create the subsequent [`Task`].
    pub fn and_then<A>(self, f: impl Fn(T) -> Task<A> + Send + 'static) -> Task<A>
    where
        T: Send + 'static,
        A: Send + 'static,
    {
        self.then(move |option| option.map_or_else(Task::none, &f))
    }

    /// Creates a new [`Task`] that consumes the result of the current one and produces
    /// no output, only when it produces a `Some` value.
    pub fn and_consume<O>(self, f: impl Fn(T) + Send + 'static) -> Task<O>
    where
        T: Send + 'static,
        O: Send + 'static,
    {
        self.and_then(move |t| {
            (f)(t);
            Task::none()
        })
    }
}

impl<T, E> Task<Result<T, E>> {
    /// Executes a new [`Task`] after this one, only when it succeeds with an `Ok` value.
    ///
    /// The success value is provided to the closure to create the subsequent [`Task`].
    pub fn and_then<A>(self, f: impl Fn(T) -> Task<A> + Send + 'static) -> Task<A>
    where
        T: Send + 'static,
        E: Send + 'static,
        A: Send + 'static,
    {
        self.then(move |option| option.map_or_else(|_| Task::none(), &f))
    }

    /// Creates a new [`Task`] that consumes the result of the current one and produces
    /// no output, only when it produces an `Ok` value.
    pub fn and_consume<O>(self, f: impl Fn(T) + Send + 'static) -> Task<O>
    where
        T: Send + 'static,
        E: Send + 'static,
        O: Send + 'static,
    {
        self.and_then(move |t| {
            (f)(t);
            Task::none()
        })
    }
}

impl<T> Default for Task<T> {
    fn default() -> Self {
        Self::none()
    }
}

impl<T> From<()> for Task<T> {
    fn from(_value: ()) -> Self {
        Self::none()
    }
}

// /// Creates a new [`Task`] that runs the given [`widget::Operation`] and produces
// /// its output.
// pub fn widget<T>(operation: impl widget::Operation<T> + 'static) -> Task<T>
// where
//     T: Send + 'static,
// {
//     channel(move |sender| {
//         let operation = widget::operation::map(Box::new(operation), move |value| {
//             let _ = sender.clone().try_send(value);
//         });

//         Action::Widget(Box::new(operation))
//     })
// }

/// Creates a new [`Task`] that executes the [`Action`] returned by the closure and
/// produces the value fed to the [`oneshot::Sender`].
pub fn oneshot<T>(f: impl FnOnce(oneshot::Sender<T>) -> Action<T>) -> Task<T>
where
    T: Send + 'static,
{
    let (sender, receiver) = oneshot::channel();

    let action = f(sender);

    Task {
        stream: Some(boxed_stream(
            stream::once(async move { action }).chain(
                receiver
                    .into_stream()
                    .filter_map(|result| async move { Some(Action::Output(result.ok()?)) }),
            ),
        )),
        units: 1,
    }
}

/// Creates a new [`Task`] that executes the [`Action`] returned by the closure and
/// produces the values fed to the [`mpsc::Sender`].
pub fn channel<T>(f: impl FnOnce(mpsc::Sender<T>) -> Action<T>) -> Task<T>
where
    T: Send + 'static,
{
    let (sender, receiver) = mpsc::channel(1);

    let action = f(sender);

    Task {
        stream: Some(boxed_stream(
            stream::once(async move { action })
                .chain(receiver.map(|result| Action::Output(result))),
        )),
        units: 1,
    }
}

/// Creates a new [`Task`] that executes the given [`Action`] and produces no output.
pub fn effect<T>(action: impl Into<Action<Infallible>>) -> Task<T> {
    let _action = action.into();

    Task {
        stream: Some(boxed_stream(stream::once(async move {
            _action.output().expect_err("no output")
        }))),
        units: 1,
    }
}

/// Creates a new [`Task`] that sets the window mode.
pub fn set_window_mode<T>(mode: WindowMode) -> Task<T> {
    effect(Action::Window(WindowAction::SetMode(mode)))
}

/// Creates a new [`Task`] that gets the window mode.
pub fn get_window_mode() -> Task<WindowMode> {
    oneshot(|sender| Action::Window(WindowAction::GetMode(sender)))
}

/// Creates a new [`Task`] that activates the window.
pub fn activate_window<T>() -> Task<T> {
    effect(Action::Window(WindowAction::Activate))
}

/// Creates a new [`Task`] that exits the application.
pub fn exit_application<T>() -> Task<T> {
    effect(Action::Exit)
}

/// Creates a new [`Task`] that shows the window.
pub fn show_window<T: 'static>() -> Task<T> {
    set_window_mode(WindowMode::Windowed).chain(activate_window())
}

/// Creates a new [`Task`] that hides the window.
pub fn hide_window<T>() -> Task<T> {
    set_window_mode(WindowMode::Hidden)
}

/// Creates a new [`Task`] that minimizes the window.
pub fn minimize_window<T>() -> Task<T> {
    effect(Action::Window(WindowAction::Minimize))
}

/// Creates a new [`Task`] that toggles between maximized and restored window state.
pub fn toggle_maximize_window<T>() -> Task<T> {
    effect(Action::Window(WindowAction::ToggleMaximizeRestore))
}

/// Creates a new [`Task`] that closes the window.
pub fn close_window<T>() -> Task<T> {
    effect(Action::Window(WindowAction::Close))
}

/// Returns the underlying [`Stream`] of the [`Task`].
pub fn into_stream<T>(task: Task<T>) -> Option<BoxStream<'static, Action<T>>> {
    task.stream
}

/// Creates a new [`Task`] that shows a context menu at the cursor position.
///
/// The task completes with `Some(message)` if an item is selected, or `None` if cancelled.
///
/// # Example
/// ```ignore
/// show_context_menu(vec![
///     ("Copy", Message::Copy, true, false, false),
///     ("Paste", Message::Paste, true, false, false),
///     ("", Message::default(), true, false, true), // separator
///     ("Delete", Message::Delete, true, false, false),
/// ])
/// ```
pub fn show_context_menu<T: Clone + Send + Sync + 'static>(
    items: Vec<(Option<T>, ContextMenuItem)>,
    cancel: T,
) -> Task<T> {
    oneshot({
        let items = items.clone();
        move |sender| {
            Action::ContextMenu(ContextMenuAction::Show {
                items: items.into_iter().map(|(_message, item)| item).collect(),
                position: None,
                sender,
            })
        }
    })
    .map(move |index| {
        if let Some(index) = index {
            items[index].0.clone()
        } else {
            Some(cancel.clone())
        }
    })
    .and_then(|option| Task::done(option))
}

// /// Creates a new [`Task`] that shows a context menu at a specific screen position.
// ///
// /// The task completes with `Some(message)` if an item is selected, or `None` if cancelled.
// pub fn show_context_menu_at<T>(
//     items: Vec<(String, T, bool, bool, bool)>,
//     cancel: T,
//     x: i32,
//     y: i32,
// ) -> Task<T>
// where
//     T: Send + 'static,
// {
//     oneshot::<T>(|sender| {
//         Action::ContextMenu(ContextMenuAction::Show {
//             items,
//             position: Some((x, y)),
//             sender,
//         })
//     })
// }

/// Creates a new [`Task`] that gets the roaming app data directory.
pub fn get_roaming_app_data() -> Task<Option<PathBuf>> {
    oneshot(|sender| Action::System(SystemAction::GetRoamingAppData(sender)))
}

/// Creates a new [`Task`] that gets the local app data directory.
pub fn get_local_app_data() -> Task<Option<PathBuf>> {
    oneshot(|sender| Action::System(SystemAction::GetLocalAppData(sender)))
}

/// Runs the task executor loop on an async runtime
pub fn run_task_executor<Message: Send + Clone + 'static>(
    task_receiver: std::sync::mpsc::Receiver<Task<Message>>,
    message_sender: std::sync::mpsc::Sender<Message>,
    hwnd: crate::runtime::UncheckedHWND,
) {
    use crate::runtime::clipboard;
    use crate::runtime::context_menu::ContextMenu;
    use futures::StreamExt;
    use std::sync::atomic::Ordering;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        IsZoomed, PostMessageW, SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOW,
        ShowWindow, WM_CLOSE,
    };

    async fn process_task_stream<Message: Send + Clone + 'static>(
        stream: impl futures::Stream<Item = Action<Message>> + Send + Unpin + 'static,
        message_sender: std::sync::mpsc::Sender<Message>,
        hwnd: crate::runtime::UncheckedHWND,
    ) {
        let mut stream = stream;
        while let Some(action) = stream.next().await {
            match action {
                Action::Output(message) => {
                    // Send message to channel for UI thread processing
                    let _ = message_sender.send(message);

                    // If the UI thread is not processing messages, notify it
                    if !crate::runtime::app_handle::PENDING_MESSAGE_PROCESSING
                        .swap(true, Ordering::SeqCst)
                    {
                        unsafe {
                            PostMessageW(
                                Some(hwnd.0),
                                crate::runtime::WM_ASYNC_MESSAGE,
                                WPARAM(0),
                                LPARAM(0),
                            )
                            .ok();
                        }
                    }
                }
                Action::Clipboard(action) => match action {
                    ClipboardAction::Set(text) => {
                        let _ = clipboard::set_clipboard_text(hwnd.0, &text);
                    }
                    ClipboardAction::Get(sender) => {
                        let text = clipboard::get_clipboard_text(hwnd.0);
                        let _ = sender.send(text);
                    }
                },
                Action::Window(action) => match action {
                    WindowAction::Activate => unsafe {
                        SetForegroundWindow(hwnd.0).ok().ok();
                    },
                    WindowAction::GetMode(sender) => unsafe {
                        let _ = sender.send(match IsWindowVisible(hwnd.0).as_bool() {
                            true => WindowMode::Windowed,
                            false => WindowMode::Hidden,
                        });
                    },
                    WindowAction::SetMode(mode) => unsafe {
                        let show_cmd = match mode {
                            WindowMode::Windowed => SW_SHOW,
                            WindowMode::Hidden => SW_HIDE,
                        };
                        ShowWindow(hwnd.0, show_cmd).ok().ok();
                    },
                    WindowAction::Minimize => unsafe {
                        PostMessageW(Some(hwnd.0), WM_SYSCOMMAND, WPARAM(SC_MINIMIZE as usize), LPARAM(0)).ok();
                    },
                    WindowAction::ToggleMaximizeRestore => unsafe {
                        let is_maximized = IsZoomed(hwnd.0).as_bool();
                        let show_cmd = if is_maximized { SC_RESTORE } else { SC_MAXIMIZE };
                        PostMessageW(Some(hwnd.0), WM_SYSCOMMAND, WPARAM(show_cmd as usize), LPARAM(0)).ok();
                    },
                    WindowAction::Close => unsafe {
                        PostMessageW(Some(hwnd.0), WM_SYSCOMMAND, WPARAM(SC_CLOSE as usize), LPARAM(0)).ok();
                    },
                },
                Action::System(action) => {
                    // Determine folder ID before extracting sender
                    let folder_id = match &action {
                        SystemAction::GetRoamingAppData(_) => &FOLDERID_RoamingAppData,
                        SystemAction::GetLocalAppData(_) => &FOLDERID_LocalAppData,
                    };

                    // Extract sender
                    let sender = match action {
                        SystemAction::GetRoamingAppData(sender)
                        | SystemAction::GetLocalAppData(sender) => sender,
                    };

                    let Ok(co_path) =
                        (unsafe { SHGetKnownFolderPath(folder_id, KNOWN_FOLDER_FLAG(0), None) })
                    else {
                        let _ = sender.send(None);
                        continue;
                    };

                    let Ok(path) = (unsafe { co_path.to_string() }) else {
                        let _ = sender.send(None);
                        continue;
                    };
                    unsafe { CoTaskMemFree(Some(co_path.0 as _)) };

                    let _ = sender.send(Some(PathBuf::from(path)));
                }
                Action::ContextMenu(ContextMenuAction::Show {
                    items,
                    position,
                    sender,
                }) => {
                    // Build the context menu from items
                    let menu = ContextMenu::new(items);

                    // Request the menu to be shown on the UI thread and await the result
                    let result = menu.show_async(hwnd, position).await;

                    // Send the result back through the channel
                    let _ = sender.send(result);
                }
                Action::Exit => unsafe {
                    PostMessageW(Some(hwnd.0), WM_CLOSE, WPARAM(0), LPARAM(0)).ok();
                },
            }
        }
    }

    async fn run_task_loop<Message: Send + Clone + 'static>(
        task_receiver: std::sync::mpsc::Receiver<Task<Message>>,
        message_sender: std::sync::mpsc::Sender<Message>,
        hwnd: crate::runtime::UncheckedHWND,
    ) {
        while let Ok(task) = task_receiver.recv() {
            if let Some(stream) = into_stream(task) {
                let message_sender = message_sender.clone();

                #[cfg(all(feature = "smol-runtime", not(feature = "tokio")))]
                smol::spawn(process_task_stream(stream, message_sender, hwnd)).detach();

                #[cfg(feature = "tokio")]
                tokio::spawn(process_task_stream(stream, message_sender, hwnd));
            }
        }
    }

    #[cfg(all(feature = "smol-runtime", not(feature = "tokio")))]
    smol::block_on(run_task_loop(task_receiver, message_sender, hwnd));

    #[cfg(feature = "tokio")]
    {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");
        rt.block_on(run_task_loop(task_receiver, message_sender, hwnd));
    }
}
