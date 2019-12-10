use tokio_02::runtime::Handle;
use tokio_02::task::JoinHandle;
use tokio_executor_01::{self as executor_01, Executor as Executor01};

use futures_01::future::Future as Future01;
use futures_util::{compat::Future01CompatExt, future::FutureExt};
use std::future::Future;

/// Executes futures on the runtime
///
/// All futures spawned using this executor will be submitted to the associated
/// Runtime's executor. This executor is usually a thread pool.
///
/// For more details, see the [module level](index.html) documentation.
#[derive(Debug, Clone)]
#[cfg_attr(docsrs, doc(cfg(feature = "rt-full")))]
pub struct TaskExecutor {
    pub(super) inner: super::compat::CompatSpawner<Handle>,
}

impl TaskExecutor {
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
    pub fn spawn<F>(&self, future: F)
    where
        F: Future01<Item = (), Error = ()> + Send + 'static,
    {
        let future = Box::pin(future.compat().map(|_| ()));
        self.spawn_std(future)
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
    pub fn spawn_std<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let idle = self.inner.idle.reserve();
        let _ = self.inner.inner.spawn(idle.with(future));
    }

    /// Spawn a `futures` 0.1 future onto the Tokio runtime, returning a
    /// `JoinHandle` that can be used to await its result.
    ///
    /// This spawns the given future onto the runtime's executor, usually a
    /// thread pool. The thread pool is then responsible for polling the future
    /// until it completes.
    ///
    /// **Note** that futures spawned in this manner do not "count" towards
    /// keeping the runtime active for [`shutdown_on_idle`], since they are paired
    /// with a `JoinHandle` for  awaiting their completion. See [here] for
    /// details on shutting down the compatibility runtime.
    ///
    /// [mod]: index.html
    /// [`shutdown_on_idle`]: struct.Runtime.html#method.shutdown_on_idle
    /// [here]: index.html#shutting-down
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
    /// with a `JoinHandle` for  awaiting their completion. See [here] for
    /// details on shutting down the compatibility runtime.
    ///
    /// [mod]: index.html
    /// [`shutdown_on_idle`]: struct.Runtime.html#method.shutdown_on_idle
    /// [here]: index.html#shutting-down
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
    ///
    /// # Panics
    ///
    /// This function panics if the spawn fails. Failure occurs if the executor
    /// is currently at capacity and is unable to spawn a new future.
    pub fn spawn_handle_std<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.inner.spawn(future)
    }
}

impl Executor01 for TaskExecutor {
    fn spawn(
        &mut self,
        future: Box<dyn Future01<Item = (), Error = ()> + Send>,
    ) -> Result<(), executor_01::SpawnError> {
        Executor01::spawn(&mut self.inner, future)
    }
}

impl<T> executor_01::TypedExecutor<T> for TaskExecutor
where
    T: Future01<Item = (), Error = ()> + Send + 'static,
{
    fn spawn(&mut self, future: T) -> Result<(), executor_01::SpawnError> {
        executor_01::TypedExecutor::spawn(&mut self.inner, future)
    }
}
