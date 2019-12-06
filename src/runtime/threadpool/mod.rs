mod builder;
mod task_executor;

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::task_executor::TaskExecutor;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use builder::Builder;

use super::{
    compat::{self, CompatSpawner},
    idle,
};

use futures_01::future::Future as Future01;
use futures_util::{compat::Future01CompatExt, future::FutureExt};
use std::{future::Future, io};
use tokio_02::{
    runtime::{self, Handle},
    task::JoinHandle,
};
use tokio_executor_01 as executor_01;
use tokio_reactor_01 as reactor_01;

use tokio_timer_02 as timer_02;

/// A thread pool runtime that can run tasks that use both `tokio` 0.1 and
/// `tokio` 0.2 APIs.
///
/// This functions similarly to the [`tokio::runtime::Runtime`][rt] struct in the
/// `tokio` crate. However, unlike that runtime, the `tokio-compat` runtime is
/// capable of running both `std::future::Future` tasks that use `tokio` 0.2
/// runtime services. and `futures` 0.1 tasks that use `tokio` 0.1 runtime
/// services.
///
/// [rt]: https://docs.rs/tokio/0.2.0-alpha.6/tokio/runtime/struct.Runtime.html
#[derive(Debug)]
pub struct Runtime {
    /// The actual runtime. This is in an option so that it can be dropped when
    /// shutting down.
    inner: Option<Inner>,

    /// Idleness tracking.
    idle: idle::Idle,
    idle_rx: idle::Rx,
}

#[derive(Debug)]
struct Inner {
    runtime: runtime::Runtime,

    /// Compatibility background thread.
    ///
    /// This maintains a `tokio` 0.1 timer and reactor to support running
    /// futures that use older tokio APIs.
    compat_bg: compat::Background,
}

// ===== impl Runtime =====

/// Start the Tokio runtime using the supplied `futures` 0.1 future to bootstrap
/// execution.
///
/// This function is used to bootstrap the execution of a Tokio application. It
/// does the following:
///
/// * Start the Tokio runtime using a default configuration.
/// * Spawn the given future onto the thread pool.
/// * Block the current thread until the runtime shuts down.
///
/// Note that the function will not return immediately once `future` has
/// completed. Instead it waits for the entire runtime to become idle.
///
/// See the [module level][mod] documentation for more details.
///
/// # Examples
///
/// ```rust
/// use futures_01::{Future as Future01, Stream as Stream01};
/// use tokio_01::net::TcpListener;
///
/// # fn process<T>(_: T) -> Box<dyn Future01<Item = (), Error = ()> + Send> {
/// # unimplemented!();
/// # }
/// # fn dox() {
/// # let addr = "127.0.0.1:8080".parse().unwrap();
/// let listener = TcpListener::bind(&addr).unwrap();
///
/// let server = listener.incoming()
///     .map_err(|e| println!("error = {:?}", e))
///     .for_each(|socket| {
///         tokio_01::spawn(process(socket))
///     });
///
/// tokio_compat::run(server);
/// # }
/// # pub fn main() {}
/// ```
///
/// # Panics
///
/// This function panics if called from the context of an executor.
///
/// [mod]: ../index.html
pub fn run<F>(future: F)
where
    F: Future01<Item = (), Error = ()> + Send + 'static,
{
    let runtime = Runtime::new().expect("failed to start new Runtime");
    runtime.spawn(future);
    runtime.shutdown_on_idle();
}

/// Start the Tokio runtime using the supplied `std::future` future to bootstrap
/// execution.
///
/// This function is used to bootstrap the execution of a Tokio application. It
/// does the following:
///
/// * Start the Tokio runtime using a default configuration.
/// * Spawn the given future onto the thread pool.
/// * Block the current thread until the runtime shuts down.
///
/// Note that the function will not return immediately once `future` has
/// completed. Instead it waits for the entire runtime to become idle.
///
/// See the [module level][mod] documentation for more details.
///
/// # Examples
///
/// ```rust
/// use futures_01::{Future as Future01, Stream as Stream01};
/// use tokio_01::net::TcpListener;
///
/// # fn process<T>(_: T) -> Box<dyn Future01<Item = (), Error = ()> + Send> {
/// # unimplemented!();
/// # }
/// # fn dox() {
/// # let addr = "127.0.0.1:8080".parse().unwrap();
/// let listener = TcpListener::bind(&addr).unwrap();
///
/// let server = listener.incoming()
///     .map_err(|e| println!("error = {:?}", e))
///     .for_each(|socket| {
///         tokio_01::spawn(process(socket))
///     });
///
/// tokio_compat::run(server);
/// # }
/// # pub fn main() {}
/// ```
///
/// # Panics
///
/// This function panics if called from the context of an executor.
///
/// [mod]: ../index.html
pub fn run_std<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let mut runtime = Runtime::new().expect("failed to start new Runtime");
    runtime.spawn(futures_01::future::lazy(|| Ok(())));
    runtime.block_on_std(future);
    runtime.shutdown_on_idle();
}

impl Runtime {
    /// Create a new runtime instance with default configuration values.
    ///
    /// This results in a reactor, thread pool, and timer being initialized. The
    /// thread pool will not spawn any worker threads until it needs to, i.e.
    /// tasks are scheduled to run.
    ///
    /// Most users will not need to call this function directly, instead they
    /// will use [`tokio_compat::run`](fn.run.html).
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// # Examples
    ///
    /// Creating a new `Runtime` with default configuration values.
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    /// ```
    ///
    /// [mod]: index.html
    pub fn new() -> io::Result<Self> {
        Builder::new().build()
    }

    /// Return a handle to the runtime's executor.
    ///
    /// The returned handle can be used to spawn both `futures` 0.1 and
    /// `std::future` tasks that run on this runtime.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// let executor_handle = rt.executor();
    ///
    /// // use `executor_handle`
    /// ```
    pub fn executor(&self) -> TaskExecutor {
        let inner = self.spawner();
        TaskExecutor { inner }
    }

    /// Spawn a `futures` 0.1 future onto the Tokio runtime.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// fn main() {
    ///    // Create the runtime
    ///    let rt = Runtime::new().unwrap();
    ///
    ///    // Spawn a future onto the runtime
    ///    rt.spawn(futures_01::future::lazy(|| {
    ///        println!("now running on a worker thread");
    ///        Ok(())
    ///    }));
    ///
    ///     rt.shutdown_on_idle();
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn<F>(&self, future: F) -> &Self
    where
        F: Future01<Item = (), Error = ()> + Send + 'static,
    {
        self.spawn_std(future.compat().map(|_| ()))
    }

    /// Spawn a `std::future` future onto the Tokio runtime.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// fn main() {
    ///    // Create the runtime
    ///    let rt = Runtime::new().unwrap();
    ///
    ///    // Spawn a future onto the runtime
    ///    rt.spawn_std(async {
    ///        println!("now running on a worker thread");
    ///    });
    ///
    ///     rt.shutdown_on_idle();
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn_std<F>(&self, future: F) -> &Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let idle = self.idle.reserve();
        self.inner().runtime.spawn(idle.with(future));
        self
    }

    /// Spawn a `futures` 0.1 future onto the Tokio runtime, returning a
    /// `JoinHandle` that can be used to await its result.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// **Note** that futures spawned in this manner do not "count" towards
    /// `shutdown_on_idle`, since they are paired with a `JoinHandle` for
    /// awaiting their completion.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// [mod]: index.html
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
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
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
    /// **Note** that futures spawned in this manner do not "count" towards
    /// `shutdown_on_idle`, since they are paired with a `JoinHandle` for
    /// awaiting their completion.
    ///
    /// See [module level][mod] documentation for more details.
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
        self.inner().runtime.spawn(future)
    }

    /// Run a `futures` 0.1 future to completion on the Tokio runtime.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers which
    /// the future spawns internally will be executed on the runtime.
    ///
    /// This method should not be called from an asynchronous context.
    ///
    /// # Panics
    ///
    /// This function panics if the executor is at capacity, if the provided
    /// future panics, or if called within an asynchronous execution context.
    pub fn block_on<F>(&mut self, future: F) -> Result<F::Item, F::Error>
    where
        F: Future01,
    {
        self.block_on_std(future.compat())
    }

    /// Run a `std::future` future to completion on the Tokio runtime.
    ///
    /// This runs the given future on the runtime, blocking until it is
    /// complete, and yielding its resolved result. Any tasks or timers which
    /// the future spawns internally will be executed on the runtime.
    ///
    /// This method should not be called from an asynchronous context.
    ///
    /// # Panics
    ///
    /// This function panics if the executor is at capacity, if the provided
    /// future panics, or if called within an asynchronous execution context.
    pub fn block_on_std<F>(&mut self, future: F) -> F::Output
    where
        F: Future,
    {
        let spawner = self.spawner();
        let inner = self.inner_mut();
        let compat = &inner.compat_bg;
        let _timer = timer_02::timer::set_default(compat.timer());
        let _reactor = reactor_01::set_default(compat.reactor());
        let _executor = executor_01::set_default(spawner);
        inner.runtime.block_on(future)
    }

    /// Signals the runtime to shutdown once it becomes idle.
    ///
    /// Blocks the current thread until the shutdown operation has completed.
    /// This function can be used to perform a graceful shutdown of the runtime.
    ///
    /// The runtime enters an idle state once **all** of the following occur.
    ///
    /// * The thread pool has no tasks to execute, i.e., all tasks that were
    ///   spawned have completed.
    /// * The reactor is not managing any I/O resources.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// **Note**: tasks spawned with associated [`JoinHandle`]s do _not_ "count"
    /// towards `shutdown_on_idle`. Since `shutdown_on_idle` does not exist in
    /// `tokio` 0.2, this is intended as a _transitional_ API; its use should be
    /// phased out in favor of waiting on `JoinHandle`s.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    /// # rt.spawn_std(async {});
    ///
    /// // Shutdown the runtime
    /// rt.shutdown_on_idle();
    /// ```
    ///
    /// [mod]: index.html
    /// [`JoinHandle`]: https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html
    pub fn shutdown_on_idle(mut self) {
        let spawner = self.spawner();
        let Inner {
            compat_bg,
            mut runtime,
        } = self.inner.take().expect("runtime is only shut down once");
        let _timer = timer_02::timer::set_default(compat_bg.timer());
        let _reactor = reactor_01::set_default(compat_bg.reactor());
        let _executor = executor_01::set_default(spawner);
        let idle = &mut self.idle_rx;
        runtime.block_on(idle.recv());
    }

    /// Signals the runtime to shutdown immediately.
    ///
    /// Blocks the current thread until the shutdown operation has completed.
    /// This function will forcibly shutdown the runtime, causing any
    /// in-progress work to become canceled.
    ///
    /// The shutdown steps are:
    ///
    /// * Drain any scheduled work queues.
    /// * Drop any futures that have not yet completed.
    /// * Drop the reactor.
    ///
    /// Once the reactor has dropped, any outstanding I/O resources bound to
    /// that reactor will no longer function. Calling any method on them will
    /// result in an error.
    ///
    /// See [module level][mod] documentation for more details.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_compat::runtime::Runtime;
    ///
    /// let rt = Runtime::new()
    ///     .unwrap();
    ///
    /// // Use the runtime...
    ///
    /// // Shutdown the runtime
    /// rt.shutdown_now();
    /// ```
    ///
    /// [mod]: index.html
    pub fn shutdown_now(mut self) {
        drop(self.inner.take().unwrap())
    }

    fn spawner(&self) -> CompatSpawner<Handle> {
        CompatSpawner {
            inner: self.inner().runtime.handle().clone(),
            idle: self.idle.clone(),
        }
    }

    fn inner(&self) -> &Inner {
        self.inner.as_ref().unwrap()
    }

    fn inner_mut(&mut self) -> &mut Inner {
        self.inner.as_mut().unwrap()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            drop(inner);
        }
    }
}
