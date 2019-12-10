use super::Runtime;
use futures_01::future::Future as Future01;
use futures_util::{compat::Future01CompatExt, FutureExt};
use std::future::Future;
use tokio_executor_01 as executor_01;

/// Executes futures on the current thread.
///
/// All futures executed using this executor will be executed on the current
/// thread. As such, `run` will wait for these futures to complete before
/// returning, if they are spawned without a `JoinHandle`.
///
/// For more details, see the [module level](index.html) documentation.
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "rt-current-thread")))]
pub struct TaskExecutor {
    _p: (),
}

// ===== impl TaskExecutor =====

impl TaskExecutor {
    /// Returns an executor that executes futures on the current thread.
    ///
    /// The user of `TaskExecutor` must ensure that when a future is submitted,
    /// that it is done within the context of a call to `run`.
    ///
    /// For more details, see the [module level](index.html) documentation.
    pub fn current() -> TaskExecutor {
        TaskExecutor { _p: () }
    }

    /// Spawn a `futures` 0.1 future onto the current `CurrentThread` instance.
    pub fn spawn_local(
        &mut self,
        future: impl Future01<Item = (), Error = ()> + 'static,
    ) -> Result<(), executor_01::SpawnError> {
        self.spawn_local_std(future.compat().map(|_| ()))
    }

    /// Spawn a `std::futures` future onto the current `CurrentThread`
    /// instance.
    pub fn spawn_local_std(
        &mut self,
        future: impl Future<Output = ()> + 'static,
    ) -> Result<(), executor_01::SpawnError> {
        if let Some(idle) = Runtime::reserve_idle() {
            tokio_02::task::spawn_local(idle.with(future));
            Ok(())
        } else {
            Err(executor_01::SpawnError::shutdown())
        }
    }

    /// Spawn a `futures` 0.1 future onto the current `CurrentThread` instance,
    /// returning a `JoinHandle` that can be used to await its result.
    ///
    /// **Note** that futures spawned in this manner do not "count" towards
    /// keeping the runtime active for [`shutdown_on_idle`], since they are paired
    /// with a `JoinHandle` for  awaiting their completion. See [here] for
    /// details on shutting down the compatibility runtime.
    ///
    /// [`shutdown_on_idle`]: struct.Runtime.html#method.shutdown_on_idle
    /// [here]: ../index.html#shutting-down
    ///
    /// # Panics
    ///
    /// This function panics if there is no current runtime, or if the current
    /// runtime is not a current-thread runtime.
    pub fn spawn_handle<T: 'static, E: 'static>(
        &mut self,
        future: impl Future01<Item = T, Error = E> + 'static,
    ) -> tokio_02::task::JoinHandle<Result<T, E>> {
        self.spawn_handle_std(future.compat())
    }

    /// Spawn a `std::future` future onto the current `CurrentThread` instance,
    /// returning a `JoinHandle` that can be used to await its result.
    ///
    /// **Note** that futures spawned in this manner do not "count" towards
    /// keeping the runtime active for [`shutdown_on_idle`], since they are paired
    /// with a `JoinHandle` for  awaiting their completion. See [here] for
    /// details on shutting down the compatibility runtime.
    ///
    /// [`shutdown_on_idle`]: struct.Runtime.html#method.shutdown_on_idle
    /// [here]: ../index.html#shutting-down
    ///
    /// # Panics
    ///
    /// This function panics if there is no current runtime, or if the current
    /// runtime is not a current-thread runtime.
    pub fn spawn_handle_std<T: 'static>(
        &mut self,
        future: impl Future<Output = T> + 'static,
    ) -> tokio_02::task::JoinHandle<T> {
        tokio_02::task::spawn_local(future)
    }
}

impl executor_01::Executor for TaskExecutor {
    fn spawn(
        &mut self,
        future: Box<dyn Future01<Item = (), Error = ()> + Send>,
    ) -> Result<(), executor_01::SpawnError> {
        self.spawn_local(future)
    }

    fn status(&self) -> Result<(), executor_01::SpawnError> {
        if Runtime::is_current() {
            Ok(())
        } else {
            Err(executor_01::SpawnError::shutdown())
        }
    }
}

impl<F> executor_01::TypedExecutor<F> for TaskExecutor
where
    F: Future01<Item = (), Error = ()> + 'static,
{
    fn spawn(&mut self, future: F) -> Result<(), executor_01::SpawnError> {
        self.spawn_local(Box::new(future))
    }

    fn status(&self) -> Result<(), executor_01::SpawnError> {
        executor_01::Executor::status(self)
    }
}
