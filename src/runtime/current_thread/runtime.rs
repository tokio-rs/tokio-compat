use super::{compat, idle, Builder};

use tokio_02::{
    runtime::Handle as Handle02,
    task::{JoinHandle, LocalSet},
};
use tokio_executor_01 as executor_01;
use tokio_reactor_01 as reactor_01;
use tokio_timer_02 as timer_02;

use futures_01::future::Future as Future01;
use futures_util::{compat::Future01CompatExt, future::FutureExt};
use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::io;

/// Single-threaded runtime provides a way to start reactor
/// and executor on the current thread.
///
/// See [module level][mod] documentation for more details.
///
/// [mod]: index.html
#[derive(Debug)]
pub struct Runtime {
    inner: tokio_02::runtime::Runtime,
    local: LocalSet,
    idle: idle::Idle,
    idle_rx: idle::Rx,

    /// Compatibility background thread.
    ///
    /// This maintains a `tokio` 0.1 timer and reactor to support running
    /// futures that use older tokio APIs.
    compat: compat::Background,
}

/// Handle to spawn a future on the corresponding `CurrentThread` runtime instance
#[derive(Debug, Clone)]
pub struct Handle {
    inner: Handle02,
    idle: idle::Idle,
}

thread_local! {
    static IS_CURRENT: Cell<bool> = Cell::new(false);
}

impl Handle {
    /// Spawn a `futures` 0.1 future onto the `CurrentThread` runtime instance
    /// corresponding to this handle.
    ///
    /// Note that unlike the `tokio` 0.1 version of this function, this method
    /// never actually returns `Err` — it returns `Result` only for API
    /// compatibility.
    pub fn spawn<F>(&self, future: F) -> Result<(), executor_01::SpawnError>
    where
        F: Future01<Item = (), Error = ()> + Send + 'static,
    {
        let future = future.compat().map(|_| ());
        self.spawn_std(future);
        Ok(())
    }

    /// Spawn a `std::future` future onto the `CurrentThread` runtime instance
    /// corresponding to this handle.
    pub fn spawn_std<F>(&self, future: F) -> Result<(), executor_01::SpawnError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let idle = self.idle.reserve();
        self.inner.spawn(idle.with(future));
        Ok(())
    }

    /// Spawn a `futures` 0.1 future onto the Tokio runtime, returning a
    /// `JoinHandle` that can be used to await its result.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// **Note** that futures spawned in this manner do not "count" towards
    /// keeping the runtime active for [`shutdown_on_idle`], since they are paired
    /// with a `JoinHandle` for  awaiting their completion.
    ///
    /// [mod]: index.html
    /// [`shutdown_on_idle`]: struct.Runtime.html#method.shutdown_on_idle
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    /// # fn dox() {
    /// // Create the runtime
    /// let rt = Runtime::new().unwrap();
    /// let executor = rt.executor();
    ///
    /// // Spawn a `futures` 0.1 future onto the runtime
    /// executor.spawn(futures_01::future::lazy(|| {
    ///     println!("now running on a worker thread");
    ///     Ok(())
    /// }));
    /// # }
    /// ```
    pub fn spawn_handle<F>(&self, future: F) -> JoinHandle<Result<F::Item, F::Error>>
    where
        F: Future01 + Send + 'static,
        F::Item: Send + 'static,
        F::Error: Send + 'static,
    {
        let future = Box::pin(future.compat());
        self.spawn_handle_std(future)
    }

    /// Spawn a `std::future` future onto the Tokio runtime, returning a
    /// `JoinHandle` that can be used to await its result.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// **Note** that futures spawned in this manner do not "count" towards
    /// keeping the runtime active for [`shutdown_on_idle`], since they are paired
    /// with a `JoinHandle` for  awaiting their completion.
    ///
    /// [mod]: index.html
    /// [`shutdown_on_idle`]: struct.Runtime.html#method.shutdown_on_idle
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let rt = Runtime::new().unwrap();
    /// let executor = rt.executor();
    ///
    /// // Spawn a `std::future` future onto the runtime
    /// executor.spawn_std(async {
    ///     println!("now running on a worker thread");
    /// });
    /// # }
    /// ```
    pub fn spawn_handle_std<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn(future)
    }

    /// Provides a best effort **hint** to whether or not `spawn` will succeed.
    ///
    /// This function may return both false positives **and** false negatives.
    /// If `status` returns `Ok`, then a call to `spawn` will *probably*
    /// succeed, but may fail. If `status` returns `Err`, a call to `spawn` will
    /// *probably* fail, but may succeed.
    ///
    /// This allows a caller to avoid creating the task if the call to `spawn`
    /// has a high likelihood of failing.
    pub fn status(&self) -> Result<(), executor_01::SpawnError> {
        Ok(())
    }
}

/// Error returned by the `run` function.
#[derive(Debug)]
pub struct RunError {
    inner: (),
}

impl fmt::Display for RunError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "RunError")
    }
}

impl Error for RunError {}

impl Runtime {
    /// Returns a new runtime initialized with default configuration values.
    pub fn new() -> io::Result<Runtime> {
        Builder::new().build()
    }

    pub(super) fn new2(
        inner: tokio_02::runtime::Runtime,
        idle: idle::Idle,
        idle_rx: idle::Rx,
        compat: compat::Background,
    ) -> Self {
        Self {
            inner,
            idle,
            idle_rx,
            compat,
            local: tokio_02::task::LocalSet::new(),
        }
    }

    /// Get a new handle to spawn futures on the single-threaded Tokio runtime
    ///
    /// Different to the runtime itself, the handle can be sent to different
    /// threads.
    pub fn handle(&self) -> Handle {
        let inner = self.inner.handle().clone();
        let idle = self.idle.clone();
        Handle { inner, idle }
    }

    /// Spawn a `futures` 0.1 future onto the single-threaded Tokio runtime.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::current_thread::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let mut rt = Runtime::new().unwrap();
    ///
    /// // Spawn a future onto the runtime
    /// rt.spawn(futures_01::future::lazy(|| {
    ///     println!("now running on a worker thread");
    ///     Ok(())
    /// }));
    /// # }
    /// ```
    pub fn spawn<F>(&mut self, future: F) -> &mut Self
    where
        F: Future01<Item = (), Error = ()> + 'static,
    {
        let future = future.compat().map(|_| ());
        self.spawn_std(future)
    }

    /// Spawn a `std::future` future onto the single-threaded Tokio runtime.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::current_thread::Runtime;
    ///
    /// # fn dox() {
    /// // Create the runtime
    /// let mut rt = Runtime::new().unwrap();
    ///
    /// // Spawn a future onto the runtime
    /// rt.spawn_std(async {
    ///     println!("now running on a worker thread");
    /// });
    /// # }
    /// ```
    pub fn spawn_std<F>(&mut self, future: F) -> &mut Self
    where
        F: Future<Output = ()> + 'static,
    {
        let idle = self.idle.reserve();
        self.local.spawn_local(idle.with(future));
        self
    }

    /// Runs the provided `futures` 0.1 future, blocking the current thread
    /// until the future completes.
    ///
    /// This function can be used to synchronously block the current thread
    /// until the provided `future` has resolved either successfully or with an
    /// error. The result of the future is then returned from this function
    /// call.
    ///
    /// Note that this function will **also** execute any spawned futures on the
    /// current thread, but will **not** block until these other spawned futures
    /// have completed. Once the function returns, any uncompleted futures
    /// remain pending in the `Runtime` instance. These futures will not run
    /// until `block_on` or `run` is called again.
    ///
    /// The caller is responsible for ensuring that other spawned futures
    /// complete execution by calling `block_on` or `run`.
    pub fn block_on<F>(&mut self, f: F) -> Result<F::Item, F::Error>
    where
        F: Future01 + 'static,
    {
        self.block_on_std(f.compat())
    }

    /// Runs the provided `std::future` future, blocking the current thread
    /// until the future completes.
    ///
    /// This function can be used to synchronously block the current thread
    /// until the provided `future` has resolved either successfully or with an
    /// error. The result of the future is then returned from this function
    /// call.
    ///
    /// Note that this function will **also** execute any spawned futures on the
    /// current thread, but will **not** block until these other spawned futures
    /// have completed. Once the function returns, any uncompleted futures
    /// remain pending in the `Runtime` instance. These futures will not run
    /// until `block_on` or `run` is called again.
    ///
    /// The caller is responsible for ensuring that other spawned futures
    /// complete execution by calling `block_on` or `run`.
    pub fn block_on_std<F>(&mut self, f: F) -> F::Output
    where
        F: Future + 'static,
    {
        let handle = self.inner.handle().clone();
        let Runtime {
            ref mut local,
            ref mut inner,
            ref compat,
            ref idle,
            ..
        } = *self;
        let mut spawner = compat::CompatSpawner::new(handle, &idle);
        let mut enter = executor_01::enter().unwrap();
        // Set the default tokio 0.1 reactor to the background compat reactor.
        let _reactor = reactor_01::set_default(compat.reactor());
        let _timer = timer_02::timer::set_default(compat.timer());
        executor_01::with_default(&mut spawner, &mut enter, |_enter| {
            mark_current(move || local.block_on(inner, f))
        })
    }

    /// Run the executor to completion, blocking the thread until **all**
    /// spawned futures have completed.
    pub fn run(&mut self) -> Result<(), RunError> {
        let handle = self.inner.handle().clone();
        let Runtime {
            ref mut local,
            ref mut inner,
            ref compat,
            ref mut idle_rx,
            ref idle,
            ..
        } = *self;
        let mut spawner = compat::CompatSpawner::new(handle, &idle);
        let mut enter = executor_01::enter().unwrap();
        // Set the default tokio 0.1 reactor to the background compat reactor.
        let _reactor = reactor_01::set_default(compat.reactor());
        let _timer = timer_02::timer::set_default(compat.timer());

        executor_01::with_default(&mut spawner, &mut enter, |_enter| {
            mark_current(move || local.block_on(inner, idle_rx.recv()))
        });
        Ok(())
    }
}

pub(super) fn is_current() -> bool {
    IS_CURRENT.try_with(|c| c.get()).unwrap_or(false)
}

fn mark_current<T>(f: impl FnOnce() -> T) -> T {
    struct Reset(());

    impl Drop for Reset {
        fn drop(&mut self) {
            // this may fail if we're already panicking, ignore that.
            let _ = IS_CURRENT.try_with(|c| {
                c.set(false);
            });
        }
    }

    let was_current = IS_CURRENT.with(|c| c.replace(true));
    assert_eq!(was_current, false, "entered current_thread runtime twice!");
    let reset = Reset(());
    f()
}
