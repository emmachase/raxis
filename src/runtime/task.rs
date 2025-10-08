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
// use futures::{BoxStream, Send, boxed_stream};

// pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + 'a>>;

use std::convert::Infallible;
use std::sync::Arc;

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
    SetMode(WindowMode),
}

pub enum Action<T> {
    Output(T),
    Clipboard(ClipboardAction),
    Window(WindowAction),
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
            // Action::System(action) => Err(Action::System(action)),
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

/// Creates a new [`Task`] that activates the window.
pub fn activate_window<T>() -> Task<T> {
    effect(Action::Window(WindowAction::Activate))
}

/// Creates a new [`Task`] that shows the window.
pub fn show_window<T: 'static>() -> Task<T> {
    set_window_mode(WindowMode::Windowed).chain(activate_window())
}

/// Creates a new [`Task`] that hides the window.
pub fn hide_window<T>() -> Task<T> {
    set_window_mode(WindowMode::Hidden)
}

/// Returns the underlying [`Stream`] of the [`Task`].
pub fn into_stream<T>(task: Task<T>) -> Option<BoxStream<'static, Action<T>>> {
    task.stream
}
